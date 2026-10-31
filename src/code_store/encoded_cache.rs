//! Bounded canonical-value reuse during one module's package publication.
//!
//! The store still rereads and checksum-verifies bytes on every lookup. This
//! cache only avoids repeating syntax/count checks for the same immutable record.

use super::encoded::EncodedFunction;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

pub(super) struct Cache {
    entries: BTreeMap<String, Arc<EncodedFunction>>,
    retained: usize,
    limit: usize,
}
impl Cache {
    pub(super) fn get(&self, id: &str) -> Option<Arc<EncodedFunction>> {
        self.entries.get(id).map(Arc::clone)
    }

    pub(super) fn insert(&mut self, id: &str, value: Arc<EncodedFunction>) {
        // Reserve serialized bytes, the owned key, and a conservative allowance
        // for tree/Arc allocations. Oversized records remain uncached.
        let allowance = std::mem::size_of::<(String, Arc<EncodedFunction>)>()
            + std::mem::size_of::<EncodedFunction>()
            + std::mem::size_of::<crate::model::SignatureData>()
            + 128;
        let total = value
            .byte_len()
            .checked_add(id.len())
            .and_then(|bytes| bytes.checked_add(allowance))
            .and_then(|bytes| self.retained.checked_add(bytes));
        let Some(total) = total.filter(|total| *total <= self.limit) else {
            return;
        };
        self.retained = total;
        self.entries.insert(id.into(), value);
    }
}

pub(crate) struct Scope<'a> {
    cache: &'a RefCell<Option<Cache>>,
}
impl<'a> Scope<'a> {
    pub(super) fn enter(cache: &'a RefCell<Option<Cache>>, limit: usize) -> Result<Self, String> {
        let mut current = cache.borrow_mut();
        if current.is_some() {
            return Err("canonical code cache scope is already active".into());
        }
        *current = Some(Cache {
            entries: BTreeMap::new(),
            retained: 0,
            limit,
        });
        Ok(Self { cache })
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.cache.borrow_mut().take();
    }
}
