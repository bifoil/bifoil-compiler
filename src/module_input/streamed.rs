//! Stream existing JSON modules into a private, scoped instruction spool.
//!
//! Only function headers remain resident. The public format, canonical digest,
//! and structural checks stay shared with encoded cache records.

mod functions;
mod shared;

use super::FunctionCode;
use crate::code_store::CodeStore;
use crate::model::{Function, Module};
use serde::de::{DeserializeSeed, Error, IgnoredAny, MapAccess, Visitor};
use serde::{Deserializer, Serialize, Serializer};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

pub(crate) struct Code {
    store: Rc<RefCell<CodeStore>>,
    failures: Rc<RefCell<Option<String>>>,
    id: String,
    count: usize,
    facts: Arc<crate::module_validation::body::Facts>,
}
impl Serialize for Code {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let function = self
            .store
            .borrow()
            .encoded(&self.id)
            .map_err(|error| serde::ser::Error::custom(record_failure(&self.failures, error)))?;
        function.serialize_code(serializer)
    }
}
impl FunctionCode for Code {
    fn structural_facts(&self) -> Arc<crate::module_validation::body::Facts> {
        Arc::clone(&self.facts)
    }
    fn instruction_count(&self) -> Result<usize, String> {
        Ok(self.count)
    }
}
pub(crate) type StreamedModule = Module<BTreeMap<String, Function<Code>>>;

/// Move a validated module's canonical bodies without decoding them again.
/// Every body must belong to the same private parse, whose headers were checked.
pub(super) fn store_verified(
    mut module: StreamedModule,
    target: &mut CodeStore,
    started: Instant,
) -> Result<Module, String> {
    let mut source = None;
    let mut headers = BTreeMap::new();
    for (id, function) in std::mem::take(&mut module.functions) {
        crate::budget(started)?;
        if function.code.id != id
            || source
                .as_ref()
                .is_some_and(|owner| !Rc::ptr_eq(owner, &function.code.store))
        {
            return Err("validated module contains unrelated private function storage".into());
        }
        source.get_or_insert_with(|| Rc::clone(&function.code.store));
        headers.insert(
            id,
            Function {
                signature: function.signature,
                code: Vec::new(),
                init: function.init,
                local: function.local,
            },
        );
    }
    if let Some(source) = source {
        target.copy_pending_from(&mut source.borrow_mut(), &headers, started)?;
    }
    Ok(module.with_functions(headers))
}

pub(crate) fn read(
    path: &Path,
    limit: u64,
    aggregate: Option<&mut u64>,
    started: Instant,
) -> Result<(StreamedModule, String), String> {
    crate::input::json_seed_hashed(path, limit, aggregate, Seed::new(started))
}

#[derive(Clone)]
pub(crate) struct Seed {
    store: Rc<RefCell<CodeStore>>,
    failures: Rc<RefCell<Option<String>>>,
    started: Instant,
}
impl Seed {
    pub(crate) fn new(started: Instant) -> Self {
        Self {
            store: Rc::new(RefCell::new(CodeStore::default())),
            failures: Rc::new(RefCell::new(None)),
            started,
        }
    }
    pub(crate) fn take_failure(&self) -> Option<String> {
        self.failures.borrow_mut().take()
    }
    pub(crate) fn check_budget<E: Error>(&self) -> Result<(), E> {
        crate::budget(self.started).map_err(|error| self.fatal(error))
    }
    fn fatal<E: Error>(&self, error: String) -> E {
        E::custom(record_failure(&self.failures, error))
    }
}

/// Malformed JSON or module metadata may invalidate a disposable cache. A
/// private-spool I/O failure or deadline failure must never become a cache miss.
fn record_failure(state: &RefCell<Option<String>>, error: String) -> String {
    *state.borrow_mut() = Some(error.clone());
    error
}
impl<'de> DeserializeSeed<'de> for Seed {
    type Value = StreamedModule;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for Seed {
    type Value = StreamedModule;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a Bifoil module object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut fields: A) -> Result<Self::Value, A::Error> {
        let mut module = Module {
            summaries: BTreeMap::new(),
            group: None,
            legacy_globals: BTreeMap::new(),
            shared: BTreeMap::new(),
            types: BTreeMap::new(),
            format: String::new(),
            profile: String::new(),
            name: String::new(),
            digest: String::new(),
            dependencies: BTreeMap::new(),
            exports: BTreeMap::new(),
            functions: BTreeMap::new(),
            init: None,
        };
        let mut seen = BTreeSet::new();
        while let Some(name) = fields.next_key::<String>()? {
            self.check_budget()?;
            if !seen.insert(name.clone()) {
                return Err(A::Error::custom(format!("duplicate module field '{name}'")));
            }
            match name.as_str() {
                "group" => module.group = fields.next_value()?,
                "legacy_globals" => module.legacy_globals = fields.next_value()?,
                "shared" => module.shared = fields.next_value_seed(shared::Shared(self.clone()))?,
                "types" => module.types = fields.next_value()?,
                "format" => module.format = fields.next_value()?,
                "profile" => module.profile = fields.next_value()?,
                "name" => module.name = fields.next_value()?,
                "digest" => module.digest = fields.next_value()?,
                "dependencies" => module.dependencies = fields.next_value()?,
                "exports" => module.exports = fields.next_value()?,
                "functions" => module.functions = fields.next_value_seed(functions::Functions(self.clone()))?,
                "init" => module.init = fields.next_value()?,
                _ => {
                    fields.next_value::<IgnoredAny>()?;
                }
            }
        }
        for required in [
            "format",
            "profile",
            "name",
            "digest",
            "dependencies",
            "exports",
            "functions",
        ] {
            if !seen.contains(required) {
                return Err(A::Error::missing_field(required));
            }
        }
        Ok(module)
    }
}

#[cfg(test)]
mod tests;
