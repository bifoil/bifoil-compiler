//! Generic argument inference for callable candidates, without instruction emission.

use super::super::{ScopeStack, infer};
use super::{CallableSpecialization, ungroup};
use crate::model::{GenericParam, Signature, Type};
use crate::syntax::Expr;
use std::collections::BTreeMap;

pub(super) fn specialize(
    signature: &Signature,
    expression: &Expr,
    arguments: Option<&[(Option<String>, Expr)]>,
    positions: &[usize],
    expected: Option<&Type>,
    env: &ScopeStack,
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
                if let Some(expected) = expected {
                    crate::generics::infer(&signature.result, expected, &signature.generics, &mut types)?;
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
    for ((name, value), index) in arguments.iter().zip(positions) {
        let grouped = signature.params[*index].vararg && (name.is_some() || matches!(value, Expr::Spread(_)));
        let value = if let Expr::Spread(inner) = value {
            inner.as_ref()
        } else {
            value
        };
        if let Ok(mut actual) = infer(value, &scope) {
            if grouped {
                actual = actual.item_type().ok_or("variadic group requires List")?;
            }
            crate::generics::infer(&signature.params[*index].ty, &actual, inferable, types)?;
        }
        super::super::return_inference::normal_completion(value, &mut scope)?;
    }
    Ok(())
}

/// A reference needs a genuine function subtype, not a value-conversion
/// adapter. Return covariance must not replace a bound-valid input argument.
fn infer_reference(signature: &Signature, expected: &Type, types: &mut BTreeMap<String, Type>) -> Result<(), String> {
    let inferred = crate::generics::infer(&signature.ty(), expected, &signature.generics, types)
        .and_then(|_| crate::generics::check(&signature.generics, types));
    let Err(error) = inferred else { return Ok(()) };

    crate::generics::refine_nonnull_arguments(&signature.generics, types);
    if crate::generics::check(&signature.generics, types).is_ok()
        && crate::generics::signature(signature, types).ty().subtype_of(expected)
    {
        return Ok(());
    }
    let Type::Function { params, result, .. } = expected else {
        return Err(error);
    };
    let mut inputs = BTreeMap::new();
    for (parameter, target) in signature.params.iter().zip(params.iter()) {
        crate::generics::infer(&parameter.ty, target, &signature.generics, &mut inputs)?;
    }
    let returned = signature
        .generics
        .iter()
        .filter(|parameter| !inputs.contains_key(&parameter.name))
        .cloned()
        .collect::<Vec<_>>();
    crate::generics::infer(&signature.result, result, &returned, &mut inputs)?;
    crate::generics::check(&signature.generics, &inputs)?;
    if !crate::generics::signature(signature, &inputs).ty().subtype_of(expected) {
        return Err(error);
    }
    *types = inputs;
    Ok(())
}
