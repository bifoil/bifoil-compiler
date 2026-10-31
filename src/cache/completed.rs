//! Reuse a completed build only after byte checks of every input, module cache,
//! native source inventory and output. No timestamp or filename alone is trusted.

use super::Cache;
use crate::model::{MAX_ARTIFACT, MAX_SOURCE};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BuildContext {
    project: PathBuf,
    source_root: PathBuf,
    artifact_root: PathBuf,
    entry: String,
    legacy_path: Option<String>,
    output: PathBuf,
    packaged: bool,
}
impl BuildContext {
    pub(crate) fn new(
        project: &Path,
        artifacts: &Path,
        entry: &str,
        legacy_path: Option<&str>,
        output: &Path,
        packaged: bool,
    ) -> Option<Self> {
        let project = std::path::absolute(project).ok()?;
        let artifact_root = std::path::absolute(artifacts).ok()?;
        let output = std::path::absolute(output).ok()?;
        for path in [&project, &artifact_root, &output] {
            path.to_str()?;
        }
        Some(Self {
            source_root: project.join("src"),
            project,
            artifact_root,
            entry: entry.into(),
            legacy_path: legacy_path.map(str::to_string),
            output,
            packaged,
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ModuleInput {
    Source {
        source_sha256: String,
        cache_sha256: String,
    },
    Artifact {
        artifact_sha256: String,
    },
}
#[derive(Serialize, Deserialize)]
pub(crate) struct OutputFiles {
    pub(crate) root_sha256: String,
    pub(crate) chunks: BTreeMap<String, String>,
}
#[derive(Serialize, Deserialize)]
struct Completed {
    format: String,
    compiler: String,
    context: BuildContext,
    modules: BTreeMap<String, ModuleInput>,
    legacy_sha256: String,
    output: OutputFiles,
    digest: String,
}
impl Cache {
    pub(crate) fn try_completed(&self, context: &BuildContext, start: Instant) -> Result<Option<usize>, String> {
        let matched = self.match_completed(context, start);
        crate::budget(start)?;
        Ok(matched)
    }
    fn match_completed(&self, context: &BuildContext, start: Instant) -> Option<usize> {
        let mut record: Completed =
            crate::input::json(&self.directory.join("completed.json"), 1024 * 1024, None).ok()?;
        if record.format != "bifoil.completed.v2"
            || record.compiler != self.compiler
            || record.context != *context
            || record.modules.is_empty()
            || record.modules.len() > crate::MAX_MODULES
            || (context.packaged && record.output.chunks.len() != record.modules.len())
        {
            return None;
        }
        let expected = std::mem::take(&mut record.digest);
        if crate::artifact::digest(&record).ok()? != expected {
            return None;
        }
        let (reused, mut total) = self.verify_modules(context, &record.modules, start)?;
        let legacy = match &context.legacy_path {
            Some(path) => crate::legacy::sources(&context.project, path, &mut total, start).ok()?,
            None => BTreeMap::new(),
        };
        if crate::artifact::digest(&legacy).ok()? != record.legacy_sha256 || !record.output.matches(context, start) {
            return None;
        }
        Some(reused)
    }

    fn verify_module(
        &self,
        context: &BuildContext,
        name: &str,
        fingerprint: &ModuleInput,
        total: &mut u64,
        start: Instant,
    ) -> Option<usize> {
        if !crate::module_name(name) {
            return None;
        }
        let source = context.source_root.join(name.replace('.', "/") + ".bfl");
        match fingerprint {
            ModuleInput::Source {
                source_sha256,
                cache_sha256,
            } => {
                if !source.is_file()
                    || crate::input::hash(&source, MAX_SOURCE as u64, Some(total), start).ok()? != *source_sha256
                    || crate::input::hash(&self.path(name), MAX_ARTIFACT, None, start).ok()? != *cache_sha256
                {
                    return None;
                }
                return Some(1);
            }
            ModuleInput::Artifact { artifact_sha256 } => {
                if source.is_file() {
                    return None;
                }
                let nested = context.artifact_root.join(name.replace('.', "/") + ".bfm.json");
                let path = if nested.is_file() {
                    nested
                } else {
                    context.artifact_root.join(format!("{name}.bfm.json"))
                };
                if crate::input::hash(&path, MAX_ARTIFACT, Some(total), start).ok()? != *artifact_sha256 {
                    return None;
                }
            }
        }
        Some(0)
    }
    fn verify_modules(
        &self,
        context: &BuildContext,
        modules: &BTreeMap<String, ModuleInput>,
        start: Instant,
    ) -> Option<(usize, u64)> {
        let entries: Vec<_> = modules.iter().collect();
        let workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(4)
            .min(entries.len());
        if workers <= 1 {
            let mut total = 0;
            let mut reused = 0;
            for (name, fingerprint) in entries {
                reused += self.verify_module(context, name, fingerprint, &mut total, start)?;
            }
            return Some((reused, total));
        }
        let enforce_deadline = crate::INCREMENTAL_DEADLINE.get();
        std::thread::scope(|scope| {
            let mut jobs = Vec::new();
            for chunk in entries.chunks(entries.len().div_ceil(workers)) {
                let job = std::thread::Builder::new()
                    .name("bifoil-verify".into())
                    .spawn_scoped(scope, move || {
                        crate::INCREMENTAL_DEADLINE.set(enforce_deadline);
                        let mut total = 0;
                        let mut reused = 0;
                        for (name, fingerprint) in chunk {
                            reused += self.verify_module(context, name, fingerprint, &mut total, start)?;
                        }
                        Some((reused, total))
                    })
                    .ok()?;
                jobs.push(job);
            }
            let mut reused = 0;
            let mut total = 0u64;
            for job in jobs {
                let (count, bytes) = job.join().ok()??;
                reused += count;
                total = total.checked_add(bytes)?;
                if total > crate::MAX_INPUT {
                    return None;
                }
            }
            Some((reused, total))
        })
    }
    pub(crate) fn remember_completed(
        &self,
        context: &BuildContext,
        modules: &BTreeMap<String, ModuleInput>,
        legacy_sha256: &str,
        output: OutputFiles,
        start: Instant,
    ) -> Result<(), String> {
        let mut record = Completed {
            format: "bifoil.completed.v2".into(),
            compiler: self.compiler.clone(),
            context: context.clone(),
            modules: modules.clone(),
            legacy_sha256: legacy_sha256.into(),
            output,
            digest: String::new(),
        };
        record.digest = crate::artifact::digest(&record)?;
        crate::artifact::write_json(&self.directory.join("completed.json"), &record, "", "", start)
    }
}

impl OutputFiles {
    fn matches(&self, context: &BuildContext, start: Instant) -> bool {
        let expected = if context.packaged {
            1..=crate::MAX_MODULES
        } else {
            0..=0
        };
        if !expected.contains(&self.chunks.len())
            || crate::input::hash(&context.output, MAX_ARTIFACT, None, start)
                .ok()
                .as_ref()
                != Some(&self.root_sha256)
        {
            return false;
        }
        let parent = context.output.parent().unwrap_or(Path::new("."));
        self.chunks.iter().all(|(filename, checksum)| {
            valid_chunk_file(filename)
                && crate::input::hash(&parent.join(filename), MAX_ARTIFACT, None, start)
                    .ok()
                    .as_ref()
                    == Some(checksum)
        })
    }
}

fn valid_chunk_file(filename: &str) -> bool {
    filename
        .strip_prefix("bifoil_code_")
        .and_then(|name| name.strip_suffix(".txt"))
        .is_some_and(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}
