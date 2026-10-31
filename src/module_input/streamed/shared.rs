//! Intern shared dependency lists as each definition is decoded, before retaining
//! another complete definition. Duplicate keys cannot silently replace metadata.

use super::Seed;
use crate::model::{MAX_FUNCTIONS, SharedDef};
use crate::shared_dependencies::Interner;
use serde::Deserializer;
use serde::de::{DeserializeSeed, Error, MapAccess, Visitor};
use std::collections::BTreeMap;
use std::fmt;

pub(super) struct Shared(pub(super) Seed);
impl<'de> DeserializeSeed<'de> for Shared {
    type Value = BTreeMap<String, SharedDef>;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for Shared {
    type Value = BTreeMap<String, SharedDef>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a map of Bifoil shared definitions")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut entries: A) -> Result<Self::Value, A::Error> {
        let mut definitions = BTreeMap::new();
        let mut dependencies = Interner::default();
        while let Some(id) = entries.next_key::<String>()? {
            self.0.check_budget()?;
            if definitions.len() >= MAX_FUNCTIONS {
                return Err(A::Error::custom("module complexity budget exceeded"));
            }
            if definitions.contains_key(&id) {
                return Err(A::Error::custom(format!("duplicate shared binding '{id}'")));
            }
            let mut definition: SharedDef = entries.next_value()?;
            definition.dependencies = dependencies.intern(definition.dependencies);
            definitions.insert(id, definition);
        }
        Ok(definitions)
    }
}
