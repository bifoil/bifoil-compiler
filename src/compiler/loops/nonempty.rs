//! Nonempty evidence must describe the captured iterable, not later RHS effects.

use crate::compiler::{
    ScopeStack,
    bindings::{Identity, lookup_ref},
    constants,
};
use crate::model::Type;
use crate::syntax::Expr;
use std::collections::BTreeSet;

/// Only immutable declarations record value evidence. Lexical identities keep
/// old scope entries and later bindings with the same name independent.
#[derive(Default)]
pub(in crate::compiler) struct ImmutableValues {
    known: BTreeSet<Identity>,
}

impl ImmutableValues {
    pub(in crate::compiler) fn remember(
        &mut self,
        identity: Identity,
        expression: &Expr,
        actual: &Type,
        target: &Type,
        scopes: &ScopeStack,
    ) {
        if stored_contents_are_fixed(actual, target) && iterable(expression, actual, scopes, self) {
            self.known.insert(identity);
        }
    }
}

/// A readonly binding can still refer to a mutable container. A family copy
/// into an immutable target freezes this container's shape, but erasure does
/// not. Element mutation never changes the container's element count.
fn stored_contents_are_fixed(actual: &Type, target: &Type) -> bool {
    if let Some((_, _, mutable)) = target.present().collection() {
        return !mutable;
    }
    !actual.present().collection().is_some_and(|(_, _, mutable)| mutable)
}

pub(in crate::compiler) fn iterable(
    expression: &Expr,
    actual: &Type,
    scopes: &ScopeStack,
    values: &ImmutableValues,
) -> bool {
    ascending_type(actual) || known_shape(expression, scopes, values)
}

fn ascending_type(ty: &Type) -> bool {
    matches!(ty, Type::Named { name, .. } if name == "IntRange")
}

fn known_shape(expression: &Expr, scopes: &ScopeStack, values: &ImmutableValues) -> bool {
    match expression {
        Expr::Group(value) => known_shape(value, scopes, values),
        // The declared type is invariant under RHS assignments or narrowing.
        // A progression binding might hold an empty descending value, even if
        // the step's RHS later replaces and narrows that same binding.
        Expr::Name(name) => lookup_ref(scopes, name).is_some_and(|binding| {
            ascending_type(&binding.ty)
                || (!binding.mutable && !binding.volatile && values.known.contains(&binding.identity))
        }),
        Expr::String(value) | Expr::Char(value) => !value.is_empty(),
        Expr::Collection { entries, .. } => !entries.is_empty(),
        Expr::Format(parts) => parts.iter().any(|part| known_shape(&part.value, scopes, values)),
        Expr::Binary(operator, left, right) => match operator.as_str() {
            ".." | "..<" => true,
            "downTo" => constants::constant_comparison(">=", left, right) == Some(true),
            // A valid positive step retains the first value of a nonempty input.
            "step" => known_shape(left, scopes, values),
            "+" => known_shape(left, scopes, values) || known_shape(right, scopes, values),
            _ => false,
        },
        _ => false,
    }
}
