//! Recover constraints from unresolved references and nested literal arguments.
//!
//! Lookup never selects a callable here. Only facts shared by all viable
//! declarations escape; ordinary reference resolution remains responsible for
//! choosing a unique overload and checking access, variance, and effects.

use super::super::{ScopeStack, infer, infer_as};
use super::specialization::{ResultInference, specialize};
use crate::model::{GenericParam, Signature, Type};
use crate::syntax::Expr;
use std::collections::BTreeMap;

pub(super) struct Deferred<'a> {
    pub(super) value: &'a Expr,
    pub(super) pattern: Type,
    pub(super) scope: ScopeStack,
}

pub(super) fn requires_context(expression: &Expr, env: &ScopeStack) -> bool {
    match expression {
        Expr::Group(inner) => requires_context(inner, env),
        Expr::Collection { entries, .. } => entries.iter().any(|(key, value)| {
            key.as_ref().is_some_and(|key| requires_context(key, env)) || requires_context(value, env)
        }),
        _ => {
            super::callable_candidates(expression, env).len() > 1 && super::reference_needs_resolution(expression, env)
        }
    }
}

pub(super) fn resolve(
    deferred: &[Deferred<'_>],
    inferable: &[GenericParam],
    types: &mut BTreeMap<String, Type>,
) -> Result<(), String> {
    if deferred.is_empty() {
        return Ok(());
    }
    // A growing recursive constraint can have no finite inferred type. Keep
    // the existing type-depth budget instead of constructing unbounded graphs.
    loop {
        let before = types.clone();
        for argument in deferred {
            constrain(argument.value, &argument.pattern, inferable, &argument.scope, types)?;
            check_depth(types)?;
        }
        if *types == before {
            return Ok(());
        }
    }
}

fn constrain(
    expression: &Expr,
    pattern: &Type,
    inferable: &[GenericParam],
    env: &ScopeStack,
    types: &mut BTreeMap<String, Type>,
) -> Result<(), String> {
    let hint = crate::generics::substitute(pattern, types);
    let structural = matches!(hint.present(), Type::Function { .. }) || hint.present().collection().is_some();
    if (!structural || !requires_context(expression, env))
        && let Ok(actual) = infer(expression, env)
    {
        crate::generics::infer(pattern, &actual, inferable, types)?;
        return check_depth(types);
    }
    if let Expr::Group(inner) = expression {
        return constrain(inner, pattern, inferable, env, types);
    }
    if let Expr::Collection { entries, .. } = expression
        && let Some((family, hinted, _)) = hint.present().collection()
    {
        let arguments = pattern
            .present()
            .collection()
            .map(|(_, args, _)| args)
            .unwrap_or(hinted);
        let mut scope = env.clone();
        for (key, value) in entries {
            if let Some(key) = key {
                if family != "Map" {
                    return Ok(());
                }
                constrain(key, &arguments[0], inferable, &scope, types)?;
                super::super::return_inference::normal_completion(key, &mut scope)?;
            }
            constrain(value, arguments.last().unwrap(), inferable, &scope, types)?;
            super::super::return_inference::normal_completion(value, &mut scope)?;
        }
        return Ok(());
    }
    if matches!(hint.present(), Type::Function { .. }) && super::reference_needs_resolution(expression, env) {
        super::super::members::check_static_receiver(expression, env)?;
        let unresolved = inferable
            .iter()
            .filter(|parameter| !types.contains_key(&parameter.name))
            .map(|parameter| parameter.name.clone())
            .collect::<Vec<_>>();
        let mut viable = vec![];
        for declaration in super::visible_candidates(expression, env)? {
            for actual in reference_types(&declaration, expression, hint.present(), &unresolved, env) {
                let mut proposed = types.clone();
                if crate::generics::infer(pattern, &actual, inferable, &mut proposed).is_ok()
                    && actual.subtype_of(&crate::generics::substitute(pattern, &proposed))
                {
                    viable.push(proposed);
                    // Keep the declaration's existing generic inference order.
                    break;
                }
            }
        }
        if let Some(first) = viable.first() {
            for (name, ty) in first {
                if viable.iter().all(|candidate| candidate.get(name) == Some(ty)) {
                    types.insert(name.clone(), ty.clone());
                }
            }
        }
        return check_depth(types);
    }
    // Other contextual expressions can now use arguments learned later in the
    // call. Never expose an unbound callee parameter as a concrete hint.
    let unresolved = hint.any_node(|node| {
        matches!(node, Type::Parameter { name, .. }
        if inferable.iter().any(|p| p.name == *name) && !types.contains_key(name))
    });
    if !unresolved
        && !hint.any_node(|node| matches!(node, Type::Star | Type::Pending))
        && let Ok(actual) = infer_as(expression, env, Some(&hint))
    {
        crate::generics::infer(pattern, &actual, inferable, types)?;
    }
    Ok(())
}

fn check_depth(types: &BTreeMap<String, Type>) -> Result<(), String> {
    if types.values().any(|ty| !ty.within_depth(32)) {
        Err("generic callback inference exceeds the type graph budget; supply type arguments explicitly".into())
    } else {
        Ok(())
    }
}

pub(super) fn reference_types(
    declaration: &Signature,
    expression: &Expr,
    hint: &Type,
    unresolved: &[String],
    env: &ScopeStack,
) -> Vec<Type> {
    if matches!(super::ungroup(expression), Expr::TypeApply { .. }) {
        return specialize(declaration, expression, None, &[], None, env, ResultInference::Join)
            .map(|value| vec![value.signature.ty()])
            .unwrap_or_default();
    }
    if declaration.generics.is_empty() {
        return vec![declaration.ty()];
    }
    super::reference_inference::options(declaration, hint, unresolved)
        .into_iter()
        .flat_map(|options| options.arguments)
        .map(|types| crate::generics::signature(declaration, &types).ty())
        .collect()
}
