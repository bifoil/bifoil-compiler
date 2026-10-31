//! Lexical capture discovery and definite-initialization requirements.

use super::{Binding, ScopeStack};
use crate::syntax::Expr;
use std::sync::Arc;

mod call_profile;
mod empty;
mod escapes;
pub(super) use escapes::{check_escaping, mark_escaped};
mod names;
mod publication;
mod publication_names;
pub(super) use empty::EmptyDependencies;
pub(super) use publication::{Publications, mark_published};
mod requirements;
pub(super) use names::{
    assigned_names, callable_captures, captured_names, expression_names, named_captures, statement_names,
};
pub(super) use requirements::{Requirement, Requirements};

/// Qualified static symbols use their full binding name. Ordinary member paths
/// capture their receiver cell, matching expression lowering's lookup order.
pub(super) fn capture_binding<'a>(env: &'a ScopeStack, name: &str) -> Option<(usize, &'a Binding)> {
    capture_slot(env, name).map(|(index, _, binding)| (index, binding))
}

fn capture_slot<'a>(env: &'a [super::bindings::Scope], name: &str) -> Option<(usize, &'a str, &'a Binding)> {
    let mut path = name;
    loop {
        if let Some(found) = env.iter().enumerate().rev().find_map(|(index, scope)| {
            scope
                .get_key_value(path)
                .map(|(key, binding)| (index, key.as_str(), binding))
        }) {
            return Some(found);
        }
        path = path.rsplit_once('.')?.0;
    }
}

/// Scope limits describe the callable's declaration environment, not the
/// deeper environment of a later caller. Missing future bindings stay missing.
pub(super) fn capture_required_slot<'a>(
    env: &'a ScopeStack,
    required: &Requirement,
) -> Option<(usize, &'a str, &'a Binding)> {
    let depth = required.scope_depth().unwrap_or(env.len());
    capture_slot(env.get(..depth)?, required.name())
}

fn initialized_reference(
    env: &ScopeStack,
    required: &Requirement,
    visited: &mut Requirements,
    owner: &str,
    empty: &mut EmptyDependencies,
) -> Result<(), String> {
    let mut pending = vec![required.clone()];
    let mut proof = empty::Proof::new();
    while let Some(required) = pending.pop() {
        let name = required.name();
        let (_, _, symbol) = capture_required_slot(env, &required)
            .ok_or_else(|| format!("cannot prove captured binding '{name}' initialized before this call/reference"))?;
        let key = EmptyDependencies::key(&required, symbol);
        if key.as_ref().is_some_and(|key| empty.contains(key)) {
            continue;
        }
        if !visited.insert(required.clone()) {
            proof.revisit(key.as_ref());
            continue;
        }
        if required.reads_binding() && symbol.owner == owner && !symbol.initialized {
            return Err(format!("captured binding '{name}' is not initialized"));
        }
        proof.include(key);
        pending.extend(required.dependencies(symbol).iter().cloned());
    }
    proof.finish(empty);
    Ok(())
}

pub(super) fn expression_requirements(e: &Expr, env: &ScopeStack) -> Requirements {
    match e {
        Expr::Name(n) | Expr::Reference(n) => super::bindings::lookup_ref(env, n)
            .map(|binding| {
                if let [signature] = binding.callables.as_slice()
                    && let Some(capture) = binding.callable_requirements.get(&signature.id)
                {
                    capture.body.clone()
                } else {
                    binding.requires.clone()
                }
            })
            .unwrap_or_default(),
        Expr::Group(e) => expression_requirements(e, env),
        Expr::Lambda(fun) => callable_captures(fun).body,
        _ => Default::default(),
    }
}

/// A direct call retains only the selected overload's requirements. The
/// returned set is captured before arguments can reassign its source binding.
pub(super) fn callable_metadata(
    expression: &Expr,
    signature: &crate::model::Signature,
    env: &ScopeStack,
) -> Option<Arc<super::bindings::CallableRequirements>> {
    let base = super::overloads::callable_base(expression);
    let (Expr::Name(name) | Expr::Reference(name)) = base else {
        return None;
    };
    super::bindings::lookup_ref(env, name)?
        .callable_requirements
        .get(&signature.id)
        .cloned()
}

/// Function values expose no defaults. Named calls add only omitted groups
/// after capturing this overload's immutable metadata before argument effects.
pub(super) fn callable_requirements(
    expression: &Expr,
    signature: &crate::model::Signature,
    env: &ScopeStack,
) -> Requirements {
    if let Some(capture) = callable_metadata(expression, signature, env) {
        return capture.body.clone();
    }
    let base = super::overloads::callable_base(expression);
    let (Expr::Name(name) | Expr::Reference(name)) = base else {
        return Default::default();
    };
    super::bindings::lookup_ref(env, name)
        .map(|binding| binding.requires.clone())
        .unwrap_or_default()
}

pub(super) fn check_requirements(
    names: &Requirements,
    env: &ScopeStack,
    owner: &str,
    empty: &mut EmptyDependencies,
) -> Result<(), String> {
    let mut visited = Default::default();
    for name in names {
        initialized_reference(env, name, &mut visited, owner, empty)?;
    }
    Ok(())
}

/// A yielded closure may retain an outer dependency for later initialization.
/// Retiring local reads require initialization; a write-only capture needs only
/// the existing cell. Expand local callables before removing their source names.
pub(super) fn outside_scope(names: &Requirements, env: &ScopeStack, depth: usize) -> Result<Requirements, String> {
    let mut result = Requirements::new();
    let mut visited = Requirements::new();
    let mut pending = names.iter().cloned().collect::<Vec<_>>();
    while let Some(required) = pending.pop() {
        if !visited.insert(required.clone()) {
            continue;
        }
        let name = required.name();
        if let Some((index, _, binding)) = capture_required_slot(env, &required) {
            if index >= depth {
                if required.reads_binding() && !binding.initialized {
                    return Err(format!(
                        "captured binding '{name}' is not initialized before leaving its scope"
                    ));
                }
                pending.extend(required.dependencies(binding).iter().cloned());
            } else {
                // The resolved outer binding survives; discard any no-longer
                // relevant inner portion of the declaration environment.
                result.insert(required.at_depth(index + 1));
            }
        } else {
            result.insert(required);
        }
    }
    Ok(result)
}

/// Retire lexical names from surviving bindings before a block or loop jump
/// removes their source scope. Captured cells remain alive at runtime.
pub(super) fn leaving_scopes(env: &ScopeStack, depth: usize) -> Result<ScopeStack, String> {
    let mut result = env[..depth].to_vec();
    for (index, scope) in env[..depth].iter().enumerate() {
        for (name, binding) in scope.iter().filter(|(_, binding)| !binding.requires.is_empty()) {
            let requires = outside_scope(&binding.requires, env, depth)?;
            if requires != binding.requires {
                std::sync::Arc::make_mut(&mut result[index])
                    .get_mut(name)
                    .unwrap()
                    .requires = requires;
            }
        }
    }
    Ok(result)
}

/// Resolve lexical paths once; global bindings remain available through the root.
pub(super) fn runtime_capture_names(names: &std::collections::BTreeSet<String>, env: &ScopeStack) -> Vec<String> {
    let mut captures = std::collections::BTreeSet::new();
    for name in names {
        let Some((_, binding)) = capture_binding(env, name) else {
            continue;
        };
        if binding.owner.is_empty() {
            continue;
        }
        if binding.callables.is_empty() {
            captures.insert(binding.runtime.clone());
        } else {
            // One lexical name can denote several independently stored local
            // closures. A compiled call may use any selected declaration ID.
            captures.extend(binding.callables.iter().map(|signature| signature.id.clone()));
        }
    }
    captures.into_iter().collect()
}

/// Named local closures are installed at block entry and may run before their
/// declaration. They can retain entry proofs, not checks performed later in the
/// block. Anonymous functions instead retain their creation-time immutable facts.
pub(super) fn named_environment(env: &ScopeStack, entry: &ScopeStack) -> ScopeStack {
    let mut result = env.clone();
    for (index, scope) in env.iter().enumerate() {
        for (name, binding) in scope
            .iter()
            .filter(|(_, binding)| binding.narrow.is_some() || binding.field_narrows.is_some())
        {
            let earlier = super::bindings::lookup_ref(entry, name).filter(|earlier| earlier.ty == binding.ty);
            let narrow = earlier.and_then(|earlier| earlier.narrow.clone());
            let field_narrows = earlier.and_then(|earlier| earlier.field_narrows.clone());
            if binding.narrow != narrow || binding.field_narrows != field_narrows {
                let target = std::sync::Arc::make_mut(&mut result[index]).get_mut(name).unwrap();
                target.narrow = narrow;
                target.field_narrows = field_narrows;
            }
        }
    }
    result
}
