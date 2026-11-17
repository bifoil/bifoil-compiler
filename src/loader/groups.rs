//! Cyclic interfaces converge before lowering; group fingerprints avoid circular hashes.

use super::{Loader, Node};
use crate::model::{Module, ModuleGroup, digest};
use crate::{budget, compiler, initialization};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Compare declaration metadata, including unresolved types during inference.
/// Instruction bodies and content digests are intentionally absent from this phase.
fn same_interface(left: &Module, right: &Module) -> bool {
    left.exports == right.exports
        && left.types == right.types
        && left.legacy_globals == right.legacy_globals
        && left.functions.len() == right.functions.len()
        && left.functions.iter().all(|(id, function)| {
            right
                .functions
                .get(id)
                .is_some_and(|other| function.signature == other.signature)
        })
}
fn group_fingerprint(
    modules: &mut BTreeMap<String, Module>,
    members: &[String],
    code: &crate::code_store::CodeStore,
    started: std::time::Instant,
) -> Result<String, String> {
    let mut hashes = BTreeMap::new();
    let member_names = members.iter().map(String::as_str).collect::<BTreeSet<_>>();
    for name in members {
        budget(started)?;
        let mut module = modules.remove(name).unwrap();
        let saved_digest = std::mem::take(&mut module.digest);
        let saved_group = module.group.take();
        let mut dependencies = Vec::new();
        // Visit actual edges rather than probing every component member for
        // every module. Sparse cycles should not require quadratic lookups.
        for (dependency, hash) in &mut module.dependencies {
            if member_names.contains(dependency.as_str()) {
                dependencies.push((dependency.clone(), std::mem::take(hash)));
            }
        }
        let (mut module, hash) = code.inspect_module(module, |stored| digest(stored));
        for (member, value) in dependencies {
            module.dependencies.insert(member, value);
        }
        module.digest = saved_digest;
        module.group = saved_group;
        modules.insert(name.clone(), module);
        hashes.insert(name, hash?);
    }
    digest(&hashes)
}
/// Published artifacts and disposable cache records are checked against the entire
/// component. A matching declaration alone cannot conceal a changed sibling body.
pub(super) fn reusable(
    nodes: &mut BTreeMap<String, Node>,
    completed: &BTreeMap<String, Module>,
    cyclic: bool,
    code: &crate::code_store::CodeStore,
    started: std::time::Instant,
) -> Result<bool, String> {
    let members = nodes.keys().cloned().collect::<Vec<_>>();
    let expected_group = nodes
        .values()
        .next()
        .and_then(|node| node.module.as_ref())
        .and_then(|module| module.group.as_ref());
    if cyclic && expected_group.is_none_or(|group| group.members.as_slice() != members.as_slice()) {
        return Ok(false);
    }
    let mut modules = BTreeMap::new();
    for node in nodes.values() {
        let Some(module) = &node.module else {
            return Ok(false);
        };
        if cyclic {
            if module
                .group
                .as_ref()
                .is_none_or(|group| !super::memberships::same_identity(group, expected_group.unwrap()))
            {
                return Ok(false);
            }
        } else if module.group.is_some() {
            return Ok(false);
        }
        for (dependency, hash) in &module.dependencies {
            if nodes.contains_key(dependency) {
                // Every member is checked against the same identity above.
                // An internal edge then needs only its component digest.
                if !cyclic || expected_group.is_none_or(|group| group.digest != *hash) {
                    return Ok(false);
                }
            } else if completed.get(dependency).is_none_or(|module| module.digest != *hash) {
                return Ok(false);
            }
        }
    }
    if !cyclic {
        return Ok(true);
    }
    for (name, node) in nodes.iter_mut() {
        modules.insert(name.clone(), node.module.take().unwrap());
    }
    let valid = group_fingerprint(&mut modules, &members, code, started).map(|fingerprint| {
        modules
            .values()
            .all(|module| module.group.as_ref().is_some_and(|group| group.digest == fingerprint))
    });
    for (name, module) in modules {
        nodes.get_mut(&name).unwrap().module = Some(module);
    }
    valid
}
impl Loader {
    pub(super) fn compile_group(
        &mut self,
        members: &[String],
        nodes: &mut BTreeMap<String, Node>,
    ) -> Result<(), String> {
        let mut maximum_rounds = 2;
        for name in members {
            let ast = nodes[name].read_ast()?;
            maximum_rounds += 1
                + ast.functions.len()
                + ast.globals.len()
                + ast.aliases.len()
                + ast
                    .types
                    .iter()
                    .map(|definition| 1 + definition.methods.len() + definition.variants.len())
                    .sum::<usize>();
            self.modules.insert(name.clone(), compiler::seed(&ast, name)?);
        }
        for declarations_only in [true, false] {
            let mut converged = false;
            for _ in 0..maximum_rounds {
                let mut changed = false;
                let mut error = None;
                for name in members {
                    budget(self.start)?;
                    let node = &nodes[name];
                    let logical = node.source.as_ref().unwrap().logical.clone();
                    let ast = node.read_ast()?;
                    let scope = crate::scope::for_ast(&ast, name, &self.modules)?;
                    let resolved = if declarations_only {
                        compiler::declaration_interface(ast, name, &scope)
                    } else {
                        compiler::interface(ast, name, &logical, &scope, self.start)
                    };
                    match resolved {
                        Ok(interface) => {
                            changed |= !same_interface(&self.modules[name], &interface);
                            self.modules.insert(name.clone(), interface);
                        }
                        Err(message) => error = Some(format!("{logical}: {message}")),
                    }
                }
                if !changed {
                    if let Some(error) = error {
                        return Err(error);
                    }
                    converged = true;
                    break;
                }
            }
            if !converged {
                return Err("cyclic module interfaces do not converge; add explicit result/type annotations".into());
            }
        }
        let mut lowered = BTreeMap::new();
        for name in members {
            let node = &nodes[name];
            let logical = node.source.as_ref().unwrap().logical.clone();
            let ast = node.read_ast()?;
            let scope = crate::scope::for_ast(&ast, name, &self.modules)?;
            let mut module = compiler::compile_group_member(ast, name, &logical, &scope, self.start)?;
            self.pending_code.record_pending(&mut module, self.start)?;
            lowered.insert(name.clone(), module);
        }
        self.modules.extend(lowered);
        // Every signature and binding now exists, so summaries can be computed
        // without retaining dependency instruction bodies during later loading.
        for name in members {
            let mut module = self.modules.remove(name).unwrap();
            let scope = crate::scope::for_module(&module, &self.modules)?;
            let analysis = initialization::SummaryContext::new(&module.functions, &module.shared, &scope, self.start);
            let mut summaries = BTreeMap::new();
            self.pending_code.inspect_functions(
                &module.functions,
                self.start,
                initialization::SummaryContext::requires_body,
                |id, function| {
                    if let Some(summary) = analysis.analyze(function)? {
                        summaries.insert(id.to_owned(), summary);
                    }
                    Ok(())
                },
            )?;
            module.summaries = summaries;
            self.modules.insert(name.clone(), module);
        }
        // Every member now has checked summaries. Dependency resolution needs
        // metadata only; shared cycles remain errors across declaration cycles.
        for name in members {
            let mut module = self.modules.remove(name).unwrap();
            let scope = crate::scope::for_module(&module, &self.modules)?;
            initialization::resolve_module(
                &module.functions,
                &mut module.shared,
                &module.summaries,
                &scope,
                self.start,
            )?;
            self.modules.insert(name.clone(), module);
        }
        let all_shared = self
            .modules
            .values()
            .flat_map(|module| {
                module
                    .shared
                    .iter()
                    .map(|(id, definition)| (id.clone(), definition.clone()))
            })
            .collect();
        initialization::order(&all_shared, self.start)?;
        let fingerprint = group_fingerprint(&mut self.modules, members, &self.pending_code, self.start)?;
        let mut group = Arc::new(ModuleGroup {
            members: members.to_vec(),
            digest: fingerprint.clone(),
        });
        self.memberships.share(&mut group);
        let member_names = members.iter().map(String::as_str).collect::<BTreeSet<_>>();
        for name in members {
            let mut module = self.modules.remove(name).unwrap();
            module.group = Some(group.clone());
            for (dependency, hash) in &mut module.dependencies {
                if member_names.contains(dependency.as_str()) {
                    hash.clone_from(&fingerprint);
                }
            }
            module.digest.clear();
            let (mut module, checksum) = self.pending_code.inspect_module(module, |stored| digest(stored));
            module.digest = checksum?;
            let node = nodes.get_mut(name).unwrap();
            node.module = Some(module);
            node.stored = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn member(name: &str, peer: &str, result: i32) -> Module {
        let source = format!("export fun value() -> Int = {result}\n");
        let ast = crate::parser::parse(&source, "src/member.bfl").unwrap();
        let mut module = compiler::compile(ast, name, "src/member.bfl", &BTreeMap::new(), Instant::now()).unwrap();
        module.dependencies.insert(peer.into(), "previous-group".into());
        module.dependencies.insert("outside".into(), "external-content".into());
        module.group = Some(Arc::new(ModuleGroup {
            members: vec!["alpha".into(), "beta".into()],
            digest: "previous-group".into(),
        }));
        module
    }

    fn fingerprint(modules: &mut BTreeMap<String, Module>, names: &[String]) -> Result<String, String> {
        let mut code = crate::code_store::CodeStore::default();
        for module in modules.values_mut() {
            code.record_pending(module, Instant::now())?;
        }
        let result = group_fingerprint(modules, names, &code, Instant::now());
        for module in modules.values_mut() {
            code.restore_pending(module, Instant::now())?;
        }
        result
    }

    #[test]
    fn component_fingerprints_preserve_canonical_hashes_and_input_metadata() {
        let names: Vec<String> = vec!["alpha".into(), "beta".into()];
        let mut modules = BTreeMap::from([
            ("alpha".into(), member("alpha", "beta", 1)),
            ("beta".into(), member("beta", "alpha", 2)),
        ]);
        let before = serde_json::to_vec(&modules).unwrap();
        let mut expected = BTreeMap::new();
        for name in &names {
            let mut module = modules[name].clone();
            module.digest.clear();
            module.group = None;
            for peer in &names {
                if let Some(hash) = module.dependencies.get_mut(peer) {
                    hash.clear();
                }
            }
            expected.insert(name.clone(), digest(&module).unwrap());
        }
        assert_eq!(fingerprint(&mut modules, &names).unwrap(), digest(&expected).unwrap());
        assert_eq!(serde_json::to_vec(&modules).unwrap(), before);
    }

    #[test]
    fn component_fingerprints_include_external_edges_and_every_member_body() {
        let names: Vec<String> = vec!["alpha".into(), "beta".into()];
        let mut modules = BTreeMap::from([
            ("alpha".into(), member("alpha", "beta", 1)),
            ("beta".into(), member("beta", "alpha", 2)),
        ]);
        let original = fingerprint(&mut modules, &names).unwrap();
        modules
            .get_mut("alpha")
            .unwrap()
            .dependencies
            .insert("beta".into(), "replacement-group".into());
        modules.get_mut("beta").unwrap().digest = "replacement-digest".into();
        assert_eq!(fingerprint(&mut modules, &names).unwrap(), original);
        modules
            .get_mut("alpha")
            .unwrap()
            .dependencies
            .insert("outside".into(), "changed-external-content".into());
        assert_ne!(fingerprint(&mut modules, &names).unwrap(), original);
        modules.insert("alpha".into(), member("alpha", "beta", 1));
        modules.insert("beta".into(), member("beta", "alpha", 3));
        assert_ne!(fingerprint(&mut modules, &names).unwrap(), original);
    }

    #[test]
    fn failed_stored_fingerprinting_returns_every_member_with_its_original_metadata() {
        let names: Vec<String> = vec!["alpha".into(), "beta".into()];
        let mut modules = BTreeMap::from([
            ("alpha".into(), member("alpha", "beta", 1)),
            ("beta".into(), member("beta", "alpha", 2)),
        ]);
        let mut code = crate::code_store::CodeStore::default();
        for module in modules.values_mut() {
            code.record_pending(module, Instant::now()).unwrap();
        }
        let header = &mut modules
            .get_mut("alpha")
            .unwrap()
            .functions
            .get_mut("alpha.value")
            .unwrap()
            .signature;
        std::sync::Arc::make_mut(header).result = crate::model::Type::Float;
        let expected = serde_json::to_vec(&modules).unwrap();
        let error = group_fingerprint(&mut modules, &names, &code, Instant::now()).unwrap_err();
        assert!(error.contains("header"), "{error}");
        assert_eq!(serde_json::to_vec(&modules).unwrap(), expected);
        assert!(
            modules
                .values()
                .all(|module| module.functions.values().all(|function| function.code.is_empty()))
        );
    }

    #[test]
    fn shared_identity_validation_rejects_conflicting_members_even_with_the_same_claimed_digest() {
        let names: Vec<String> = vec!["alpha".into(), "beta".into()];
        let mut modules = BTreeMap::from([
            ("alpha".into(), member("alpha", "beta", 1)),
            ("beta".into(), member("beta", "alpha", 2)),
        ]);
        let mut code = crate::code_store::CodeStore::default();
        for module in modules.values_mut() {
            module.dependencies.remove("outside");
            code.record_pending(module, Instant::now()).unwrap();
        }
        let fingerprint = group_fingerprint(&mut modules, &names, &code, Instant::now()).unwrap();
        let group = Arc::new(ModuleGroup {
            members: names.clone(),
            digest: fingerprint.clone(),
        });
        let mut nodes = BTreeMap::new();
        for (name, mut module) in modules {
            module.group = Some(Arc::clone(&group));
            for hash in module.dependencies.values_mut() {
                hash.clone_from(&fingerprint);
            }
            module.digest.clear();
            let (mut module, hash) = code.inspect_module(module, |stored| digest(stored));
            module.digest = hash.unwrap();
            let dependencies = module.dependencies.keys().cloned().collect();
            nodes.insert(
                name,
                Node {
                    source: None,
                    module: Some(module),
                    artifact_hash: None,
                    stored: true,
                    dependencies,
                },
            );
        }
        assert!(reusable(&mut nodes, &BTreeMap::new(), true, &code, Instant::now()).unwrap());
        let conflicting = nodes
            .get_mut("beta")
            .unwrap()
            .module
            .as_mut()
            .unwrap()
            .group
            .as_mut()
            .unwrap();
        Arc::make_mut(conflicting).members = vec!["alpha".into(), "other".into()];
        let mut memberships = crate::loader::memberships::Memberships::default();
        for node in nodes.values_mut() {
            memberships.share(node.module.as_mut().unwrap().group.as_mut().unwrap());
        }
        let encode = |nodes: &BTreeMap<String, Node>| {
            serde_json::to_vec(
                &nodes
                    .iter()
                    .map(|(name, node)| (name, node.module.as_ref().unwrap()))
                    .collect::<BTreeMap<_, _>>(),
            )
            .unwrap()
        };
        let expected = encode(&nodes);
        let instructions = code.instructions;
        assert!(!reusable(&mut nodes, &BTreeMap::new(), true, &code, Instant::now()).unwrap());
        assert_eq!(encode(&nodes), expected);
        assert_eq!(code.instructions, instructions);
    }
}
