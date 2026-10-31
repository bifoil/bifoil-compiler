//! Storage and callables share the VM's load namespace across module boundaries.

use super::Loader;
use std::collections::BTreeMap;

impl Loader {
    pub(super) fn validate_runtime_identities(&self) -> Result<(), String> {
        // Only storage needs an auxiliary index. Callable bodies and signatures
        // remain in their module maps, avoiding a second whole-program catalog.
        let mut storage = BTreeMap::new();
        for (owner, module) in &self.modules {
            for (kind, id) in module
                .shared
                .keys()
                .map(|id| ("shared binding", id))
                .chain(module.legacy_globals.keys().map(|id| ("legacy binding", id)))
            {
                crate::budget(self.start)?;
                if let Some((other, previous)) = storage.insert(id.as_str(), (owner.as_str(), kind)) {
                    return Err(format!(
                        "linked runtime identity '{id}' collides: {previous} in {other} and {kind} in {owner}"
                    ));
                }
            }
        }
        if storage.is_empty() {
            return Ok(());
        }
        for (owner, module) in &self.modules {
            for (index, id) in module.functions.keys().enumerate() {
                if index % 1024 == 0 {
                    crate::budget(self.start)?;
                }
                if let Some((other, kind)) = storage.get(id.as_str()) {
                    return Err(format!(
                        "linked runtime identity '{id}' collides: callable in {owner} and {kind} in {other}"
                    ));
                }
            }
        }
        Ok(())
    }
}
