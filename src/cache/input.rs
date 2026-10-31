//! Decode a disposable cache envelope with the shared spooling module seed.

use super::Record;
use crate::module_input::streamed::{Seed, StreamedModule};
use serde::Deserializer;
use serde::de::{DeserializeSeed, Error, IgnoredAny, MapAccess, Visitor};
use std::collections::BTreeSet;
use std::fmt;

/// Expected identities let canonical metadata reject an unusable cache before
/// decoding its module. Other field orders remain supported by the same visitor.
pub(super) struct RecordSeed<'a> {
    module: Seed,
    compiler: &'a str,
    source: &'a str,
}
impl<'a> RecordSeed<'a> {
    pub(super) fn new(module: Seed, compiler: &'a str, source: &'a str) -> Self {
        Self {
            module,
            compiler,
            source,
        }
    }
}
impl<'de> DeserializeSeed<'de> for RecordSeed<'_> {
    type Value = Record<StreamedModule>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for RecordSeed<'_> {
    type Value = Record<StreamedModule>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a Bifoil module-cache record")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut fields: A) -> Result<Self::Value, A::Error> {
        let mut compiler = None;
        let mut source = None;
        let mut module = None;
        let mut seen = BTreeSet::new();
        while let Some(name) = fields.next_key::<String>()? {
            self.module.check_budget()?;
            if !seen.insert(name.clone()) {
                return Err(A::Error::custom(format!("duplicate cache field '{name}'")));
            }
            match name.as_str() {
                "compiler" => {
                    let value: String = fields.next_value()?;
                    if value != self.compiler {
                        return Err(A::Error::custom("cache compiler identity changed"));
                    }
                    compiler = Some(value);
                }
                "source" => {
                    let value: String = fields.next_value()?;
                    if value != self.source {
                        return Err(A::Error::custom("cache source identity changed"));
                    }
                    source = Some(value);
                }
                "module" => module = Some(fields.next_value_seed(self.module.clone())?),
                _ => {
                    fields.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(Record {
            compiler: compiler.ok_or_else(|| A::Error::missing_field("compiler"))?,
            source: source.ok_or_else(|| A::Error::missing_field("source"))?,
            module: module.ok_or_else(|| A::Error::missing_field("module"))?,
        })
    }
}
