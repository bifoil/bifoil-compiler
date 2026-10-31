//! Check a forward frame without executing it, then verify its final declaration.
//!
//! Only declaration-scope bindings enter the early check. The final function
//! must retain the same canonical code and signature before earlier facts count.

use super::{Compiler, FunctionContext, ScopeStack};
use crate::model::Function;
use crate::syntax::FunctionAst;
use std::sync::Arc;

pub(super) struct Frame {
    function: Arc<FunctionAst>,
    entry: Arc<ScopeStack>,
    checked_digest: Option<[u8; 32]>,
}

impl Frame {
    pub(super) fn new(function: &FunctionAst, entry: Arc<ScopeStack>) -> Self {
        Self {
            function: Arc::new(function.clone()),
            entry,
            checked_digest: None,
        }
    }

    pub(super) fn verify(&mut self, function: &Function) -> Result<(), String> {
        let actual = crate::artifact::digest_bytes(function)?;
        if let Some(expected) = self.checked_digest {
            if actual != expected {
                return Err("cannot prove forward initializer under its final declaration environment".into());
            }
        } else {
            self.checked_digest = Some(actual);
        }
        Ok(())
    }
}

impl Compiler {
    pub(in crate::compiler) fn precheck_forward_initializer(
        &mut self,
        id: &str,
        context: &mut FunctionContext,
    ) -> Result<bool, String> {
        let Some(plan) = self.single_use_initializers.get(id) else {
            return Ok(false);
        };
        let Some(forward) = plan.forward.as_ref() else {
            return Ok(false);
        };
        if context.function_id != plan.creator || context.scopes.len() < forward.entry.len() {
            return Err("forward initializer requires its declaration scope and creating function".into());
        }
        if forward.checked_digest.is_some() {
            // Branches start from separate scope snapshots. A checked body may
            // be reused, but each direct call must refine its own named binding.
            if let Some(initialized) = self.initialized_captures.get(id) {
                crate::compiler::capture_initialization::refine_entry_reads(
                    &forward.function.name,
                    id,
                    initialized,
                    &mut context.scopes,
                );
            }
            return Ok(true);
        }
        let function = Arc::clone(&forward.function);
        let entry = Arc::clone(&forward.entry);
        let mut visible = context.scopes.clone();
        visible.truncate(entry.len());
        let signature = crate::compiler::lookup(&visible, &function.name)
            .ok_or("forward initializer is unavailable in its declaration scope")?
            .signature_by_id(id);
        let environment = crate::compiler::captures::named_environment(&visible, &entry);
        self.check_local_definition(&function, signature, environment, context)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
