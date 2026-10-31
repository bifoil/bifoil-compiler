//! Prose evaluation and structural output scopes.

use super::{Compiler, FunctionContext};
use crate::model::{Op, Type};
use crate::syntax::{Expr, Statement};
use std::sync::Arc;

impl Compiler {
    pub(super) fn prose_block(
        &mut self,
        body: &[Statement],
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        context.emit(Op::ProseStart, line);
        context.prose_depth += 1;
        context.emit(Op::Enter, line);
        context.scopes.push(Arc::default());
        self.body(body, context)?;
        context.exit_scope()?;
        context.emit(Op::Exit, line);
        context.emit(Op::ProseEnd, line);
        context.prose_depth -= 1;

        Ok(())
    }

    pub(super) fn prose_text(
        &mut self,
        value: &Expr,
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        let effect = context.may_suspend;
        context.may_suspend = false;
        if self.expression(value, context, line)? != Type::String {
            return Err("prose text must be String".into());
        }
        context.may_suspend = effect;
        context.emit(Op::ProseText, line);

        Ok(())
    }
}
