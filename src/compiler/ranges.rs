//! Preserve the checked range receiver view without changing stored value identity.

use super::FunctionContext;
use crate::model::{Op, Type};
use serde_json::json;

/// The stack ends with [query, range]. A progression view must use iterable
/// search even when the retained value was constructed as an IntRange.
/// Normalize only a temporary copy, using existing instructions and its step.
pub(super) fn membership_view(receiver: &Type, context: &mut FunctionContext, line: usize) {
    let view = crate::generics::member_view(receiver);
    if !matches!(view, Type::Named { ref name, .. } if name == "IntProgression") {
        return;
    }
    context.emit(Op::Dup, line);
    context.emit(
        Op::Member {
            name: "step".into(),
            ty: Type::Int,
            function: None,
        },
        line,
    );
    context.emit(Op::Dup, line);
    context.emit(
        Op::Constant {
            ty: Type::Int,
            value: json!(0),
        },
        line,
    );
    context.emit(Op::Binary { operator: "<".into() }, line);
    let positive = context.emit(Op::JumpFalse { target: 0 }, line);
    context.emit(Op::Unary { operator: "-".into() }, line);
    context.patch(positive, context.instructions.len());
    context.emit(Op::RangeStep, line);
}
