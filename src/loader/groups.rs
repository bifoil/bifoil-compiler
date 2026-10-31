//! Cyclic interfaces converge before lowering; group fingerprints avoid circular hashes.

use super::{Loader, Node};
use crate::model::{Module, ModuleGroup, digest};
use crate::{budget, compiler, initialization};
use std::collections::BTreeMap;

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
fn group_fingerprint(modules: &mut BTreeMap<String, Module>, members: &[String]) -> Result<String, String> {
    let mut hashes = BTreeMap::new();
    for name in members {
        let module = modules.get_mut(name).unwrap();
        let saved_digest = std::mem::take(&mut module.digest);
        let saved_group = module.group.take();
        let mut dependencies = Vec::new();
        for member in members {
            if let Some(hash) = module.dependencies.get_mut(member) {
                dependencies.push((member, std::mem::take(hash)));
            }
        }
        let hash = digest(module);
        for (member, value) in dependencies {
            module.dependencies.insert(member.clone(), value);
        }
        module.digest = saved_digest;
        module.group = saved_group;
        hashes.insert(name, hash?);
    }
    digest(&hashes)
}
/// Published artifacts and disposable cache records are checked against the entire
/// component. A matching declaration alone cannot conceal a changed sibling body.
pub(super) fn reusable(nodes: &mut BTreeMap<String, Node>, completed: &BTreeMap<String, Module>, cyclic: bool) -> bool {
    let members = nodes.keys().cloned().collect::<Vec<_>>();
    let mut modules = BTreeMap::new();
    for node in nodes.values() {
        let Some(module) = &node.module else {
            return false;
        };
        if cyclic {
            if module.group.as_ref().is_none_or(|group| group.members != members) {
                return false;
            }
        } else if module.group.is_some() {
            return false;
        }
        for (dependency, hash) in &module.dependencies {
            if let Some(other) = nodes.get(dependency) {
                if !cyclic
                    || other.module.as_ref().and_then(|module| module.group.as_ref()) != module.group.as_ref()
                    || module.group.as_ref().is_none_or(|group| group.digest != *hash)
                {
                    return false;
                }
            } else if completed.get(dependency).is_none_or(|module| module.digest != *hash) {
                return false;
            }
        }
    }
    if !cyclic {
        return true;
    }
    for (name, node) in nodes.iter_mut() {
        modules.insert(name.clone(), node.module.take().unwrap());
    }
    let valid = group_fingerprint(&mut modules, &members).is_ok_and(|fingerprint| {
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
        for name in members {
            let node = nodes.get_mut(name).unwrap();
            self.modules.insert(name.clone(), compiler::seed(node.ast()?, name)?);
        }
        let maximum_rounds = 2 + nodes
            .values()
            .map(|node| {
                let ast = node.source.as_ref().unwrap().ast.as_ref().unwrap();
                1 + ast.functions.len()
                    + ast.globals.len()
                    + ast.aliases.len()
                    + ast
                        .types
                        .iter()
                        .map(|definition| 1 + definition.methods.len() + definition.variants.len())
                        .sum::<usize>()
            })
            .sum::<usize>();
        for declarations_only in [true, false] {
            let mut converged = false;
            for _ in 0..maximum_rounds {
                let mut changed = false;
                let mut error = None;
                for name in members {
                    budget(self.start)?;
                    let node = nodes.get_mut(name).unwrap();
                    let logical = node.source.as_ref().unwrap().logical.clone();
                    let ast = node.ast()?;
                    let scope = crate::scope::for_ast(ast, name, &self.modules)?;
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
            let node = nodes.get_mut(name).unwrap();
            let logical = node.source.as_ref().unwrap().logical.clone();
            let ast = node.ast()?;
            let scope = crate::scope::for_ast(ast, name, &self.modules)?;
            let ast = node.take_ast()?;
            let module = compiler::compile_group_member(ast, name, &logical, &scope, self.start)?;
            lowered.insert(name.clone(), module);
        }
        self.modules.extend(lowered);
        // Every signature and binding now exists, so summaries can be computed
        // without retaining dependency instruction bodies during later loading.
        for name in members {
            let mut module = self.modules.remove(name).unwrap();
            let scope = crate::scope::for_module(&module, &self.modules)?;
            module.summaries = initialization::summarize(&module.functions, &module.shared, &scope, self.start)?;
            self.modules.insert(name.clone(), module);
        }
        // Analyze real bodies only after every member is available. Shared cycles
        // remain errors even though declaration/import cycles are now permitted.
        for name in members {
            let mut module = self.modules.remove(name).unwrap();
            let scope = crate::scope::for_module(&module, &self.modules)?;
            module.summaries = initialization::discover(&module.functions, &mut module.shared, &scope, self.start)?;
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
        let fingerprint = group_fingerprint(&mut self.modules, members)?;
        for name in members {
            let mut module = self.modules.remove(name).unwrap();
            module.group = Some(ModuleGroup {
                members: members.to_vec(),
                digest: fingerprint.clone(),
            });
            for member in members {
                if let Some(hash) = module.dependencies.get_mut(member) {
                    *hash = fingerprint.clone();
                }
            }
            module.digest.clear();
            module.digest = digest(&module)?;
            nodes.get_mut(name).unwrap().module = Some(module);
        }
        Ok(())
    }
}
