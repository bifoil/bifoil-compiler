//! Retry rejected generic calls with directional and alternative constraints.
//!
//! Existing successful inference wins before this pass. Each branch proposes
//! types only. Normal contextual checking must prove every argument, including
//! unique reference resolution, before an instantiation can be selected.

mod arguments;
mod expression_options;
mod literal_types;

use super::super::ScopeStack;
use super::{CallableSpecialization, callbacks};
use crate::generics::call_constraints::Failure;
use crate::model::{Signature, Type};
use crate::syntax::Expr;
use std::collections::BTreeMap;

const MAX_INFERENCE_STATES: usize = 256;

pub(super) fn specialize(
    signature: &Signature,
    supplied: &[(Option<String>, Expr)],
    positions: &[usize],
    expected: Option<&Type>,
    env: &ScopeStack,
) -> Result<(CallableSpecialization, Vec<u8>), String> {
    let deferred = arguments_in_order(signature, supplied, positions, env)?;
    let mut states = vec![BTreeMap::new()];
    let mut selected: Option<(CallableSpecialization, Vec<u8>)> = None;
    let mut cursor = 0;
    while cursor < states.len() {
        let current = states[cursor].clone();
        cursor += 1;
        for constraints in arguments::alternatives(signature, &deferred, expected, &current)? {
            let mut proposed = match constraints.solve() {
                Ok(types) => types,
                Err(Failure::Budget) => return Err(Failure::Budget.to_string()),
                Err(Failure::Conflict) => continue,
            };
            // The existing partial-reference pass may fill parameters still
            // absent from this branch, but cannot overwrite directional facts.
            let missing = signature
                .generics
                .iter()
                .filter(|parameter| !proposed.contains_key(&parameter.name))
                .cloned()
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                callbacks::resolve(&deferred, &missing, &mut proposed)?;
            }
            if proposed.values().any(|ty| !ty.within_depth(32)) {
                return Err(Failure::Budget.to_string());
            }
            if proposed != current {
                if !states.contains(&proposed) {
                    if states.len() == MAX_INFERENCE_STATES {
                        return Err(Failure::Budget.to_string());
                    }
                    states.push(proposed);
                }
                continue;
            }
            let Ok(callable) = complete(signature, proposed, expected) else {
                continue;
            };
            let Ok(ranks) = super::argument_ranks(&callable.signature, supplied, positions, env) else {
                continue;
            };
            if let Some((previous, _)) = &selected {
                if previous.type_arguments != callable.type_arguments {
                    return Err("ambiguous generic inference; supply explicit type arguments".into());
                }
            } else {
                selected = Some((callable, ranks));
            }
        }
    }
    selected.ok_or_else(|| Failure::Conflict.to_string())
}

fn complete(
    signature: &Signature,
    types: BTreeMap<String, Type>,
    expected: Option<&Type>,
) -> Result<CallableSpecialization, String> {
    crate::generics::check(&signature.generics, &types)?;
    for ty in types.values() {
        crate::declarations::validate_type(ty)?;
    }
    let specialized = crate::generics::signature(signature, &types);
    if expected.is_some_and(|expected| !expected.accepts(&specialized.result)) {
        return Err(Failure::Conflict.to_string());
    }
    Ok(CallableSpecialization {
        signature: specialized,
        type_arguments: types,
    })
}

fn arguments_in_order<'a>(
    signature: &Signature,
    supplied: &'a [(Option<String>, Expr)],
    positions: &[usize],
    env: &ScopeStack,
) -> Result<Vec<callbacks::Deferred<'a>>, String> {
    let mut scope = env.clone();
    let mut deferred = vec![];
    for ((name, value), index) in supplied.iter().zip(positions) {
        let parameter = &signature.params[*index];
        let grouped = parameter.vararg && (name.is_some() || matches!(value, Expr::Spread(_)));
        let value = if let Expr::Spread(inner) = value {
            inner.as_ref()
        } else {
            value
        };
        deferred.push(callbacks::Deferred {
            value,
            pattern: if grouped {
                Type::named("List", vec![parameter.ty.clone()])
            } else {
                parameter.ty.clone()
            },
            scope: scope.clone(),
        });
        super::super::return_inference::normal_completion(value, &mut scope)?;
    }
    Ok(deferred)
}
