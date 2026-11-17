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
        let batches = verify_batches(&entries, |chunk| {
            let mut total = 0;
            let mut reused = 0;
            for (name, fingerprint) in chunk {
                reused += self.verify_module(context, name, fingerprint, &mut total, start)?;
            }
            Some((reused, total))
        })?;
        let mut reused = 0;
        let mut total = 0u64;
        for (count, bytes) in batches {
            reused += count;
            total = total.checked_add(bytes)?;
            if total > crate::MAX_INPUT {
                return None;
            }
        }
        Some((reused, total))
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
        let entries = self.chunks.iter().collect::<Vec<_>>();
        verify_batches(&entries, |chunk| {
            for (filename, checksum) in chunk {
                if !valid_chunk_file(filename)
                    || crate::input::hash(&parent.join(filename), MAX_ARTIFACT, None, start)
                        .ok()
                        .as_ref()
                        != Some(*checksum)
                {
                    return None;
                }
            }
            Some(())
        })
        .is_some()
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

/// Byte validation is I/O bound on mounted filesystems. Bound parallelism by
/// hardware and a fixed ceiling; workers never parse bodies or spawn helpers.
/// Every worker inherits the caller's incremental deadline. Failed optional
/// checks still join all scoped workers before the normal rebuild path runs.
fn verify_batches<T: Sync, R: Send>(entries: &[T], check: impl Fn(&[T]) -> Option<R> + Sync) -> Option<Vec<R>> {
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(16)
        .min(entries.len());
    if workers <= 1 {
        return Some(vec![check(entries)?]);
    }
    let enforce_deadline = crate::INCREMENTAL_DEADLINE.get();
    std::thread::scope(|scope| {
        let mut jobs = Vec::new();
        let check = &check;
        for chunk in entries.chunks(entries.len().div_ceil(workers)) {
            let job = std::thread::Builder::new()
                .name("bifoil-verify".into())
                .spawn_scoped(scope, move || {
                    crate::INCREMENTAL_DEADLINE.set(enforce_deadline);
                    check(chunk)
                })
                .ok()?;
            jobs.push(job);
        }
        jobs.into_iter().map(|job| job.join().ok()?).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

    struct CompletedFixture {
        directory: PathBuf,
        cache: Cache,
        record: Completed,
    }
    impl Drop for CompletedFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
    impl CompletedFixture {
        fn new(packaged: bool, artifact: bool) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "bifoil-completed-contract-{}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            for name in ["src", "cache", "artifacts", "output"] {
                fs::create_dir(directory.join(name)).unwrap();
            }
            let cache = Cache {
                directory: directory.join("cache"),
                compiler: "original-compiler-identity".into(),
                observed: BTreeMap::new(),
                compiled: 0,
                reused: 0,
            };
            let context = BuildContext::new(
                &directory,
                &directory.join("artifacts"),
                "start",
                None,
                &directory.join("output/startup.txt"),
                packaged,
            )
            .unwrap();
            fs::write(&context.output, "Original generated output").unwrap();
            let hash = |path: &Path| crate::input::hash(path, MAX_ARTIFACT, None, Instant::now()).unwrap();
            // Completed-build reuse checks byte identities. The independent
            // module-admission tests own semantic validation of module bodies.
            let input = if artifact {
                let path = context.artifact_root.join("helpers.bfm.json");
                fs::write(&path, "Original published module bytes").unwrap();
                ModuleInput::Artifact {
                    artifact_sha256: hash(&path),
                }
            } else {
                let source = context.source_root.join("helpers.bfl");
                fs::write(&source, "export fun read() = 1\n").unwrap();
                fs::write(cache.path("helpers"), "Original private cache bytes").unwrap();
                ModuleInput::Source {
                    source_sha256: hash(&source),
                    cache_sha256: hash(&cache.path("helpers")),
                }
            };
            let mut chunks = BTreeMap::new();
            if packaged {
                let name = format!("bifoil_code_{}.txt", "a".repeat(64));
                let path = directory.join("output").join(&name);
                fs::write(&path, "Original code chunk").unwrap();
                chunks.insert(name, hash(&path));
            }
            let record = Completed {
                format: "bifoil.completed.v2".into(),
                compiler: cache.compiler.clone(),
                output: OutputFiles {
                    root_sha256: hash(&context.output),
                    chunks,
                },
                context,
                modules: BTreeMap::from([("helpers".into(), input)]),
                legacy_sha256: crate::artifact::digest(&BTreeMap::<String, String>::new()).unwrap(),
                digest: String::new(),
            };
            Self {
                directory,
                cache,
                record,
            }
        }
        fn publish(&mut self) {
            self.record.digest.clear();
            self.record.digest = crate::artifact::digest(&self.record).unwrap();
            fs::write(
                self.cache.directory.join("completed.json"),
                serde_json::to_vec(&self.record).unwrap(),
            )
            .unwrap();
        }
        fn reused(&self) -> Option<usize> {
            self.cache.try_completed(&self.record.context, Instant::now()).unwrap()
        }
    }

    #[test]
    fn coverage_completed_builds_reject_each_stale_metadata_identity() {
        for kind in [
            "format",
            "compiler",
            "digest",
            "empty-modules",
            "invalid-module",
            "legacy",
            "chunk-count",
        ] {
            let mut fixture = CompletedFixture::new(true, false);
            fixture.publish();
            assert_eq!(fixture.reused(), Some(1), "intact control: {kind}");
            match kind {
                "format" => fixture.record.format = "old-format".into(),
                "compiler" => fixture.record.compiler = "old-compiler".into(),
                "digest" => {}
                "empty-modules" => fixture.record.modules.clear(),
                "invalid-module" => {
                    let value = fixture.record.modules.remove("helpers").unwrap();
                    fixture.record.modules.insert("../outside".into(), value);
                }
                "legacy" => fixture.record.legacy_sha256 = "stale".into(),
                "chunk-count" => fixture.record.output.chunks.clear(),
                _ => unreachable!(),
            }
            fixture.publish();
            if kind == "digest" {
                let path = fixture.cache.directory.join("completed.json");
                let text = fs::read_to_string(&path).unwrap();
                fs::write(path, text.replace(&fixture.record.digest, "stale")).unwrap();
            }
            assert_eq!(fixture.reused(), None, "stale metadata accepted: {kind}");
        }
    }

    #[test]
    fn coverage_completed_builds_recheck_source_cache_artifact_and_output_bytes() {
        for kind in [
            "source",
            "cache",
            "artifact",
            "source-override",
            "output",
            "chunk",
            "unsafe-chunk",
        ] {
            let artifact = kind == "artifact" || kind == "source-override";
            let mut fixture = CompletedFixture::new(true, artifact);
            fixture.publish();
            assert_eq!(fixture.reused(), Some(usize::from(!artifact)), "intact control: {kind}");
            let context = &fixture.record.context;
            let path = match kind {
                "source" | "source-override" => context.source_root.join("helpers.bfl"),
                "cache" => fixture.cache.path("helpers"),
                "artifact" => context.artifact_root.join("helpers.bfm.json"),
                "output" => context.output.clone(),
                "chunk" => fixture
                    .directory
                    .join("output")
                    .join(fixture.record.output.chunks.keys().next().unwrap()),
                "unsafe-chunk" => {
                    let checksum = fixture.record.output.chunks.pop_first().unwrap().1;
                    fixture.record.output.chunks.insert("../outside.txt".into(), checksum);
                    fixture.publish();
                    assert_eq!(fixture.reused(), None);
                    continue;
                }
                _ => unreachable!(),
            };
            fs::write(path, "Changed after the successful build").unwrap();
            assert_eq!(fixture.reused(), None, "changed bytes accepted: {kind}");
        }
    }

    #[test]
    fn coverage_chunk_names_and_unbundled_output_reject_noncanonical_shapes() {
        let hash = "a".repeat(64);
        assert!(valid_chunk_file(&format!("bifoil_code_{hash}.txt")));
        for name in [
            "../outside.txt".into(),
            format!("bifoil_code_{}.txt", "a".repeat(63)),
            format!("bifoil_code_{}.txt", "A".repeat(64)),
            format!("bifoil_code_{hash}.json"),
        ] {
            assert!(!valid_chunk_file(&name), "{name}");
        }
        let mut fixture = CompletedFixture::new(false, false);
        fixture.publish();
        assert_eq!(fixture.reused(), Some(1));
        fixture
            .record
            .output
            .chunks
            .insert(format!("bifoil_code_{hash}.txt"), "stale".into());
        fixture.publish();
        assert_eq!(fixture.reused(), None);
    }

    #[test]
    fn optional_misses_join_all_workers_with_the_callers_deadline() {
        struct Restore(bool);
        impl Drop for Restore {
            fn drop(&mut self) {
                crate::INCREMENTAL_DEADLINE.set(self.0);
            }
        }
        let _restore = Restore(crate::INCREMENTAL_DEADLINE.get());
        crate::INCREMENTAL_DEADLINE.set(true);
        let visited = AtomicUsize::new(0);
        let entries = (0..64).collect::<Vec<_>>();
        let result = verify_batches(&entries, |chunk| {
            assert!(crate::INCREMENTAL_DEADLINE.get());
            for _ in chunk {
                visited.fetch_add(1, Ordering::Relaxed);
            }
            (!chunk.contains(&0)).then_some(())
        });
        assert!(result.is_none());
        assert_eq!(visited.load(Ordering::Relaxed), entries.len());
        assert!(crate::INCREMENTAL_DEADLINE.get());
    }
}
