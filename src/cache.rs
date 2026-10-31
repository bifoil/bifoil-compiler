//! Disposable module cache, separate from published source-free artifacts.
//!
//! Source bytes and the compiler executable are hashed, not timestamps. Cache
//! hits still validate artifact digests and every imported module dependency.

mod completed;
mod input;
#[cfg(test)]
mod tests;
pub(crate) use completed::{BuildContext, ModuleInput, OutputFiles};
use std::collections::BTreeMap;

use crate::model::{MAX_ARTIFACT, Module};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;

pub(crate) struct Cache {
    directory: PathBuf,
    compiler: String,
    observed: BTreeMap<String, String>,
    pub(crate) compiled: usize,
    pub(crate) reused: usize,
}
struct Record<M = Module> {
    compiler: String,
    source: String,
    module: M,
}
// Keep the private-spool failure channel alive through structural validation.
struct ParsedRecord {
    record: Record<crate::module_input::streamed::StreamedModule>,
    checksum: String,
    seed: crate::module_input::streamed::Seed,
}
#[derive(Serialize)]
struct RecordRef<'a> {
    compiler: &'a str,
    source: &'a str,
    module: &'a Module,
}
impl Cache {
    pub(crate) fn new(directory: PathBuf) -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let mut file = fs::File::open(executable).map_err(|error| error.to_string())?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        let compiler = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Self {
            directory,
            compiler,
            observed: BTreeMap::new(),
            compiled: 0,
            reused: 0,
        })
    }
    pub(crate) fn path(&self, name: &str) -> PathBuf {
        self.directory.join(format!("{name}.bfm.json"))
    }
    pub(crate) fn source_hash(source: &str) -> String {
        Sha256::digest(source.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
    pub(crate) fn load(
        &mut self,
        name: &str,
        source: &str,
        pending: &mut crate::code_store::CodeStore,
        started: Instant,
    ) -> Result<Option<Module>, String> {
        let path = self.path(name);
        let seed = crate::module_input::streamed::Seed::new(started);
        let parsed = crate::input::json_seed_hashed(
            &path,
            MAX_ARTIFACT,
            None,
            input::RecordSeed::new(seed.clone(), &self.compiler, source),
        );
        let (cached, checksum) = match parsed {
            Ok(value) => value,
            Err(_) => return cache_miss(&seed),
        };
        self.accept(
            name,
            source,
            pending,
            started,
            ParsedRecord {
                record: cached,
                checksum,
                seed,
            },
        )
    }

    pub(crate) fn load_bytes(
        &mut self,
        name: &str,
        source: &str,
        bytes: &[u8],
        pending: &mut crate::code_store::CodeStore,
        started: Instant,
    ) -> Result<Option<Module>, String> {
        let seed = crate::module_input::streamed::Seed::new(started);
        let parsed = crate::input::json_seed_bytes_hashed(
            bytes,
            MAX_ARTIFACT,
            input::RecordSeed::new(seed.clone(), &self.compiler, source),
        );
        let (record, checksum) = match parsed {
            Ok(record) => record,
            Err(_) => return cache_miss(&seed),
        };
        self.accept(name, source, pending, started, ParsedRecord { record, checksum, seed })
    }

    fn accept(
        &mut self,
        name: &str,
        source: &str,
        pending: &mut crate::code_store::CodeStore,
        started: Instant,
        input: ParsedRecord,
    ) -> Result<Option<Module>, String> {
        let ParsedRecord {
            record: cached,
            checksum,
            seed,
        } = input;
        if cached.compiler != self.compiler || cached.source != source || cached.module.name != name {
            return Ok(None);
        }
        let checked = match crate::module_input::check(cached.module) {
            Ok(module) => module,
            Err(_) => return cache_miss(&seed),
        };
        let module = checked.store(pending, started)?;
        self.observed.insert(name.into(), checksum);
        Ok(Some(module))
    }
    pub(crate) fn store(&mut self, module: &Module, source: &str, start: Instant) -> Result<(), String> {
        let record = RecordRef {
            compiler: &self.compiler,
            source,
            module,
        };
        let checksum = crate::artifact::digest(&record)?;
        crate::artifact::write_json(&self.path(&module.name), &record, "", "", start)?;
        self.observed.insert(module.name.clone(), checksum);
        Ok(())
    }
    pub(crate) fn observed(&self, name: &str) -> Option<&str> {
        self.observed.get(name).map(String::as_str)
    }
    pub(crate) fn mark_build(&self, start: Instant) -> Result<(), String> {
        crate::artifact::write_text(&self.directory.join("build.sha256"), &self.compiler, start)
    }
    pub(crate) fn is_warm(&self) -> bool {
        fs::File::open(self.directory.join("build.sha256"))
            .is_ok_and(|file| marker_matches(file, self.compiler.as_bytes()))
    }
}

fn cache_miss(seed: &crate::module_input::streamed::Seed) -> Result<Option<Module>, String> {
    seed.take_failure().map_or(Ok(None), Err)
}

/// Read at most one byte beyond the fixed digest, including for changing files.
fn marker_matches(mut reader: impl Read, expected: &[u8]) -> bool {
    let mut bytes = [0u8; 65];
    let mut count = 0;
    if expected.len() != 64 {
        return false;
    }
    while count < bytes.len() {
        match reader.read(&mut bytes[count..]) {
            Ok(0) => return count == 64 && &bytes[..count] == expected,
            Ok(read) => count += read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return false,
        }
    }
    false
}
