//! Decode and spool one function before advancing to the next map entry.

use super::{Code, Seed};
use crate::model::{Function, MAX_FUNCTIONS, MAX_INSTRUCTIONS};
use serde::Deserializer;
use serde::de::{DeserializeSeed, Error, MapAccess, Visitor};
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

pub(super) struct Functions(pub(super) Seed);
impl<'de> DeserializeSeed<'de> for Functions {
    type Value = BTreeMap<String, Function<Code>>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for Functions {
    type Value = BTreeMap<String, Function<Code>>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a map of Bifoil functions")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut entries: A) -> Result<Self::Value, A::Error> {
        let mut store = self.0.store.borrow_mut();
        let mut writer = store.pending_writer().map_err(|error| self.0.fatal(error))?;
        let parsed: Result<Self::Value, A::Error> = (|| {
            let mut functions = BTreeMap::new();
            let mut instructions = 0usize;
            while let Some(id) = entries.next_key::<String>()? {
                self.0.check_budget()?;
                if functions.len() >= MAX_FUNCTIONS {
                    return Err(A::Error::custom("module complexity budget exceeded"));
                }
                if functions.contains_key(&id) {
                    return Err(A::Error::custom(format!("duplicate function '{id}'")));
                }
                let function: Function = entries.next_value()?;
                let count = function.code.len();
                instructions = instructions
                    .checked_add(count)
                    .filter(|total| *total <= MAX_INSTRUCTIONS)
                    .ok_or_else(|| A::Error::custom("module complexity budget exceeded"))?;
                let header = Function {
                    signature: function.signature.clone(),
                    code: Code {
                        store: Rc::clone(&self.0.store),
                        failures: Rc::clone(&self.0.failures),
                        id: id.clone(),
                        count,
                        facts: crate::module_validation::body::Facts::collect(&function.code),
                    },
                    init: function.init,
                    local: function.local,
                };
                // This spool is private to this parse. Failed or invalid input drops
                // it; only complete digest-checked modules reach shared pending code.
                writer.record(&id, &function).map_err(|error| self.0.fatal(error))?;
                functions.insert(id, header);
            }
            Ok(functions)
        })();
        // Finish even after a syntax/shape error. Otherwise BufWriter::drop
        // could swallow a private flush failure and turn it into a cache miss.
        writer.finish().map_err(|error| self.0.fatal(error))?;
        self.0.check_budget()?;
        parsed
    }
}
