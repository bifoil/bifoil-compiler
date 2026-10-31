//! Shared validation and transfer of independently spooled instruction bodies.
//!
//! Parsed instructions are canonically encoded before hashing, so formatting
//! cannot change identity. Only verified modules may append to pending storage.

pub(crate) mod streamed;

use crate::code_store::CodeStore;
use crate::model::{Function, Instruction, Module};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

pub(crate) trait FunctionCode: Serialize {
    fn structural_facts(&self) -> Arc<crate::module_validation::body::Facts>;
    fn instruction_count(&self) -> Result<usize, String>;
}
impl FunctionCode for Vec<Instruction> {
    fn structural_facts(&self) -> Arc<crate::module_validation::body::Facts> {
        crate::module_validation::body::Facts::collect(self)
    }
    fn instruction_count(&self) -> Result<usize, String> {
        Ok(self.len())
    }
}

pub(crate) struct VerifiedModule<C: FunctionCode>(Module<BTreeMap<String, Function<C>>>);
pub(crate) fn check<C: FunctionCode>(
    mut module: Module<BTreeMap<String, Function<C>>>,
) -> Result<VerifiedModule<C>, String> {
    crate::verify(&mut module)?;
    Ok(VerifiedModule(module))
}
impl VerifiedModule<streamed::Code> {
    pub(crate) fn store(self, store: &mut CodeStore, started: Instant) -> Result<Module, String> {
        streamed::store_verified(self.0, store, started)
    }
}

#[cfg(test)]
mod tests;
