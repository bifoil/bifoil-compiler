//! Scoped temporary storage for completed instruction bodies.
//!
//! Only declaration metadata remains resident between modules. Final hashing and
//! output serialization borrow one function at a time from this store; canonical
//! JSON and the public module/program formats remain unchanged.

mod counting_writer;
mod encoded;
mod encoded_cache;
mod function_map;
mod legacy_scan;
pub(crate) use function_map::StoredFunctions;
mod record_index;
use record_index::RecordIndex;
mod writer;
use encoded::EncodedFunction;
use sha2::{Digest, Sha256};
use writer::{CodeWriter, PendingWriter};

use crate::model::{Function, LegacyTarget, Module, Span};
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Callable metadata read without allocating or exposing an instruction body.
#[derive(Debug, Deserialize)]
pub(crate) struct FunctionHeader {
    pub(crate) signature: crate::model::Signature,
    pub(crate) init: bool,
    #[serde(default)]
    pub(crate) local: bool,
}

#[derive(Clone)]
struct Location {
    offset: u64,
    length: u64,
    instructions: usize,
    checksum: [u8; 32],
    header_checksum: [u8; 32],
}
pub(crate) struct LegacyReference {
    pub(crate) function: std::sync::Arc<str>,
    pub(crate) target: LegacyTarget,
    pub(crate) span: Span,
}
#[derive(Default)]
pub(crate) struct CodeStore {
    file: RefCell<Option<File>>,
    path: Option<PathBuf>,
    entries: RecordIndex,
    encoded_cache: RefCell<Option<encoded_cache::Cache>>,
    pub(crate) instructions: usize,
    pub(crate) legacy: Vec<LegacyReference>,
}
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
impl CodeStore {
    fn open(&mut self) -> Result<(), String> {
        if self.path.is_some() {
            return Ok(());
        }
        loop {
            let index = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("bifoil-code-{}-{index}.tmp", std::process::id()));
            match OpenOptions::new().read(true).write(true).create_new(true).open(&path) {
                Ok(file) => {
                    *self.file.get_mut() = Some(file);
                    self.path = Some(path);
                    // Unix keeps the open handle alive after unlinking, so an
                    // interrupted compiler cannot leave a large named spool file.
                    #[cfg(unix)]
                    fs::remove_file(self.path.as_ref().unwrap()).map_err(|error| error.to_string())?;
                    return Ok(());
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("cannot create temporary code store: {error}")),
            }
        }
    }
    pub(crate) fn record(&mut self, module: &mut Module, started: Instant) -> Result<(), String> {
        self.record_module(module, started, true)
    }
    #[cfg(test)]
    pub(crate) fn record_pending(&mut self, module: &mut Module, started: Instant) -> Result<(), String> {
        self.record_module(module, started, false)
    }
    /// Keep one buffered writer alive while a module's functions are decoded.
    pub(crate) fn pending_writer(&mut self) -> Result<PendingWriter<'_>, String> {
        self.writer().map(PendingWriter::new)
    }
    /// Append digest-checked input bodies through one buffered writer. Validation
    /// already decoded them; pending transfer needs only canonical bytes here.
    pub(crate) fn copy_pending_from(
        &mut self,
        source: &mut Self,
        headers: &BTreeMap<String, Function>,
        started: Instant,
    ) -> Result<(), String> {
        let mut writer = self.writer()?;
        for (id, header) in headers {
            crate::budget(started)?;
            if !header.code.is_empty() {
                return Err(format!("pending function already has instructions: {id}"));
            }
            let location = source
                .entries
                .get(id)?
                .ok_or_else(|| format!("missing stored function {id}"))?;
            let encoded = source.encoded_checked_at(id, header, &location)?;
            writer.record_encoded(id, &encoded, location.instructions, location.header_checksum)?;
            source.remove(id)?;
        }
        writer.finish()?;
        source.reclaim_empty()?;
        crate::budget(started)
    }
    fn writer(&mut self) -> Result<CodeWriter<'_>, String> {
        self.open()?;
        CodeWriter::new(
            self.file.get_mut().as_mut().unwrap(),
            &mut self.entries,
            &mut self.instructions,
            &mut self.legacy,
        )
    }
    fn record_module(&mut self, module: &mut Module, started: Instant, collect_legacy: bool) -> Result<(), String> {
        let mut writer = self.writer()?;
        for (id, function) in &mut module.functions {
            crate::budget(started)?;
            writer.record(id, function, collect_legacy)?;
            function.code = Vec::new();
        }
        writer.finish()?;
        crate::budget(started)
    }

    /// Summarize and move pending bodies individually. Headers stay resident;
    /// no complete module instruction map is reconstructed. Decode bodies selected
    /// by the consumer, plus any body needed for legacy-reference collection.
    pub(crate) fn transfer_pending(
        &mut self,
        source: &mut Self,
        headers: &BTreeMap<String, Function>,
        started: Instant,
        requires_body: impl Fn(&Function) -> bool,
        mut inspect: impl FnMut(&str, &Function) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut writer = self.writer()?;
        for (id, header) in headers {
            crate::budget(started)?;
            if !header.code.is_empty() {
                return Err(format!("pending function already has instructions: {id}"));
            }
            let location = source
                .entries
                .get(id)?
                .ok_or_else(|| format!("missing stored function {id}"))?;
            let encoded = source.encoded_checked_at(id, header, &location)?;
            if requires_body(header)
                || encoded
                    .has_legacy_calls()
                    .map_err(|error| format!("stored function {id}: {error}"))?
            {
                let function = encoded
                    .decoded()
                    .map_err(|error| format!("stored function {id}: {error}"))?;
                inspect(id, &function)?;
                writer.record_existing(id, &encoded, &function)?;
            } else {
                // Input validation already decoded this body. Keep its checked
                // bytes when neither analysis nor legacy collection needs it.
                writer.record_encoded(id, &encoded, location.instructions, location.header_checksum)?;
            }
            source.remove(id)?;
        }
        writer.finish()?;
        source.reclaim_empty()?;
        crate::budget(started)
    }

    /// A stale disposable cache contributes no code to its source rebuild.
    pub(crate) fn discard_pending(&mut self, module: &Module, started: Instant) -> Result<(), String> {
        for (id, header) in &module.functions {
            crate::budget(started)?;
            if !header.code.is_empty() {
                return Err(format!("pending function already has instructions: {id}"));
            }
            self.remove(id)?;
        }
        self.reclaim_empty()
    }
    fn remove(&mut self, id: &str) -> Result<(), String> {
        let location = self
            .entries
            .remove(id)?
            .ok_or_else(|| format!("missing stored function {id}"))?;
        self.instructions -= location.instructions;
        Ok(())
    }
    fn reclaim_empty(&mut self) -> Result<(), String> {
        self.entries.reclaim_empty()?;
        if self.entries.is_empty()
            && let Some(file) = self.file.get_mut().as_mut()
        {
            file.set_len(0).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
    /// Restore only the component currently undergoing dependency validation.
    /// Pending ancestors remain on disk, and an invalid cache can be discarded.
    pub(crate) fn restore_pending(&mut self, module: &mut Module, started: Instant) -> Result<(), String> {
        for (id, header) in &mut module.functions {
            crate::budget(started)?;
            if !header.code.is_empty() {
                return Err(format!("pending function already has instructions: {id}"));
            }
            let function = self.read_checked(id, header)?;
            self.remove(id)?;
            *header = function;
        }
        self.reclaim_empty()?;
        crate::budget(started)
    }
    fn read_checked(&self, id: &str, header: &Function) -> Result<Function, String> {
        let function = self.read(id)?;
        if function.signature != header.signature
            || function.init != header.init
            || function.local != header.local
            || function.code.len()
                != self
                    .entries
                    .get(id)?
                    .ok_or_else(|| format!("missing stored function {id}"))?
                    .instructions
        {
            return Err(format!("stored function header changed: {id}"));
        }
        Ok(function)
    }
    fn read(&self, id: &str) -> Result<Function, String> {
        let location = self
            .entries
            .get(id)?
            .ok_or_else(|| format!("missing stored function {id}"))?;
        let mut file = self.file.borrow_mut();
        let file = file.as_mut().ok_or("temporary code store is closed")?;
        file.seek(SeekFrom::Start(location.offset))
            .map_err(|error| error.to_string())?;
        let reader = BufReader::with_capacity(65536, file.take(location.length));
        serde_json::from_reader(reader).map_err(|error| format!("stored function {id}: {error}"))
    }
    #[cfg(test)]
    pub(crate) fn force_disk_index_for_test(&mut self) {
        assert!(self.entries.is_empty());
        self.entries = RecordIndex::new(0);
    }
    #[cfg(test)]
    pub(crate) fn damage_index_for_test(&mut self, id: &str) -> Result<(), String> {
        self.entries.damage(id)
    }
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn fail_writes_for_test(&mut self) -> Result<(), String> {
        // Keep our own disposable path for Drop; never take ownership of the
        // device pathname. Its descriptor reports ENOSPC when buffers flush.
        self.open()?;
        *self.file.get_mut() = Some(
            OpenOptions::new()
                .read(true)
                .write(true)
                .open("/dev/full")
                .map_err(|error| error.to_string())?,
        );
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn close_for_test(&self) {
        self.file.borrow_mut().take();
    }

    /// Reuse canonical bytes only after checking their private spool identity.
    /// Semantic consumers still decode one typed function through `read`.
    pub(crate) fn encoded(&self, id: &str) -> Result<Arc<EncodedFunction>, String> {
        let location = self
            .entries
            .get(id)?
            .ok_or_else(|| format!("missing stored function {id}"))?;
        self.encoded_at(id, &location)
    }
    fn encoded_at(&self, id: &str, location: &Location) -> Result<Arc<EncodedFunction>, String> {
        // An internal function cannot be larger than the canonical module budget.
        // Check before allocating even though normal records are already bounded.
        if location.length > crate::model::MAX_ARTIFACT * 4 {
            return Err(format!("stored function {id}: canonical data size budget exceeded"));
        }
        let mut file = self.file.borrow_mut();
        let file = file.as_mut().ok_or("temporary code store is closed")?;
        file.seek(SeekFrom::Start(location.offset))
            .map_err(|error| error.to_string())?;
        let mut bytes = String::with_capacity(location.length as usize);
        file.take(location.length)
            .read_to_string(&mut bytes)
            .map_err(|error| format!("stored function {id}: {error}"))?;
        if bytes.len() as u64 != location.length || Sha256::digest(bytes.as_bytes()).as_slice() != location.checksum {
            return Err(format!("stored function {id}: temporary data checksum changed"));
        }
        // The scope borrows this store immutably, so record identities/counts
        // cannot change. Keep the actual I/O and checksum checks above even on
        // a hit: a cached value must never conceal damaged private storage.
        if let Some(value) = self.encoded_cache.borrow().as_ref().and_then(|cache| cache.get(id)) {
            value.check_record(id, location.instructions, location.header_checksum)?;
            return Ok(value);
        }
        let value = Arc::new(
            EncodedFunction::new(bytes, location.instructions)
                .map_err(|error| format!("stored function {id}: {error}"))?,
        );
        value.check_record(id, location.instructions, location.header_checksum)?;
        if let Some(cache) = self.encoded_cache.borrow_mut().as_mut() {
            cache.insert(id, Arc::clone(&value));
        }
        Ok(value)
    }
    fn encoded_checked(&self, id: &str, header: &Function) -> Result<Arc<EncodedFunction>, String> {
        let location = self
            .entries
            .get(id)?
            .ok_or_else(|| format!("missing stored function {id}"))?;
        self.encoded_checked_at(id, header, &location)
    }
    fn encoded_expected(&self, id: &str, header: &[u8; 32]) -> Result<Arc<EncodedFunction>, String> {
        // encoded() still checks actual spool bytes, instruction counts and the
        // private location fingerprint on every lookup, including cache hits.
        let value = self.encoded(id)?;
        value.check_checksum(id, header)?;
        Ok(value)
    }

    fn encoded_checked_at(
        &self,
        id: &str,
        header: &Function,
        location: &Location,
    ) -> Result<Arc<EncodedFunction>, String> {
        if header.signature.id != id {
            return Err(format!("stored function header changed: {id}"));
        }
        let value = self.encoded_at(id, location)?;
        value.check_header(id, header)?;
        Ok(value)
    }
    /// Read checked callable metadata without constructing its instruction vector.
    pub(crate) fn header(&self, id: &str) -> Result<FunctionHeader, String> {
        let header = self
            .encoded(id)?
            .header()
            .map_err(|error| format!("stored function {id}: {error}"))?;
        if header.signature.id != id {
            return Err(format!("stored function header changed: {id}"));
        }
        Ok(header)
    }

    pub(crate) fn cache_scope(&self, limit: usize) -> Result<encoded_cache::Scope<'_>, String> {
        encoded_cache::Scope::enter(&self.encoded_cache, limit)
    }

    pub(crate) fn record_scope<'a>(
        &'a self,
        ids: impl IntoIterator<Item = &'a str>,
    ) -> Result<record_index::ReadScope<'a>, String> {
        self.entries.read_scope(ids)
    }

    pub(crate) fn functions<'a, H: function_map::Headers + ?Sized>(&'a self, headers: &'a H) -> StoredFunctions<'a, H> {
        StoredFunctions { store: self, headers }
    }
}
pub(crate) fn header_checksum(function: &Function) -> Result<[u8; 32], String> {
    crate::artifact::digest_bytes(&(&function.signature, function.init, function.local))
}

impl Drop for CodeStore {
    fn drop(&mut self) {
        // Close first so the same cleanup works on platforms that lock open files.
        self.file.get_mut().take();
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}
#[cfg(test)]
mod tests;
