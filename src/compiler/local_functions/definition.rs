//! Shared checking and captured-write proofs for a local callable definition.

use crate::compiler::{Compiler, FunctionContext, ScopeStack, lookup};
use crate::model::Signature;
use crate::syntax::FunctionAst;

impl Compiler {
    pub(in crate::compiler) fn check_local_definition(
        &mut self,
        function: &FunctionAst,
        signature: Signature,
        mut environment: ScopeStack,
        context: &mut FunctionContext,
    ) -> Result<(), String> {
        let capture = lookup(&context.scopes, &function.name).unwrap().callable_requirements[&signature.id].clone();
        let targets = crate::compiler::capture_initialization::candidates(
            &capture,
            &context.scopes,
            &context.function_id,
            self.single_use_initializers.get(&signature.id),
        );
        if let Some(plan) = self.single_use_initializers.get(&signature.id) {
            plan.prepare_definition(&mut environment)?;
        }
        let id = signature.id.clone();
        self.function(
            function,
            crate::compiler::functions::FunctionEnvironment::new(&environment),
            signature,
        )?;
        if let Some(plan) = self.single_use_initializers.get_mut(&id) {
            plan.verify_definition(&self.functions[&id])?;
        }
        let initialized =
            crate::compiler::capture_initialization::analyze(&self.functions[&id].code, targets, self.started)?;
        if let Some(plan) = self.single_use_initializers.get(&id) {
            plan.verify_writes(initialized.iter().filter_map(|target| target.single_use_name()))?;
        }
        if !initialized.is_empty() {
            crate::compiler::capture_initialization::refine_entry_reads(
                &function.name,
                &id,
                &initialized,
                &mut context.scopes,
            );
            self.initialized_captures.insert(id, initialized);
        } else {
            self.initialized_captures.remove(&id);
        }
        Ok(())
    }
}
