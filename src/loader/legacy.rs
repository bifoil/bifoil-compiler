//! Resolve bridge eligibility against all loaded nominal declarations.

use super::Loader;
use crate::legacy as bridge;
use std::collections::BTreeSet;

impl Loader {
    pub(super) fn validate_legacy_interfaces(&self) -> Result<(), String> {
        if self.code.legacy.is_empty() && self.modules.values().all(|module| module.legacy_globals.is_empty()) {
            return Ok(());
        }
        let enums = self
            .modules
            .values()
            .flat_map(|module| &module.types)
            .filter(|(_, definition)| bridge::enum_definition(definition))
            .map(|(name, _)| name.as_str())
            .collect::<BTreeSet<_>>();
        let is_enum = |name: &str| enums.contains(name);
        for module in self.modules.values() {
            for (name, definition) in &module.legacy_globals {
                crate::budget(self.start)?;
                if !bridge::value_type_with(&definition.ty, false, &is_enum) {
                    return Err(format!("legacy binding '{name}' has an unsupported boundary type"));
                }
            }
        }
        for reference in &self.code.legacy {
            crate::budget(self.start)?;
            let function = self
                .code
                .header(&reference.function)
                .map_err(|error| format!("{}:{}: {error}", reference.span.file, reference.span.line))?;
            bridge::validate_signature_with(&function.signature, &reference.target, &is_enum)
                .map_err(|error| format!("{}:{}: {error}", reference.span.file, reference.span.line))?;
        }
        Ok(())
    }
}
