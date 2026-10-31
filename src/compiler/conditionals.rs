//! Flat conditional chains, source-ordered tests, and reachable flow joins.

use super::flow::FlowJoin;
use super::{Compiler, FunctionContext, narrow_condition};
use crate::model::{Op, Type};
use crate::syntax::{ConditionalBranch, Statement};
use std::sync::Arc;

impl Compiler {
    pub(super) fn conditional(
        &mut self,
        branches: &[ConditionalBranch],
        no: &[Statement],
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        let mut base = None;
        let mut exits = FlowJoin::default();
        let mut ends = Vec::with_capacity(branches.len());
        for branch in branches {
            let checked = (|| -> Result<(), String> {
                let condition = &branch.condition;
                let line = branch.line;
                if !Type::Boolean.accepts(&self.expression(condition, context, line)?) {
                    return Err("condition requires Boolean".into());
                }
                let failed = context.scopes.clone();
                base.get_or_insert_with(|| failed.clone());
                let incoming = context.is_reachable;
                let jf = context.emit(Op::JumpFalse { target: 0 }, line);
                narrow_condition(condition, &mut context.scopes, true);
                context.is_reachable = incoming && super::constants::constant_boolean(condition) != Some(false);
                context.emit(Op::Enter, line);
                context.scopes.push(Arc::default());
                self.body(&branch.body, context)?;
                context.exit_scope()?;
                context.emit(Op::Exit, line);
                if context.is_reachable {
                    exits.add(base.as_ref().unwrap(), context.scopes.clone());
                }
                ends.push(context.emit(Op::Jump { target: 0 }, line));
                context.patch(jf, context.instructions.len());
                context.scopes = failed;
                context.is_reachable = incoming && super::constants::constant_boolean(condition) != Some(true);
                narrow_condition(condition, &mut context.scopes, false);
                self.check_budget(context)?;
                Ok(())
            })();
            checked.map_err(|error| crate::diagnostics::at_source(&self.file, Some(branch.line), error))?;
        }
        let fallback_line = branches.last().map_or(line, |branch| branch.line);
        let fallback = (|| -> Result<(), String> {
            context.emit(Op::Enter, fallback_line);
            context.scopes.push(Arc::default());
            self.body(no, context)?;
            context.exit_scope()?;
            context.emit(Op::Exit, fallback_line);
            if context.is_reachable {
                exits.add(base.as_ref().unwrap_or(&context.scopes), context.scopes.clone());
            }
            Ok(())
        })();
        fallback.map_err(|error| crate::diagnostics::at_source(&self.file, Some(fallback_line), error))?;
        context.is_reachable = !exits.is_empty();
        context.scopes = exits.finish(base.as_ref().unwrap_or(&context.scopes));
        for end in ends {
            context.patch(end, context.instructions.len());
        }
        Ok(())
    }
}
