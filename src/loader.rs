//! Discover dependency components, then compile against complete declaration interfaces.
//!
//! Tarjan's stack contains only unfinished modules. Completed components retain
//! the existing dependency-first order and disposable-cache behavior.

mod groups;
mod identities;
mod inputs;
mod legacy;
mod memberships;
mod traversal;

use crate::model::{MAX_ARTIFACT, MAX_PROGRAM_FUNCTIONS, MAX_PROGRAM_INSTRUCTIONS, MAX_SOURCE, Module};
use crate::syntax::Ast;
use crate::{MAX_MODULES, budget, cache, compiler, module_name, parser, read};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

struct Source {
    path: PathBuf,
    hash: String,
    logical: String,
}
struct Node {
    source: Option<Source>,
    module: Option<Module>,
    artifact_hash: Option<String>,
    stored: bool,
    dependencies: Vec<String>,
}
impl Node {
    /// Each pass owns only its active source AST. Revalidate the discovery hash
    /// before parsing again so interfaces cannot silently outlive a source edit.
    fn read_ast(&self) -> Result<Ast, String> {
        let source = self
            .source
            .as_ref()
            .ok_or("module source is required for recompilation")?;
        // Discovery already accounted for this file in the aggregate input budget.
        let mut reread_bytes = 0;
        let text = read(&source.path, MAX_SOURCE as u64, &mut reread_bytes)?;
        if cache::Cache::source_hash(&text) != source.hash {
            return Err(format!(
                "{}: source changed after dependency discovery; retry compilation",
                source.path.display()
            ));
        }
        parser::parse(&text, &source.logical)
            .map_err(|error| crate::diagnostics::at_source(&source.logical, None, error))
    }
}

pub(crate) struct Loader {
    root: PathBuf,
    artifacts: PathBuf,
    pub(crate) modules: BTreeMap<String, Module>,
    pub(crate) code: crate::code_store::CodeStore,
    pending_code: crate::code_store::CodeStore,
    pub(crate) inputs: BTreeMap<String, cache::ModuleInput>,
    pub(crate) order: Vec<String>,
    pub(crate) total: u64,
    start: Instant,
    pub(crate) cache: Option<cache::Cache>,
    nodes: BTreeMap<String, Node>,
    prefetched: inputs::Inputs,
    traversal: traversal::Traversal,
    memberships: memberships::Memberships,
}
impl Loader {
    pub(crate) fn new(root: PathBuf, artifacts: PathBuf, start: Instant) -> Self {
        Self {
            root,
            artifacts,
            modules: BTreeMap::new(),
            code: crate::code_store::CodeStore::default(),
            pending_code: crate::code_store::CodeStore::default(),
            inputs: BTreeMap::new(),
            order: vec![],
            total: 0,
            start,
            cache: None,
            nodes: BTreeMap::new(),
            prefetched: inputs::Inputs::default(),
            traversal: traversal::Traversal::default(),
            memberships: memberships::Memberships::default(),
        }
    }
    pub(crate) fn load(&mut self, name: &str, explicit: Option<&Path>) -> Result<(), String> {
        if self.traversal.contains(name) {
            self.validate_legacy_interfaces()?;
            return self.validate_runtime_identities();
        }
        self.discover(name, explicit)?;
        while let Some(step) = self.traversal.next() {
            budget(self.start)?;
            match step {
                traversal::Step::Discover(name) => self.discover(&name, None)?,
                traversal::Step::Component(members) => self.finish(&members)?,
            }
        }
        self.validate_legacy_interfaces()?;
        self.validate_runtime_identities()
    }
    fn read_node(&mut self, name: &str, explicit: Option<&Path>) -> Result<Node, String> {
        let path = explicit
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.root.join(name.replace('.', "/") + ".bfl"));
        let prepared = self.prefetched.take(name);
        if path.is_file() {
            let (text, cached) = match prepared {
                Some(inputs::Prepared::Source { text, cached }) => {
                    crate::input::account(&path, text.len() as u64, MAX_SOURCE as u64, Some(&mut self.total))?;
                    (text, cached)
                }
                _ => (read(&path, MAX_SOURCE as u64, &mut self.total)?, None),
            };
            let hash = cache::Cache::source_hash(&text);
            let mut module = match &mut self.cache {
                Some(cache) => match cached {
                    Some(bytes) => cache.load_bytes(name, &hash, &bytes, &mut self.pending_code, self.start)?,
                    None => cache.load(name, &hash, &mut self.pending_code, self.start)?,
                },
                None => None,
            };
            if let Some(group) = module.as_mut().and_then(|module| module.group.as_mut()) {
                self.memberships.share(group);
            }
            let logical = format!("src/{}.bfl", name.replace('.', "/"));
            let mut dependencies: Vec<String> = if let Some(module) = &module {
                module.dependencies.keys().cloned().collect()
            } else {
                parser::imports(&text, &logical)
                    .map_err(|error| crate::diagnostics::at_source(&logical, None, error))?
                    .into_iter()
                    .map(|import| import.module)
                    .collect()
            };
            // Discovery order must not depend on source import spelling versus
            // the ordered dependency map in an independently compiled artifact.
            dependencies.sort();
            dependencies.dedup();
            let stored = module.is_some();
            Ok(Node {
                source: Some(Source { path, hash, logical }),
                module,
                artifact_hash: None,
                stored,
                dependencies,
            })
        } else {
            let nested = self.artifacts.join(name.replace('.', "/") + ".bfm.json");
            let artifact = if nested.is_file() {
                nested
            } else {
                self.artifacts.join(format!("{name}.bfm.json"))
            };
            let (module, checksum) = match prepared {
                Some(inputs::Prepared::Artifact { path, bytes }) if path == artifact => {
                    crate::input::account(&artifact, bytes.len() as u64, MAX_ARTIFACT, Some(&mut self.total))?;
                    crate::input::json_seed_bytes_hashed(
                        &bytes,
                        MAX_ARTIFACT,
                        crate::module_input::streamed::Seed::new(self.start),
                    )
                    .map_err(|error| format!("{}: {error}", artifact.display()))?
                }
                _ => crate::module_input::streamed::read(&artifact, MAX_ARTIFACT, Some(&mut self.total), self.start)?,
            };
            if module.name != name {
                return Err("module artifact name mismatch".into());
            }
            let dependencies = module.dependencies.keys().cloned().collect();
            let mut module = crate::module_input::check(module)?.store(&mut self.pending_code, self.start)?;
            if let Some(group) = &mut module.group {
                self.memberships.share(group);
            }
            Ok(Node {
                source: None,
                module: Some(module),
                artifact_hash: Some(checksum),
                stored: true,
                dependencies,
            })
        }
    }
    fn discover(&mut self, name: &str, explicit: Option<&Path>) -> Result<(), String> {
        budget(self.start)?;
        if !module_name(name) {
            return Err(format!("invalid module name '{name}'"));
        }
        if self.traversal.len() >= MAX_MODULES {
            return Err("module count budget exceeded".into());
        }
        crate::trace::event(self.start, format_args!("reading {name}"));
        let node = self.read_node(name, explicit)?;
        crate::trace::event(self.start, format_args!("read {name}"));
        self.traversal.enter(name.into(), node.dependencies.clone());
        let dependencies = node
            .dependencies
            .iter()
            .filter(|name| !self.traversal.contains(name))
            .cloned()
            .collect::<Vec<_>>();
        self.nodes.insert(name.into(), node);
        self.prefetched.prepare(
            &self.root,
            &self.artifacts,
            self.cache.as_ref(),
            &dependencies,
            self.start,
        )?;
        Ok(())
    }
    fn finish(&mut self, members: &[String]) -> Result<(), String> {
        crate::trace::event(self.start, format_args!("completing {members:?}"));
        let mut nodes = members
            .iter()
            .map(|name| (name.clone(), self.nodes.remove(name).unwrap()))
            .collect::<BTreeMap<_, _>>();
        let cyclic = members.len() > 1 || nodes[&members[0]].dependencies.contains(&members[0]);
        let reusable = groups::reusable(&mut nodes, &self.modules, cyclic, &self.pending_code, self.start)?;
        if !reusable {
            for (name, node) in &nodes {
                if node.source.is_none() {
                    return Err(format!(
                        "module {name}: dependency or cyclic module group has changed; recompile the module"
                    ));
                }
            }
            if cyclic {
                // Reuse has failed and every member has source. Cached bodies
                // and interfaces must not coexist with their replacements.
                for node in nodes.values_mut() {
                    if node.stored {
                        self.pending_code
                            .discard_pending(node.module.as_ref().unwrap(), self.start)?;
                        node.stored = false;
                    }
                    node.module = None;
                }
                self.compile_group(members, &mut nodes)?;
            } else {
                let name = &members[0];
                let node = nodes.get_mut(name).unwrap();
                if node.stored {
                    self.pending_code
                        .discard_pending(node.module.as_ref().unwrap(), self.start)?;
                    node.stored = false;
                }
                // Dependency validation has already rejected these interfaces.
                // Release them before constructing the replacement AST/metadata.
                node.module = None;
                let logical = node.source.as_ref().unwrap().logical.clone();
                let ast = node.read_ast()?;
                let scope = crate::scope::for_ast(&ast, name, &self.modules)?;
                crate::trace::event(self.start, format_args!("compiling {name}"));
                let module = compiler::compile(ast, name, &logical, &scope, self.start)?;
                crate::trace::event(self.start, format_args!("compiled {name}"));
                node.module = Some(module);
            }
        }
        // Make every component header available before summarizing any member.
        for name in members {
            self.modules
                .insert(name.clone(), nodes.get_mut(name).unwrap().module.take().unwrap());
        }
        for name in members {
            let node = nodes.remove(name).unwrap();
            let mut module = self.modules.remove(name).unwrap();
            if node.stored {
                let scope = crate::scope::for_module(&module, &self.modules)?;
                let analysis =
                    crate::initialization::SummaryContext::new(&module.functions, &module.shared, &scope, self.start);
                let mut summaries = BTreeMap::new();
                self.code.transfer_pending(
                    &mut self.pending_code,
                    &module.functions,
                    self.start,
                    crate::initialization::SummaryContext::requires_body,
                    |id, function| {
                        if let Some(summary) = analysis.analyze(function)? {
                            summaries.insert(id.to_owned(), summary);
                        }
                        Ok(())
                    },
                )?;
                module.summaries = summaries;
            } else if module.summaries.is_empty() {
                let scope = crate::scope::for_module(&module, &self.modules)?;
                module.summaries =
                    crate::initialization::summarize(&module.functions, &module.shared, &scope, self.start)?;
            }
            if !node.stored {
                self.code.record(&mut module, self.start)?;
            }
            if let (Some(source), Some(cache)) = (&node.source, &mut self.cache) {
                if reusable {
                    // Reusing validated records is incremental work even when
                    // the optional completed-build marker is absent or corrupt.
                    crate::INCREMENTAL_DEADLINE.set(true);
                    budget(self.start)?;
                    cache.reused += 1;
                } else {
                    let (restored, published) = self
                        .code
                        .inspect_module(module, |stored| cache.store(stored, &source.hash, self.start));
                    module = restored;
                    published?;
                    cache.compiled += 1;
                }
            }
            if let (Some(source), Some(cache)) = (&node.source, &self.cache) {
                let checksum = cache.observed(name).ok_or("missing validated module-cache identity")?;
                self.inputs.insert(
                    name.clone(),
                    cache::ModuleInput::Source {
                        source_sha256: source.hash.clone(),
                        cache_sha256: checksum.into(),
                    },
                );
            } else if let Some(checksum) = node.artifact_hash {
                self.inputs.insert(
                    name.clone(),
                    cache::ModuleInput::Artifact {
                        artifact_sha256: checksum,
                    },
                );
            }
            self.modules.insert(name.clone(), module);
            self.order.push(name.clone());
        }
        budget(self.start)?;
        crate::trace::event(self.start, format_args!("stored {members:?}"));
        let functions: usize = self.modules.values().map(|module| module.functions.len()).sum();
        let instructions = self.code.instructions;
        if functions > MAX_PROGRAM_FUNCTIONS || instructions > MAX_PROGRAM_INSTRUCTIONS {
            return Err("program complexity budget exceeded".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct SourceDirectory(PathBuf);
    impl SourceDirectory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "bifoil-source-discovery-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for SourceDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn dependency_discovery_defers_ast_and_rejects_changed_source() {
        let directory = SourceDirectory::new();
        let path = directory.0.join("main.bfl");
        let text = "import helpers.value\nscene start():\n    val result = value + 1\n";
        fs::write(&path, text).unwrap();
        let mut loader = Loader::new(directory.0.clone(), directory.0.join("modules"), Instant::now());
        let unchanged = loader.read_node("main", None).unwrap();
        assert_eq!(unchanged.dependencies, ["helpers"]);
        assert_eq!(unchanged.read_ast().unwrap().functions.len(), 1);

        let changed = loader.read_node("main", None).unwrap();
        fs::write(&path, text.replace("+ 1", "+ 2")).unwrap();
        let error = changed.read_ast().err().unwrap();
        assert!(error.contains("source changed after dependency discovery; retry compilation"));
    }

    #[test]
    fn each_interface_pass_rechecks_source_identity() {
        let directory = SourceDirectory::new();
        let path = directory.0.join("main.bfl");
        let text = "fun value() -> Int = 1\n";
        fs::write(&path, text).unwrap();
        let mut loader = Loader::new(directory.0.clone(), directory.0.join("modules"), Instant::now());
        let node = loader.read_node("main", None).unwrap();
        assert_eq!(node.read_ast().unwrap().functions.len(), 1);
        assert_eq!(node.read_ast().unwrap().functions.len(), 1);
        fs::write(&path, text.replace("= 1", "= 2")).unwrap();
        let error = node.read_ast().err().unwrap();
        assert!(error.contains("source changed after dependency discovery; retry compilation"));
    }

    #[test]
    fn an_interface_pass_cannot_reuse_a_removed_source_ast() {
        let directory = SourceDirectory::new();
        let path = directory.0.join("main.bfl");
        fs::write(&path, "fun value() -> Int = 1\n").unwrap();
        let mut loader = Loader::new(directory.0.clone(), directory.0.join("modules"), Instant::now());
        let node = loader.read_node("main", None).unwrap();
        assert_eq!(node.read_ast().unwrap().functions.len(), 1);
        fs::remove_file(&path).unwrap();
        let error = node.read_ast().err().unwrap();
        assert!(error.contains("main.bfl"), "{error}");
    }

    #[test]
    fn cyclic_lowering_keeps_bodies_spooled_while_resolving_cross_member_shared_dependencies() {
        let directory = SourceDirectory::new();
        fs::write(directory.0.join("a.bfl"),
            "import b.read as peerRead\nexport shared val base: Int = 3\nexport fun read() -> Int = base\nexport fun through() -> Int = peerRead()\n").unwrap();
        fs::write(directory.0.join("b.bfl"),
            "import a.read as peerRead\nexport shared val total: Int = peerRead() + 2\nexport fun read() -> Int = total\n").unwrap();
        let mut loader = Loader::new(directory.0.clone(), directory.0.join("modules"), Instant::now());
        let names: Vec<String> = vec!["a".into(), "b".into()];
        let mut nodes = BTreeMap::from([
            ("a".into(), loader.read_node("a", None).unwrap()),
            ("b".into(), loader.read_node("b", None).unwrap()),
        ]);
        loader.compile_group(&names, &mut nodes).unwrap();
        assert!(loader.pending_code.instructions > 0);
        assert_eq!(loader.code.instructions, 0);
        assert!(nodes.values().all(|node| node.stored));
        let total = &nodes["b"].module.as_ref().unwrap().shared["b.total"];
        assert_eq!(total.dependencies.as_ref(), ["a.base"]);
        for node in nodes.values_mut() {
            let module = node.module.take().unwrap();
            assert!(module.functions.values().all(|function| function.code.is_empty()));
            let (returned, serialized) = loader.pending_code.inspect_module(module, |stored| {
                serde_json::to_vec(stored).map_err(|error| error.to_string())
            });
            let mut decoded: Module = serde_json::from_slice(&serialized.unwrap()).unwrap();
            let expected_digest = std::mem::take(&mut decoded.digest);
            assert_eq!(crate::model::digest(&decoded).unwrap(), expected_digest);
            node.module = Some(returned);
        }
    }

    #[test]
    fn fresh_cached_and_source_free_components_share_identity_and_preserve_module_bytes() {
        use std::sync::Arc;

        struct RestoreDeadline(bool);
        impl Drop for RestoreDeadline {
            fn drop(&mut self) {
                crate::INCREMENTAL_DEADLINE.set(self.0);
            }
        }
        let _deadline = RestoreDeadline(crate::INCREMENTAL_DEADLINE.replace(false));
        let directory = SourceDirectory::new();
        let sources = directory.0.join("src");
        let artifacts = directory.0.join("modules");
        let cache = directory.0.join("cache");
        for path in [&sources, &artifacts, &cache] {
            fs::create_dir(path).unwrap();
        }
        let names = ["alpha", "beta", "gamma"];
        for (index, name) in names.iter().enumerate() {
            let peer = names[(index + 1) % names.len()];
            let source = format!("import {peer}.read as peerRead\nexport fun read() -> Int = {index}\n");
            fs::write(sources.join(format!("{name}.bfl")), source).unwrap();
        }

        fn encode(loader: &mut Loader) -> BTreeMap<String, Vec<u8>> {
            let group = Arc::clone(loader.modules.values().next().unwrap().group.as_ref().unwrap());
            assert_eq!(group.members, ["alpha", "beta", "gamma"]);
            let names = loader.modules.keys().cloned().collect::<Vec<_>>();
            let mut result = BTreeMap::new();
            for name in names {
                let module = loader.modules.remove(&name).unwrap();
                assert!(Arc::ptr_eq(module.group.as_ref().unwrap(), &group), "{name}");
                let (module, bytes) = loader.code.inspect_module(module, |stored| {
                    serde_json::to_vec(stored).map_err(|error| error.to_string())
                });
                loader.modules.insert(name.clone(), module);
                result.insert(name, bytes.unwrap());
            }
            result
        }

        let mut fresh = Loader::new(sources.clone(), artifacts.clone(), Instant::now());
        fresh.cache = Some(cache::Cache::new(cache.clone()).unwrap());
        fresh.load("alpha", None).unwrap();
        assert_eq!(fresh.cache.as_ref().unwrap().compiled, 3);
        let expected = encode(&mut fresh);
        for (name, bytes) in &expected {
            fs::write(artifacts.join(format!("{name}.bfm.json")), bytes).unwrap();
        }
        drop(fresh);

        let mut cached = Loader::new(sources.clone(), artifacts.clone(), Instant::now());
        cached.cache = Some(cache::Cache::new(cache).unwrap());
        cached.load("alpha", None).unwrap();
        assert_eq!(cached.cache.as_ref().unwrap().compiled, 0);
        assert_eq!(cached.cache.as_ref().unwrap().reused, 3);
        assert_eq!(encode(&mut cached), expected);
        drop(cached);
        for name in names {
            fs::remove_file(sources.join(format!("{name}.bfl"))).unwrap();
        }

        let mut source_free = Loader::new(sources, artifacts, Instant::now());
        source_free.load("alpha", None).unwrap();
        assert_eq!(encode(&mut source_free), expected);
    }
}
