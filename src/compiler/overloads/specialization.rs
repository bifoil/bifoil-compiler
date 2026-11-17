//! Generic argument inference for callable candidates, without instruction emission.

use super::super::{ScopeStack, infer};
use super::{CallableSpecialization, ungroup};
use crate::model::{GenericParam, Signature, Type};
use crate::syntax::Expr;
use std::borrow::Cow;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(super) enum ResultInference {
    Join,
    Missing,
}

pub(super) fn specialize(
    signature: &Signature,
    expression: &Expr,
    arguments: Option<&[(Option<String>, Expr)]>,
    positions: &[usize],
    expected: Option<&Type>,
    env: &ScopeStack,
    result_inference: ResultInference,
) -> Result<CallableSpecialization, String> {
    let mut types = BTreeMap::new();
    let explicit = match expression {
        Expr::Group(inner) => ungroup(inner),
        _ => expression,
    };
    if let Expr::TypeApply { args, .. } = explicit {
        for argument in args {
            crate::declarations::validate_type(argument)?;
        }
        if signature.generics.is_empty() {
            return Err("nongeneric callable does not accept type arguments".into());
        }
        if args.len() != signature.generics.len() {
            return Err("incorrect generic argument count".into());
        }
        types.extend(
            signature
                .generics
                .iter()
                .zip(args)
                .map(|(p, t)| (p.name.clone(), t.clone())),
        );
    } else if !signature.generics.is_empty() {
        if let Some(arguments) = arguments {
            let supplied = infer_supplied(signature, arguments, positions, &signature.generics, env, &mut types);
            if supplied.is_ok() {
                crate::generics::widen_arguments(&signature.generics, &mut types);
            }
            let supplied_types = supplied.is_ok().then(|| types.clone());
            let inferred = supplied.and_then(|_| {
                if let Some(expected) = expected
                    && !(matches!(result_inference, ResultInference::Join)
                        && preserve_supplied_result(signature, expected, &mut types))
                {
                    let inferable = match result_inference {
                        ResultInference::Join => Cow::Borrowed(signature.generics.as_slice()),
                        ResultInference::Missing => Cow::Owned(
                            signature
                                .generics
                                .iter()
                                .filter(|parameter| !types.contains_key(&parameter.name))
                                .cloned()
                                .collect::<Vec<_>>(),
                        ),
                    };
                    crate::generics::infer(&signature.result, expected, &inferable, &mut types)?;
                }
                crate::generics::check(&signature.generics, &types)
            });
            if let Err(error) = inferred {
                let Some(expected) = expected else { return Err(error) };
                if let Some(supplied) = supplied_types
                    && crate::generics::check(&signature.generics, &supplied).is_ok()
                    && expected.accepts(&crate::generics::substitute(&signature.result, &supplied))
                {
                    // Widen the completed result instead of choosing an argument
                    // that violates a declared bound, such as Any for Equatable.
                    types = supplied;
                } else {
                    // A refined view can conflict with an enclosing caller-selected
                    // result type even though the unchanged argument still has that
                    // original identity. Retry under the explicit result context;
                    // final applicability must prove every argument against it.
                    types.clear();
                    crate::generics::infer(&signature.result, expected, &signature.generics, &mut types)?;
                    crate::generics::refine_nonnull_arguments(&signature.generics, &mut types);
                    let inferable = signature
                        .generics
                        .iter()
                        .filter(|parameter| !types.contains_key(&parameter.name))
                        .cloned()
                        .collect::<Vec<_>>();
                    infer_supplied(signature, arguments, positions, &inferable, env, &mut types)?;
                }
            }
        } else if let Some(expected) = expected {
            infer_reference(signature, expected.present(), &mut types)?;
        }
    }
    crate::generics::check(&signature.generics, &types)?;
    Ok(CallableSpecialization {
        signature: crate::generics::signature(signature, &types),
        type_arguments: types,
    })
}

/// Preserve supplied type arguments when a genuine result subtype already fits.
/// Expected types may fill missing parameters first, but value conversions and
/// invariant positions still use ordinary contextual inference when needed.
fn preserve_supplied_result(signature: &Signature, expected: &Type, types: &mut BTreeMap<String, Type>) -> bool {
    let fits = |arguments: &BTreeMap<String, Type>| {
        crate::generics::check(&signature.generics, arguments).is_ok()
            && crate::generics::substitute(&signature.result, arguments).subtype_of(expected)
    };
    if fits(types) {
        return true;
    }
    let missing = signature
        .generics
        .iter()
        .filter(|parameter| !types.contains_key(&parameter.name))
        .cloned()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return false;
    }
    let mut completed = types.clone();
    if crate::generics::infer(&signature.result, expected, &missing, &mut completed).is_err() || !fits(&completed) {
        return false;
    }
    *types = completed;
    true
}

/// Raw supplied values establish compatible joins. Empty literals and
/// overloaded references may instead need the completed parameter context.
fn infer_supplied(
    signature: &Signature,
    arguments: &[(Option<String>, Expr)],
    positions: &[usize],
    inferable: &[GenericParam],
    env: &ScopeStack,
    types: &mut BTreeMap<String, Type>,
) -> Result<(), String> {
    let mut scope = env.clone();
    let mut deferred = vec![];
    for ((name, value), index) in arguments.iter().zip(positions) {
        let grouped = signature.params[*index].vararg && (name.is_some() || matches!(value, Expr::Spread(_)));
        let value = if let Expr::Spread(inner) = value {
            inner.as_ref()
        } else {
            value
        };
        let actual = if super::callbacks::requires_context(value, &scope) {
            None
        } else {
            infer(value, &scope).ok()
        };
        if let Some(mut actual) = actual {
            if grouped {
                actual = actual.item_type().ok_or("variadic group requires List")?;
            }
            crate::generics::infer(&signature.params[*index].ty, &actual, inferable, types)?;
        } else {
            let element = signature.params[*index].ty.clone();
            deferred.push(super::callbacks::Deferred {
                value,
                pattern: if grouped {
                    Type::named("List", vec![element])
                } else {
                    element
                },
                scope: scope.clone(),
            });
        }
        super::super::return_inference::normal_completion(value, &mut scope)?;
    }
    super::callbacks::resolve(&deferred, inferable, types)
}

/// A reference needs a genuine function subtype, not a value-conversion
/// adapter. Return covariance must not replace a bound-valid input argument.
fn infer_reference(signature: &Signature, expected: &Type, types: &mut BTreeMap<String, Type>) -> Result<(), String> {
    let options = super::reference_inference::options(signature, expected, &[])?;
    if let Some(selected) = options
        .arguments
        .iter()
        .find(|types| crate::generics::signature(signature, types).ty().subtype_of(expected))
    {
        *types = selected.clone();
        return Ok(());
    }
    if let Some(error) = options.initial_error {
        return Err(error);
    }
    // An initially bound-valid map retains the old reference mismatch diagnostic
    // in the caller, which performs the final subtype check.
    *types = options.arguments[0].clone();
    Ok(())
}
