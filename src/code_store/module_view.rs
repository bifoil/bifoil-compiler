//! Canonical module publication and read-only analysis over stored bodies.

use super::{CodeStore, StoredFunctions};
use crate::model::{Function, Module};
use std::collections::BTreeMap;
use std::time::Instant;

impl CodeStore {
    /// Move metadata through the existing canonical serializer without cloning it.
    /// Return the original checked headers even when inspection fails; code stays
    /// in this store throughout hashing or publication.
    pub(crate) fn inspect_module<R>(
        &self,
        mut module: Module,
        inspect: impl FnOnce(&Module<StoredFunctions<'_>>) -> Result<R, String>,
    ) -> (Module, Result<R, String>) {
        if let Some((id, _)) = module.functions.iter().find(|(_, function)| !function.code.is_empty()) {
            let error = format!("stored module still has instructions: {id}");
            return (module, Err(error));
        }
        let headers = std::mem::take(&mut module.functions);
        let stored = module.with_functions(self.functions(&headers));
        let result = inspect(&stored);
        let mut module = stored.with_functions(BTreeMap::new());
        module.functions = headers;
        (module, result)
    }

    /// Analyze one selected, checksum-checked body at a time without removing any
    /// pending records. Other functions contribute their resident headers only.
    pub(crate) fn inspect_functions(
        &self,
        headers: &BTreeMap<String, Function>,
        started: Instant,
        requires_body: impl Fn(&Function) -> bool,
        mut inspect: impl FnMut(&str, &Function) -> Result<(), String>,
    ) -> Result<(), String> {
        let _records = self.record_scope(headers.keys().map(String::as_str))?;
        for (id, header) in headers {
            crate::budget(started)?;
            if !header.code.is_empty() {
                return Err(format!("pending function already has instructions: {id}"));
            }
            if requires_body(header) {
                let encoded = self.encoded_checked(id, header)?;
                let function = encoded
                    .decoded()
                    .map_err(|error| format!("stored function {id}: {error}"))?;
                inspect(id, &function)?;
            }
        }
        crate::budget(started)
    }
}
