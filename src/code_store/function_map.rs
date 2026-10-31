//! One serializer for standalone module maps and grouped program headers.

use super::CodeStore;
use crate::function_catalog::{FunctionCatalog, HeaderDigest};
use crate::model::Function;
use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};
use std::collections::BTreeMap;

/// Both forms verify the same stored payload. Standalone modules retain full
/// headers; linked publication consumes those headers into canonical digests.
pub(crate) trait HeaderProof {
    fn encode(&self, store: &CodeStore, id: &str) -> Result<impl Serialize, String>;
}
impl HeaderProof for Function {
    fn encode(&self, store: &CodeStore, id: &str) -> Result<impl Serialize, String> {
        store.encoded_checked(id, self)
    }
}
impl HeaderProof for HeaderDigest {
    fn encode(&self, store: &CodeStore, id: &str) -> Result<impl Serialize, String> {
        store.encoded_expected(id, &self.0)
    }
}

/// Entries must be in canonical function-ID order, independent of module order.
pub(crate) trait Headers {
    type Header: HeaderProof;
    fn function_count(&self) -> usize;
    fn entries(&self) -> impl Iterator<Item = (&str, &Self::Header)>;
}
impl<H: HeaderProof> Headers for BTreeMap<String, H> {
    type Header = H;
    fn function_count(&self) -> usize {
        self.len()
    }
    fn entries(&self) -> impl Iterator<Item = (&str, &H)> {
        self.iter().map(|(id, function)| (id.as_str(), function))
    }
}
impl<H: HeaderProof> Headers for FunctionCatalog<H> {
    type Header = H;
    fn function_count(&self) -> usize {
        self.len()
    }
    fn entries(&self) -> impl Iterator<Item = (&str, &H)> {
        self.sorted()
    }
}

pub(crate) struct StoredFunctions<'a, H: Headers + ?Sized = BTreeMap<String, Function>> {
    pub(super) store: &'a CodeStore,
    pub(super) headers: &'a H,
}
impl<H: Headers + ?Sized> Serialize for StoredFunctions<'_, H> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let _records = self
            .store
            .record_scope(self.headers.entries().map(|(id, _)| id))
            .map_err(serde::ser::Error::custom)?;
        let mut output = serializer.serialize_map(Some(self.headers.function_count()))?;
        for (id, header) in self.headers.entries() {
            let function = header.encode(self.store, id).map_err(serde::ser::Error::custom)?;
            output.serialize_entry(id, &function)?;
        }
        output.end()
    }
}
