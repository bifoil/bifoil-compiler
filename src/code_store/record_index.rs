//! Small indexes stay in memory; larger tables use a bounded paged store.

mod disk;
use super::Location;
use disk::DiskIndex;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::BTreeMap;

type Locations = BTreeMap<String, Location>;
type Snapshot = RefCell<Option<Locations>>;
const READ_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;

pub(super) struct RecordIndex {
    storage: Storage,
    limit: usize,
    count: usize,
    snapshot: Snapshot,
}
enum Storage {
    Memory(BTreeMap<String, Location>),
    Disk(Box<DiskIndex>),
}
impl Default for RecordIndex {
    fn default() -> Self {
        Self::new(crate::model::MAX_FUNCTIONS)
    }
}
impl RecordIndex {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            storage: Storage::Memory(BTreeMap::new()),
            limit,
            count: 0,
            snapshot: RefCell::new(None),
        }
    }
    pub(super) fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub(super) fn get(&self, id: &str) -> Result<Option<Cow<'_, Location>>, String> {
        match &self.storage {
            Storage::Memory(entries) => Ok(entries.get(id).map(Cow::Borrowed)),
            Storage::Disk(index) => {
                if let Some(value) = self.snapshot.borrow().as_ref().and_then(|rows| rows.get(id)) {
                    return Ok(Some(Cow::Owned(value.clone())));
                }
                index.get(id).map(|value| value.map(Cow::Owned))
            }
        }
    }
    pub(super) fn read_scope<'a>(&'a self, ids: impl IntoIterator<Item = &'a str>) -> Result<ReadScope<'a>, String> {
        self.read_scope_with_limit(ids, READ_SNAPSHOT_BYTES)
    }
    fn read_scope_with_limit<'a>(
        &'a self,
        ids: impl IntoIterator<Item = &'a str>,
        limit: usize,
    ) -> Result<ReadScope<'a>, String> {
        if matches!(self.storage, Storage::Memory(_)) || self.snapshot.borrow().is_some() {
            return Ok(ReadScope { snapshot: None });
        }
        let Storage::Disk(index) = &self.storage else {
            unreachable!()
        };
        let mut rows = BTreeMap::new();
        let mut bytes = 0usize;
        for id in ids {
            let cost = id.len().saturating_add(std::mem::size_of::<(String, Location)>() + 64);
            if bytes.saturating_add(cost) > limit {
                break;
            }
            let row = index.get(id)?.ok_or_else(|| format!("missing stored function {id}"))?;
            rows.insert(id.to_owned(), row);
            bytes += cost;
        }
        *self.snapshot.borrow_mut() = Some(rows);
        Ok(ReadScope {
            snapshot: Some(&self.snapshot),
        })
    }
    pub(super) fn contains_key(&self, id: &str) -> Result<bool, String> {
        Ok(self.get(id)?.is_some())
    }
    pub(super) fn vacant(&mut self, id: String) -> Result<VacantRecord<'_>, String> {
        if self.contains_key(&id)? {
            return Err(format!("duplicate linked function identity {id}"));
        }
        Ok(VacantRecord { index: self, id })
    }
    #[cfg(test)]
    pub(super) fn insert(&mut self, id: String, location: Location) -> Result<(), String> {
        self.vacant(id)?.insert(location)
    }
    fn insert_vacant(&mut self, id: String, location: Location) -> Result<(), String> {
        if let Storage::Memory(entries) = &self.storage
            && self.count >= self.limit
        {
            // Build the replacement before releasing the original table. A
            // failed spill must not lose already committed record identities.
            let mut index = DiskIndex::new()?;
            for (key, value) in entries {
                index.insert(key, value)?;
            }
            index.finish()?;
            self.storage = Storage::Disk(Box::new(index));
        }
        match &mut self.storage {
            Storage::Memory(entries) => {
                entries.insert(id, location);
            }
            Storage::Disk(index) => index.insert(&id, &location)?,
        }
        self.count += 1;
        Ok(())
    }
    pub(super) fn remove(&mut self, id: &str) -> Result<Option<Location>, String> {
        let removed = match &mut self.storage {
            Storage::Memory(entries) => entries.remove(id),
            Storage::Disk(index) => index.remove(id)?,
        };
        if removed.is_some() {
            self.count -= 1;
        }
        Ok(removed)
    }
    pub(super) fn finish(&mut self) -> Result<(), String> {
        if let Storage::Disk(index) = &mut self.storage {
            index.finish()?;
        }
        Ok(())
    }
    pub(super) fn reclaim_empty(&mut self) -> Result<(), String> {
        self.finish()?;
        if self.is_empty() {
            self.storage = Storage::Memory(BTreeMap::new());
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn damage(&mut self, id: &str) -> Result<(), String> {
        match &mut self.storage {
            Storage::Disk(index) => index.damage(id),
            Storage::Memory(_) => Err("fault injection requires a spilled index".into()),
        }
    }
    #[cfg(test)]
    pub(super) fn replace(&mut self, id: &str, location: Location) -> Result<(), String> {
        if !self.contains_key(id)? {
            return Err("missing test index row".into());
        }
        match &mut self.storage {
            Storage::Memory(entries) => {
                entries.insert(id.into(), location);
            }
            Storage::Disk(index) => index.insert(id, &location)?,
        }
        Ok(())
    }
}

/// Holds the exclusive index borrow between duplicate checking and spool append.
/// Dropping it after a serialization failure leaves the identity available.
pub(super) struct VacantRecord<'a> {
    index: &'a mut RecordIndex,
    id: String,
}
impl VacantRecord<'_> {
    pub(super) fn insert(self, location: Location) -> Result<(), String> {
        self.index.insert_vacant(self.id, location)
    }
}

pub(crate) struct ReadScope<'a> {
    snapshot: Option<&'a Snapshot>,
}
impl Drop for ReadScope<'_> {
    fn drop(&mut self) {
        if let Some(snapshot) = self.snapshot {
            snapshot.borrow_mut().take();
        }
    }
}

#[cfg(test)]
mod tests;
