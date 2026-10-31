//! Owned temporary redb storage. No database is part of a public artifact.

use super::super::Location;
use redb::{Database, Durability, ReadOnlyTable, ReadableDatabase, TableDefinition};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Display;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const ROWS: TableDefinition<&str, &[u8]> = TableDefinition::new("locations");
const CACHE_BYTES: usize = 4 * 1024 * 1024;
const WRITE_BATCH_ROWS: usize = 4096;
const WRITE_BATCH_BYTES: usize = 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct DiskIndex {
    pending: BTreeMap<String, Option<Location>>,
    batch_limit: usize,
    byte_limit: usize,
    pending_bytes: usize,
    published: bool,
    read: RefCell<Option<ReadOnlyTable<&'static str, &'static [u8]>>>,
    database: Option<Database>,
    path: PathBuf,
}
impl DiskIndex {
    pub(super) fn new() -> Result<Self, String> {
        Self::with_batch_limits(WRITE_BATCH_ROWS, WRITE_BATCH_BYTES)
    }
    fn with_batch_limits(batch_limit: usize, byte_limit: usize) -> Result<Self, String> {
        let (path, file) = loop {
            let path = std::env::temp_dir().join(format!(
                "bifoil-index-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new().read(true).write(true).create_new(true).open(&path) {
                Ok(file) => break (path, file),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(failure(error)),
            }
        };
        let mut index = Self {
            pending: BTreeMap::new(),
            batch_limit,
            byte_limit,
            pending_bytes: 0,
            published: false,
            read: RefCell::new(None),
            database: None,
            path,
        };
        let mut builder = Database::builder();
        builder.set_cache_size(CACHE_BYTES);
        index.database = Some(builder.create_file(file).map_err(failure)?);
        #[cfg(unix)]
        fs::remove_file(&index.path).map_err(failure)?;
        Ok(index)
    }
    pub(super) fn insert(&mut self, id: &str, value: &Location) -> Result<(), String> {
        self.stage(id, Some(value.clone()))
    }
    pub(super) fn get(&self, id: &str) -> Result<Option<Location>, String> {
        if let Some(value) = self.pending.get(id) {
            return Ok(value.clone());
        }
        if !self.published {
            return Ok(None);
        }
        let mut cached = self.read.borrow_mut();
        if cached.is_none() {
            let read = self.database.as_ref().unwrap().begin_read().map_err(failure)?;
            *cached = Some(read.open_table(ROWS).map_err(failure)?);
        }
        cached
            .as_ref()
            .unwrap()
            .get(id)
            .map_err(failure)?
            .map(|value| decode(value.value()))
            .transpose()
    }
    pub(super) fn remove(&mut self, id: &str) -> Result<Option<Location>, String> {
        let previous = self.get(id)?;
        if previous.is_some() {
            self.stage(id, None)?;
        }
        Ok(previous)
    }
    fn stage(&mut self, id: &str, value: Option<Location>) -> Result<(), String> {
        let existing = self.pending.contains_key(id);
        let cost = id
            .len()
            .saturating_add(std::mem::size_of::<(String, Option<Location>)>() + 64);
        if !existing && self.pending_bytes.saturating_add(cost) > self.byte_limit {
            self.finish()?;
        }
        if !self.pending.contains_key(id) {
            self.pending_bytes = self.pending_bytes.saturating_add(cost);
        }
        self.pending.insert(id.to_owned(), value);
        // An unusually long single key flushes immediately rather than remaining
        // resident. Reservations include the owned key and a tree-node allowance.
        if self.pending.len() >= self.batch_limit || self.pending_bytes >= self.byte_limit {
            self.finish()?;
        }
        Ok(())
    }
    pub(super) fn finish(&mut self) -> Result<(), String> {
        if self.pending.is_empty() {
            return Ok(());
        }
        // A reader owns its committed root. Release it before the next batch so
        // obsolete pages do not accumulate while modules enter or leave storage.
        self.read.get_mut().take();
        let mut write = self.database.as_ref().unwrap().begin_write().map_err(failure)?;
        // The index is disposable within one compilation; it is never recovered
        // after a crash. Batching also avoids opening a table for every record.
        write.set_durability(Durability::None).map_err(failure)?;
        {
            let mut rows = write.open_table(ROWS).map_err(failure)?;
            for (id, location) in &self.pending {
                if let Some(location) = location {
                    rows.insert(id.as_str(), encode(location).as_slice()).map_err(failure)?;
                } else {
                    rows.remove(id.as_str()).map_err(failure)?;
                }
            }
        }
        write.commit().map_err(failure)?;
        self.published = true;
        // Retain the overlay until the transaction commits successfully. A failed
        // write must not make a staged insertion or deletion disappear from reads.
        self.pending.clear();
        self.pending_bytes = 0;
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn damage(&mut self, id: &str) -> Result<(), String> {
        self.finish()?;
        self.read.get_mut().take();
        let mut write = self.database.as_ref().unwrap().begin_write().map_err(failure)?;
        write.set_durability(Durability::None).map_err(failure)?;
        {
            let mut rows = write.open_table(ROWS).map_err(failure)?;
            rows.insert(id, b"bad row".as_slice()).map_err(failure)?;
        }
        write.commit().map_err(failure)
    }
}
impl Drop for DiskIndex {
    fn drop(&mut self) {
        self.read.get_mut().take();
        self.database.take();
        let _ = fs::remove_file(&self.path);
    }
}
fn failure(error: impl Display) -> String {
    format!("temporary record index: {error}")
}

fn encode(location: &Location) -> [u8; 88] {
    let mut bytes = [0; 88];
    bytes[..8].copy_from_slice(&location.offset.to_le_bytes());
    bytes[8..16].copy_from_slice(&location.length.to_le_bytes());
    bytes[16..24].copy_from_slice(&(location.instructions as u64).to_le_bytes());
    bytes[24..56].copy_from_slice(&location.checksum);
    bytes[56..].copy_from_slice(&location.header_checksum);
    bytes
}
fn decode(bytes: &[u8]) -> Result<Location, String> {
    let bytes: &[u8; 88] = bytes
        .try_into()
        .map_err(|_| "temporary record index: invalid row size")?;
    Ok(Location {
        offset: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        length: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        instructions: usize::try_from(u64::from_le_bytes(bytes[16..24].try_into().unwrap())).map_err(failure)?,
        checksum: bytes[24..56].try_into().unwrap(),
        header_checksum: bytes[56..].try_into().unwrap(),
    })
}

#[cfg(test)]
#[test]
fn owned_temporary_index_files_are_released_on_drop() {
    let mut index = DiskIndex::new().unwrap();
    let path = index.path.clone();
    index
        .insert(
            "test.function",
            &Location {
                offset: 0,
                length: 100,
                instructions: 2,
                checksum: [1; 32],
                header_checksum: [2; 32],
            },
        )
        .unwrap();
    index.finish().unwrap();
    assert!(index.get("test.function").unwrap().is_some());
    #[cfg(unix)]
    assert!(!path.exists());
    drop(index);
    assert!(!path.exists());
}

#[cfg(test)]
#[test]
fn write_batches_preserve_overlay_reads_and_invalidate_committed_readers() {
    let mut index = DiskIndex::with_batch_limits(2, WRITE_BATCH_BYTES).unwrap();
    let mut row = Location {
        offset: 1,
        length: 100,
        instructions: 2,
        checksum: [1; 32],
        header_checksum: [2; 32],
    };
    index.insert("first", &row).unwrap();
    assert_eq!(index.get("first").unwrap().unwrap().offset, 1);
    assert!(index.get("absent").unwrap().is_none());
    assert!(!index.published);
    index.insert("second", &row).unwrap();
    assert!(index.pending.is_empty());
    assert_eq!(index.get("first").unwrap().unwrap().offset, 1);
    assert!(index.read.borrow().is_some());

    row.offset = 9;
    index.insert("first", &row).unwrap();
    assert_eq!(index.get("first").unwrap().unwrap().offset, 9);
    assert!(index.remove("second").unwrap().is_some());
    assert!(index.pending.is_empty());
    assert!(index.read.borrow().is_none());
    assert!(index.get("second").unwrap().is_none());
    assert_eq!(index.get("first").unwrap().unwrap().offset, 9);

    assert!(index.remove("first").unwrap().is_some());
    assert!(index.get("first").unwrap().is_none());
    index.insert("first", &row).unwrap();
    assert_eq!(index.get("first").unwrap().unwrap().offset, 9);
    index.finish().unwrap();
    assert!(index.pending.is_empty());
    assert_eq!(index.get("first").unwrap().unwrap().offset, 9);
}

#[cfg(test)]
#[test]
fn write_batches_bound_key_storage_and_do_not_charge_replacements_twice() {
    let mut index = DiskIndex::with_batch_limits(100, 1024).unwrap();
    let row = Location {
        offset: 1,
        length: 100,
        instructions: 2,
        checksum: [1; 32],
        header_checksum: [2; 32],
    };
    index.insert("small", &row).unwrap();
    let reserved = index.pending_bytes;
    index.insert("small", &row).unwrap();
    assert_eq!(index.pending_bytes, reserved);
    let long = "x".repeat(1024);
    index.insert(&long, &row).unwrap();
    assert!(index.pending.is_empty());
    assert_eq!(index.pending_bytes, 0);
    assert!(index.get("small").unwrap().is_some());
    assert!(index.get(&long).unwrap().is_some());
}
