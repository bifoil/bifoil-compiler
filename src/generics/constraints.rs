//! Declared bounds can select existing lossless call-argument widenings.
//! This pass never changes explicit arguments or function-reference signatures.

use crate::model::{GenericParam, Type};
use std::borrow::Cow;
use std::collections::BTreeMap;

pub(crate) fn widen_arguments(parameters: &[GenericParam], arguments: &mut BTreeMap<String, Type>) {
    // Every update replaces scalar leaves by their single widening target.
    // Revisit dependent bounds until those finite choices stop changing.
    loop {
        let mut changed = false;
        for parameter in parameters {
            for bound in &parameter.bounds {
                let Some(actual) = arguments.get(&parameter.name) else {
                    continue;
                };
                let bound = super::substitute(bound, arguments);
                let Some(candidate) = widening(actual, &bound) else {
                    continue;
                };
                if candidate != *actual {
                    arguments.insert(parameter.name.clone(), candidate);
                    changed = true;
                }
            }
        }
        if !changed {
            return;
        }
    }
}

fn widening(actual: &Type, bound: &Type) -> Option<Type> {
    if actual.subtype_of(bound) {
        return Some(actual.clone());
    }
    let view = if matches!(actual, Type::Parameter { .. }) {
        Cow::Owned(super::operation_view(actual))
    } else {
        Cow::Borrowed(actual)
    };
    let candidate = if let Type::Nullable { inner } = bound {
        let present = widening(&view.non_null(), inner)?;
        if view.permits_null() {
            present.nullable()
        } else {
            present
        }
    } else if view.permits_null() {
        return None;
    } else {
        match (view.as_ref(), bound) {
            (Type::Int, Type::Float) => Type::Float,
            (Type::Char, Type::String) => Type::String,
            _ => {
                let (source_kind, source_args, false) = view.collection()? else {
                    return None;
                };
                let (target_kind, target_args, false) = bound.collection()? else {
                    return None;
                };
                if source_kind != target_kind || source_args.len() != target_args.len() {
                    return None;
                }
                let arguments = source_args
                    .iter()
                    .zip(target_args)
                    .map(|(source, target)| widening(source, target))
                    .collect::<Option<Vec<_>>>()?;
                Type::named(source_kind, arguments)
            }
        }
    };
    // Subtyping verifies invariant positions; the conversion rank excludes
    // container-family copies, nullable unwrapping, and signature adapters.
    (candidate.subtype_of(bound) && candidate.conversion_from(actual) == Some(1)).then_some(candidate)
}
