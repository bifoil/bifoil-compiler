//! Project compilation, deterministic linking, and verified completed-build reuse.

use crate::function_catalog::FunctionCatalog;
use crate::loader::Loader;
use crate::model::{PROFILE, Program, digest};
use crate::{INCREMENTAL_DEADLINE, artifact, cache, initialization, legacy};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub(crate) struct Request<'a> {
    pub(crate) project: &'a Path,
    pub(crate) output: &'a Path,
    pub(crate) artifacts: PathBuf,
    pub(crate) entry: String,
    pub(crate) legacy_path: Option<String>,
    pub(crate) clean: bool,
    pub(crate) packaged: bool,
    pub(crate) start: Instant,
}
pub(crate) fn build(request: Request<'_>) -> Result<(), String> {
    let Request {
        project,
        output,
        artifacts,
        entry,
        legacy_path,
        clean,
        packaged,
        start,
    } = request;
    let cache_directory = project.join(".bifoil/cache/v1");
    let context = cache::BuildContext::new(project, &artifacts, &entry, legacy_path.as_deref(), output, packaged);
    let mut loader = Loader::new(project.join("src"), artifacts, start);
    loader.cache = Some(cache::Cache::new(cache_directory)?);
    INCREMENTAL_DEADLINE.set(!clean && loader.cache.as_ref().is_some_and(|cache| cache.is_warm()));
    if clean {
        loader.cache = None;
    }
    if let (Some(cache), Some(context)) = (&loader.cache, &context)
        && let Some(reused) = cache.try_completed(context, start)?
    {
        INCREMENTAL_DEADLINE.set(true);
        crate::budget(start)?;
        println!("modules: 0 compiled, {reused} reused");
        println!("completed build reused after byte validation");
        return Ok(());
    }
    let module = entry.rsplit_once('.').unwrap().0;
    loader.load(module, None)?;
    crate::trace::event(start, format_args!("modules loaded"));
    let mut types = BTreeMap::new();
    let mut shared = BTreeMap::new();
    let mut legacy_globals = BTreeMap::new();
    let mut functions = FunctionCatalog::default();
    let mut init = Vec::new();
    let mut modules = BTreeMap::new();
    let mut summaries = BTreeMap::new();
    for name in &loader.order {
        let m = loader.modules.remove(name).unwrap();
        modules.insert(name.clone(), m.digest);
        summaries.extend(m.summaries);
        merge_unique(&mut types, m.types, "nominal type")?;
        merge_unique(&mut shared, m.shared, "shared binding")?;
        merge_unique(&mut legacy_globals, m.legacy_globals, "legacy binding")?;
        functions.insert(name.clone(), m.functions)?;
        if let Some(id) = m.init {
            init.push(id);
        }
    }
    functions.validate(start)?;
    let legacy_sources = match legacy_path {
        Some(path) => legacy::sources(project, &path, &mut loader.total, start)?,
        None => BTreeMap::new(),
    };
    legacy::validate_targets(&loader.code.legacy, &legacy_sources, start)?;
    let sig = &functions.get(&entry).ok_or("entry scene was not found")?.signature;
    if !sig.scene || sig.params.iter().any(|p| !p.default) {
        return Err("entry must be a scene with no required inputs".into());
    }
    if !shared.is_empty() {
        let initializers = shared
            .values()
            .map(|definition| definition.initializer.clone())
            .collect();
        let summary_view = summaries
            .iter()
            .map(|(id, summary)| (id.as_str(), summary.as_ref()))
            .collect();
        initialization::resolve(&mut shared, &initializers, functions.values(), &summary_view, start)?;
    }
    let shared_order = initialization::order(&shared, start)?;
    drop(summaries);
    let functions = functions.freeze(start)?;
    crate::trace::event(start, format_args!("publication headers prepared"));
    let legacy_sha256 = artifact::digest(&legacy_sources)?;
    let cache = loader.cache.take();
    let inputs = std::mem::take(&mut loader.inputs);
    let mut program = Program {
        legacy_globals,
        legacy_sources,
        shared_order,
        shared,
        types,
        format: "bifoil.program.v2".into(),
        profile: PROFILE.into(),
        build: String::new(),
        entry,
        modules,
        functions: BTreeMap::<String, crate::model::Function>::new(),
        init,
    };
    crate::trace::event(start, format_args!("program linked"));
    let prepared = if packaged {
        program.format = crate::package::FORMAT.into();
        let root = crate::package::prepare(
            output,
            &mut program,
            crate::package::Sources {
                code: &loader.code,
                functions: &functions,
            },
            start,
        )?;
        drop(program);
        PreparedOutput::Package(root)
    } else {
        let mut program = program.with_functions(loader.code.functions(&functions));
        program.build = digest(&program)?;
        let prefix = format!("*comment Bifoil AOT output; requires {PROFILE}\n*bifoil ");
        PreparedOutput::Inline(artifact::prepare_json(output, &program, &prefix, "\n", start)?)
    };
    // Large declaration trees and temporary databases must finish cleanup while
    // a budget failure can still leave the previous completed root untouched.
    drop(functions);
    drop(loader);
    crate::trace::event(start, format_args!("publication inputs released"));
    prepared.commit(|checksum, chunks| {
        if let Some(cache) = &cache {
            cache.mark_build(start)?;
            if let Some(context) = &context {
                cache.remember_completed(
                    context,
                    &inputs,
                    &legacy_sha256,
                    cache::OutputFiles {
                        root_sha256: checksum.into(),
                        chunks,
                    },
                    start,
                )?;
            }
        }
        Ok(())
    })?;
    if let Some(cache) = &cache {
        println!("modules: {} compiled, {} reused", cache.compiled, cache.reused);
    }
    Ok(())
}

enum PreparedOutput {
    Inline(artifact::PreparedJson),
    Package(crate::package::Prepared),
}
impl PreparedOutput {
    fn commit(
        self,
        before_commit: impl FnOnce(&str, BTreeMap<String, String>) -> Result<(), String>,
    ) -> Result<(), String> {
        match self {
            Self::Inline(file) => file.commit(|checksum| before_commit(checksum, BTreeMap::new())),
            Self::Package(package) => package.commit(before_commit),
        }
    }
}

fn merge_unique<T>(target: &mut BTreeMap<String, T>, source: BTreeMap<String, T>, kind: &str) -> Result<(), String> {
    for (id, value) in source {
        if target.insert(id.clone(), value).is_some() {
            return Err(format!("duplicate linked {kind} identity {id}"));
        }
    }
    Ok(())
}
