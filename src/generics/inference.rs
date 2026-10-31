//! Infer call type arguments from value shapes and declared subtype paths.
//! Final applicability still checks every conversion, invariant argument,
//! variadic shape, and suspension effect against the specialized signature.

use crate::model::{GenericParam, Type};
use crate::types::graph::RetainedNode;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

const MAX_QUERY_PAIRS: usize = 4096;

pub(crate) fn infer(
    pattern: &Type,
    actual: &Type,
    inferable: &[GenericParam],
    args: &mut BTreeMap<String, Type>,
) -> Result<(), String> {
    InferenceQuery::default().infer(pattern, actual, inferable, args)
}

/// Repeating an identical structural constraint adds no new information.
/// Each supplied argument starts a fresh query, retaining the caller's joins.
#[derive(Default)]
struct InferenceQuery {
    checked: BTreeSet<(RetainedNode, RetainedNode)>,
}
impl InferenceQuery {
    fn infer(
        &mut self,
        pattern: &Type,
        actual: &Type,
        inferable: &[GenericParam],
        args: &mut BTreeMap<String, Type>,
    ) -> Result<(), String> {
        // A projection hides an argument; it cannot select a concrete instantiation.
        if *actual == Type::Star {
            return Ok(());
        }
        // One argument may repeat entire callable or scene subgraphs. Cache
        // their completed constraints just as we do nominal argument graphs.
        let actual = if matches!(pattern, Type::Named { .. } | Type::Function { .. } | Type::Scene { .. }) {
            upper_view(actual)
        } else {
            Cow::Borrowed(actual)
        };
        let key = RetainedNode::of(pattern).zip(RetainedNode::of(&actual));
        if key.as_ref().is_some_and(|key| self.checked.contains(key)) {
            return Ok(());
        }
        self.constraints(pattern, &actual, inferable, args)?;
        if let Some(key) = key
            && self.checked.len() < MAX_QUERY_PAIRS
        {
            self.checked.insert(key);
        }
        Ok(())
    }

    fn constraints(
        &mut self,
        pattern: &Type,
        actual: &Type,
        inferable: &[GenericParam],
        args: &mut BTreeMap<String, Type>,
    ) -> Result<(), String> {
        match pattern {
            Type::Parameter { name, .. } if inferable.iter().any(|parameter| parameter.name == *name) => {
                // Preserve the supplied parameter's identity. Only structural
                // patterns below may inspect its declared upper-bound shape.
                let joined = if let Some(old) = args.get(name) {
                    old.join(actual)
                        .ok_or("generic arguments have incompatible constraints")?
                } else {
                    actual.clone()
                };
                args.insert(name.clone(), joined);
            }
            Type::Nullable { inner } => {
                if actual != &Type::Null {
                    self.infer(inner, actual.present(), inferable, args)?;
                }
            }
            Type::Named { name, args: parameters } => {
                if let Type::Named {
                    name: actual_name,
                    args: arguments,
                } = actual
                    && name == actual_name
                {
                    for (parameter, argument) in parameters.iter().zip(arguments.iter()) {
                        self.infer(parameter, argument, inferable, args)?;
                    }
                } else if let Some(parent) = crate::declarations::parent(actual) {
                    // A supplied variant can satisfy its enclosing sum parameter.
                    self.infer(pattern, &parent, inferable, args)?;
                } else if let Some(parent) = crate::declarations::parent(pattern) {
                    // An expected sum can constrain a variant-valued result.
                    self.infer(&parent, actual, inferable, args)?;
                }
            }
            Type::Function { params, result, .. } => {
                if let Type::Function {
                    params: arguments,
                    result: returned,
                    ..
                } = actual
                {
                    for (parameter, argument) in params.iter().zip(arguments.iter()) {
                        self.infer(parameter, argument, inferable, args)?;
                    }
                    self.infer(result, returned, inferable, args)?;
                }
            }
            Type::Scene { params } => {
                if let Type::Scene { params: arguments } = actual {
                    for (parameter, argument) in params.iter().zip(arguments.iter()) {
                        self.infer(parameter, argument, inferable, args)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Bounds provide a usable interface, not permission to unwrap a nullable
/// value or convert it. Ordinary concrete shapes stay borrowed.
fn upper_view(actual: &Type) -> Cow<'_, Type> {
    if matches!(actual, Type::Parameter { .. }) {
        Cow::Owned(super::member_view(actual))
    } else {
        Cow::Borrowed(actual)
    }
}

/// A non-null bound may refine a contextual argument without changing its
/// identity. Applicability must still prove that the supplied value is present.
pub(crate) fn refine_nonnull_arguments(parameters: &[GenericParam], args: &mut BTreeMap<String, Type>) {
    loop {
        let mut changed = false;
        for parameter in parameters {
            let Some(actual) = args.get(&parameter.name).filter(|actual| actual.permits_null()) else {
                continue;
            };
            let nonnull_bound = parameter.bounds.iter().any(|bound| {
                let bound = super::substitute(bound, args);
                !matches!(&bound, Type::Named { name, .. } if name == "Equatable" || name == "StableKey")
                    && !bound.permits_null()
            });
            if nonnull_bound {
                args.insert(parameter.name.clone(), actual.non_null());
                changed = true;
            }
        }
        if !changed {
            return;
        }
    }
}
