//! Bounded parallel byte reads; parsing and semantic validation stay on the loader thread.
//! Missing, changing, oversized, or invalid optional inputs use the normal read path.

use crate::cache::Cache;
use crate::model::{MAX_ARTIFACT, MAX_SOURCE};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

// Bound the entire build, including source/cache bytes prepared concurrently.
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_WORKERS: usize = 4;

pub(super) enum Prepared {
    Source { text: String, cached: Option<Vec<u8>> },
    Artifact { path: PathBuf, bytes: Vec<u8> },
}

pub(super) struct Inputs {
    ready: BTreeMap<String, Prepared>,
    remaining: usize,
}
impl Default for Inputs {
    fn default() -> Self {
        Self {
            ready: BTreeMap::new(),
            remaining: MAX_BYTES,
        }
    }
}
impl Inputs {
    pub(super) fn take(&mut self, name: &str) -> Option<Prepared> {
        self.ready.remove(name)
    }

    pub(super) fn prepare(
        &mut self,
        root: &Path,
        artifacts: &Path,
        cache: Option<&Cache>,
        names: &[String],
        started: Instant,
    ) -> Result<(), String> {
        if self.remaining == 0 {
            return Ok(());
        }
        let names = names
            .iter()
            .filter(|name| crate::module_name(name) && !self.ready.contains_key(name.as_str()))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let workers = MAX_WORKERS.min(names.len());
        if workers < 2 {
            return Ok(());
        }
        let remaining = AtomicUsize::new(self.remaining);
        let enforce_deadline = crate::INCREMENTAL_DEADLINE.get();
        let batches = std::thread::scope(|scope| {
            let mut jobs = Vec::new();
            for names in names.chunks(names.len().div_ceil(workers)) {
                let remaining = &remaining;
                let job = std::thread::Builder::new()
                    .name("bifoil-read".into())
                    .spawn_scoped(scope, move || {
                        crate::INCREMENTAL_DEADLINE.set(enforce_deadline);
                        let mut ready = Vec::new();
                        for name in names {
                            crate::budget(started)?;
                            if let Some(input) = prepare_one(root, artifacts, cache, name, remaining) {
                                ready.push((name.clone(), input));
                            }
                        }
                        Ok::<_, String>(ready)
                    })
                    .map_err(|error| error.to_string())?;
                jobs.push(job);
            }
            let mut batches = Vec::new();
            for job in jobs {
                batches.push(job.join().map_err(|_| "input reader panicked".to_string())??);
            }
            Ok::<_, String>(batches)
        })?;
        self.remaining = remaining.load(Ordering::Relaxed);
        for batch in batches {
            self.ready.extend(batch);
        }
        Ok(())
    }
}

fn reserve(path: &Path, limit: u64, remaining: &AtomicUsize) -> Option<u64> {
    if remaining.load(Ordering::Relaxed) == 0 {
        return None;
    }
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > limit {
        return None;
    }
    let size = usize::try_from(metadata.len()).ok()?;
    let mut available = remaining.load(Ordering::Relaxed);
    loop {
        let next = available.checked_sub(size)?;
        match remaining.compare_exchange_weak(available, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(current) => available = current,
        }
    }
    Some(metadata.len())
}

fn prepare_one(
    root: &Path,
    artifacts: &Path,
    cache: Option<&Cache>,
    name: &str,
    remaining: &AtomicUsize,
) -> Option<Prepared> {
    let relative = name.replace('.', "/");
    let source = root.join(relative.clone() + ".bfl");
    if source.is_file() {
        let size = reserve(&source, MAX_SOURCE as u64, remaining)?;
        let mut bytes = 0;
        // The reservation is also the read limit: a racing growth cannot exceed it.
        let text = crate::input::read(&source, size, &mut bytes).ok()?;
        let cached = cache.and_then(|cache| {
            let path = cache.path(name);
            let size = reserve(&path, MAX_ARTIFACT, remaining)?;
            crate::input::bytes(&path, size).ok()
        });
        Some(Prepared::Source { text, cached })
    } else {
        let nested = artifacts.join(relative + ".bfm.json");
        let path = if nested.is_file() {
            nested
        } else {
            artifacts.join(format!("{name}.bfm.json"))
        };
        let size = reserve(&path, MAX_ARTIFACT, remaining)?;
        let bytes = crate::input::bytes(&path, size).ok()?;
        Some(Prepared::Artifact { path, bytes })
    }
}
