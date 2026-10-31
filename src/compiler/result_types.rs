//! Result inference retains a generic binding's original type beside its view.
//! These alternatives are compile-time facts; only the chosen type is emitted.

use super::{ScopeStack, infer_as};
use crate::model::Type;
use crate::syntax::Expr;

pub(super) struct InferredValue {
    pub(super) ty: Type,
    original: Option<Type>,
}

impl From<Type> for InferredValue {
    fn from(ty: Type) -> Self {
        Self { ty, original: None }
    }
}

pub(super) fn infer_value(
    expression: &Expr,
    env: &ScopeStack,
    expected: Option<&Type>,
) -> Result<InferredValue, String> {
    match expression {
        Expr::Block(body) => return super::values::infer_block_result(body, env, expected),
        Expr::Group(value) => return infer_value(value, env, expected),
        Expr::Unwrap(value) => {
            let source_expected = expected.map(Type::nullable);
            let source = infer_value(value, env, source_expected.as_ref())?;
            return Ok(InferredValue {
                ty: source.ty.non_null(),
                original: source.original.map(|ty| ty.non_null()),
            });
        }
        _ => {}
    }
    let ty = infer_as(expression, env, expected)?;
    let original = if expected.is_none() {
        match expression {
            Expr::Name(name) => super::bindings::lookup_ref(env, name)
                .filter(|binding| binding.narrow.is_some())
                .map(|binding| binding.ty.clone()),
            Expr::Member { .. } | Expr::Index { .. } if super::field_proofs::fact(expression, env).is_some() => {
                super::field_proofs::declared_type(expression, env)
            }
            _ => None,
        }
        // A non-null view belongs to the present part of its original generic
        // type. Keeping the nullable wrapper here would pollute later joins.
        .map(|declared| {
            if ty.permits_null() {
                declared
            } else {
                declared.non_null()
            }
        })
        .filter(|declared| *declared != ty && contains_parameter(declared))
    } else {
        None
    };
    Ok(InferredValue { ty, original })
}

fn contains_parameter(ty: &Type) -> bool {
    // Narrowing must not unfold repeated alias arguments merely to discover
    // that their closed leaves contain no caller-selected parameter.
    ty.any_node(|node| matches!(node, Type::Parameter { .. }))
}

pub(super) fn result_join(values: Vec<InferredValue>, expected: Option<&Type>) -> Result<InferredValue, String> {
    if let Some(expected) = expected {
        for value in values {
            if value.ty != Type::Pending && !expected.accepts(&value.ty) {
                return Err(format!("branch requires {expected}, found {}", value.ty));
            }
        }
        // Contextual conversions fix the result type; no earlier view escapes.
        return Ok(expected.clone().into());
    }
    let views = join_types(values.iter().map(|value| &value.ty));
    let originals = values
        .iter()
        .any(|value| value.original.is_some())
        .then(|| join_types(values.iter().map(|value| value.original.as_ref().unwrap_or(&value.ty))));
    match (views, originals) {
        (Ok(view), Some(Ok(original))) if view != original && original.subtype_of(&view) => Ok(original.into()),
        (Ok(view), Some(Ok(original))) => Ok(InferredValue {
            original: (view != original).then_some(original),
            ty: view,
        }),
        (Ok(view), _) => Ok(view.into()),
        (Err(_), Some(Ok(original))) => Ok(original.into()),
        (Err(error), _) => Err(error),
    }
}

fn join_types<'a>(types: impl Iterator<Item = &'a Type>) -> Result<Type, String> {
    let mut result = Type::Nothing;
    let mut pending = false;
    for ty in types {
        if *ty == Type::Pending {
            // Concrete results anchor the next recursive inference round.
            pending = true;
            continue;
        }
        result = result.join(ty).ok_or("branches have no compatible joined type")?;
    }
    Ok(if pending && result == Type::Nothing {
        Type::Pending
    } else {
        result
    })
}
