//! Discover dependency components, then compile against complete declaration interfaces.
//!
//! Tarjan's stack contains only unfinished modules. Completed components retain
//! the existing dependency-first order and disposable-cache behavior.

mod groups;
mod identities;
mod inputs;
mod legacy;
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
    ast: Option<Ast>,
}
struct Node {
    source: Option<Source>,
    module: Option<Module>,
    artifact_hash: Option<String>,
    stored: bool,
    dependencies: Vec<String>,
}
impl Node {
    /// Interface resolution borrows the parsed source; final lowering takes it.
    /// Hash-checked source identity remains in the node for cache publication.
    fn take_ast(&mut self) -> Result<Ast, String> {
        self.ast()?;
        Ok(self.source.as_mut().unwrap().ast.take().unwrap())
    }
    fn ast(&mut self) -> Result<&Ast, String> {
        let source = self
            .source
            .as_mut()
            .ok_or("module source is required for recompilation")?;
        if source.ast.is_none() {
            // Discovery already accounted for this file in the aggregate input
            // budget. Re-read only the module being compiled, and reject edits
            // rather than compiling a body against stale dependency metadata.
            let mut reread_bytes = 0;
            let text = read(&source.path, MAX_SOURCE as u64, &mut reread_bytes)?;
            if cache::Cache::source_hash(&text) != source.hash {
                return Err(format!(
                    "{}: source changed after dependency discovery; retry compilation",
                    source.path.display()
                ));
            }
            source.ast = Some(
                parser::parse(&text, &source.logical)
                    .map_err(|error| crate::diagnostics::at_source(&source.logical, None, error))?,
            );
        }
        Ok(source.ast.as_ref().unwrap())
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
            let module = match &mut self.cache {
                Some(cache) => match cached {
                    Some(bytes) => cache.load_bytes(name, &hash, &bytes, &mut self.pending_code, self.start)?,
                    None => cache.load(name, &hash, &mut self.pending_code, self.start)?,
                },
                None => None,
            };
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
                source: Some(Source {
                    path,
                    hash,
                    logical,
                    ast: None,
                }),
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
            let module = crate::module_input::check(module)?.store(&mut self.pending_code, self.start)?;
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
        if cyclic {
            // Whole-group fingerprinting still requires the active component.
            for node in nodes.values_mut() {
                if node.stored {
                    self.pending_code
                        .restore_pending(node.module.as_mut().unwrap(), self.start)?;
                    node.stored = false;
                }
            }
        }
        let reusable = groups::reusable(&mut nodes, &self.modules, cyclic);
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
                let ast = node.ast()?;
                let scope = crate::scope::for_ast(ast, name, &self.modules)?;
                let ast = node.take_ast()?;
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
            if let (Some(source), Some(cache)) = (&node.source, &mut self.cache) {
                if reusable {
                    // Reusing validated records is incremental work even when
                    // the optional completed-build marker is absent or corrupt.
                    crate::INCREMENTAL_DEADLINE.set(true);
                    budget(self.start)?;
                    cache.reused += 1;
                } else {
                    cache.store(&module, &source.hash, self.start)?;
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
            if !node.stored {
                self.code.record(&mut module, self.start)?;
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
        let mut unchanged = loader.read_node("main", None).unwrap();
        assert!(unchanged.source.as_ref().unwrap().ast.is_none());
        assert_eq!(unchanged.dependencies, ["helpers"]);
        assert_eq!(unchanged.ast().unwrap().functions.len(), 1);

        let mut changed = loader.read_node("main", None).unwrap();
        fs::write(&path, text.replace("+ 1", "+ 2")).unwrap();
        let error = changed.ast().err().unwrap();
        assert!(error.contains("source changed after dependency discovery; retry compilation"));
        assert!(changed.source.as_ref().unwrap().ast.is_none());
    }
}
