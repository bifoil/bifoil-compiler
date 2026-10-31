//! Overload applicability and selection, independent of instruction emission.
//!
//! Conversion quality is a partial order: a candidate must be no worse on every
//! supplied argument and better on at least one. Arity/default/generic tie
//! breakers apply only to candidates with identical conversion ranks.

pub(super) mod arguments;
mod ranking;
mod specialization;
pub(super) use ranking::{OverloadRank, best_match};
use specialization::specialize;

use super::{ScopeStack, infer, infer_as, lookup, static_symbol};
use crate::model::{Signature, Type};
use crate::syntax::Expr;
use std::collections::BTreeMap;

pub(super) struct CallableSpecialization {
    pub(super) signature: Signature,
    pub(super) type_arguments: BTreeMap<String, Type>,
}

struct Candidate {
    callable: CallableSpecialization,
    rank: OverloadRank,
}

pub(super) fn callable_candidates(expression: &Expr, env: &ScopeStack) -> Vec<Signature> {
    if let Expr::TypeApply { callee, .. } | Expr::Group(callee) = expression {
        return callable_candidates(callee, env);
    }
    if let Some(binding) = static_symbol(expression, env) {
        return binding.callables;
    }
    match expression {
        Expr::Name(name) | Expr::Reference(name) => lookup(env, name).map(|s| s.callables).unwrap_or_default(),
        Expr::Member { receiver, name, .. } | Expr::BoundReference(receiver, name) => {
            let Ok(receiver) = infer(receiver, env) else {
                return vec![];
            };
            // A callable field is a function value, without a declared method's
            // parameter names/defaults, and takes precedence over built-ins.
            if crate::declarations::field(receiver.present(), name).is_some() {
                return vec![];
            }
            let methods = crate::declarations::methods(receiver.present(), name);
            if methods.is_empty() {
                super::members::builtin_signature(receiver.present(), name)
                    .into_iter()
                    .collect()
            } else {
                methods
            }
        }
        _ => vec![],
    }
}

/// Match argument names, positions and spread groups without evaluating source.
fn argument_positions(signature: &Signature, arguments: &[(Option<String>, Expr)]) -> Result<Vec<usize>, String> {
    arguments::positions(
        signature,
        arguments.iter().map(|(name, value)| arguments::Form {
            name: name.as_deref(),
            spread: matches!(value, Expr::Spread(_)),
        }),
    )
}

fn applicable(
    declaration: &Signature,
    expression: &Expr,
    arguments: &[(Option<String>, Expr)],
    expected: Option<&Type>,
    env: &ScopeStack,
) -> Result<Candidate, String> {
    let positions = argument_positions(declaration, arguments)?;
    let mut scope = env.clone();
    super::return_inference::normal_completion(expression, &mut scope)?;
    let callable = specialize(declaration, expression, Some(arguments), &positions, expected, &scope)?;
    let mut ranks = vec![];
    for ((name, value), index) in arguments.iter().zip(&positions) {
        let parameter = &callable.signature.params[*index];
        let grouped = parameter.vararg && (name.is_some() || matches!(value, Expr::Spread(_)));
        let target = if grouped {
            Type::named("List", vec![parameter.ty.clone()])
        } else {
            parameter.ty.clone()
        };
        let value = if let Expr::Spread(inner) = value {
            inner.as_ref()
        } else {
            value
        };
        let contextual = infer_as(value, &scope, Some(&target))?;
        if !target.accepts(&contextual) {
            return Err(format!("argument requires {target}, found {contextual}"));
        }
        let inferred = infer(value, &scope);
        let actual = inferred.as_ref().unwrap_or(&contextual);
        let rank = target
            .conversion_from(actual)
            // A narrowed binding also retains its original declared identity.
            // Contextual typing can expose that identity without adapting a value.
            .or_else(|| target.conversion_from(&contextual))
            .or_else(|| {
                // A lambda is constructed under its expected signature. In a Unit
                // context its final expression is deliberately discarded.
                matches!(ungroup(value), Expr::Lambda(_)).then_some(0)
            })
            .ok_or("argument types do not match")?;
        ranks.push(rank);
        super::return_inference::normal_completion(value, &mut scope)?;
    }
    let omitted_defaults = arguments::omitted_defaults(declaration, &positions).len();
    Ok(Candidate {
        callable,
        rank: OverloadRank {
            conversions: ranks,
            omitted_defaults,
            variadic: declaration.params.last().is_some_and(|p| p.vararg),
            generic: !declaration.generics.is_empty(),
        },
    })
}

pub(super) fn specialize_signature(
    expression: &Expr,
    arguments: Option<&[(Option<String>, Expr)]>,
    expected: Option<&Type>,
    env: &ScopeStack,
) -> Result<Option<CallableSpecialization>, String> {
    super::members::check_static_receiver(expression, env)?;
    // Taking an explicit reference fixes a function/scene value. Its subsequent
    // call exposes types and variadic shape, without names or default metadata.
    if arguments.is_some() && matches!(callable_base(expression), Expr::Reference(_) | Expr::BoundReference(..)) {
        return Ok(None);
    }
    let mut declarations = callable_candidates(expression, env);
    if declarations.is_empty() {
        return Ok(None);
    }
    if let Expr::Member { receiver, .. } | Expr::BoundReference(receiver, _) = callable_base(expression)
        && static_symbol(callable_base(expression), env).is_none()
    {
        let receiver = infer(receiver, env)?;
        let current_receiver = super::bindings::current_receiver(env);
        declarations.retain(|signature| {
            crate::declarations::check_method_access(receiver.present(), &signature.id, current_receiver).is_ok()
        });
        if declarations.is_empty() {
            return Err("method is private".into());
        }
    }
    if let Some(arguments) = arguments {
        let mut candidates = vec![];
        let mut failures = vec![];
        for signature in &declarations {
            match applicable(signature, expression, arguments, expected, env) {
                Ok(candidate) => candidates.push(candidate),
                Err(error) => failures.push(error),
            }
        }
        if candidates.is_empty() {
            return Err(if declarations.len() == 1 {
                failures.remove(0)
            } else {
                format!("no applicable overload: {}", failures.join("; "))
            });
        }
        let selected = best_match(candidates.iter().map(|candidate| &candidate.rank))
            .ok_or("ambiguous overload; qualify the argument types or take an explicitly typed reference")?;
        return Ok(Some(candidates.swap_remove(selected).callable));
    }

    let mut matches = vec![];
    let mut failure = None;
    for declaration in &declarations {
        match specialize(declaration, expression, None, &[], expected, env) {
            Ok(callable) if expected.is_none_or(|ty| callable.signature.ty().subtype_of(ty.present())) => {
                matches.push(callable)
            }
            Ok(_) => {}
            Err(error) => failure = Some(error),
        }
    }
    if matches.len() == 1 {
        return Ok(matches.pop());
    }
    if matches.is_empty() {
        return Err(failure.unwrap_or_else(|| "no overload matches the expected function type".into()));
    }
    Err("ambiguous overload reference; supply a unique compatible function type".into())
}

pub(super) fn ungroup(mut expression: &Expr) -> &Expr {
    while let Expr::Group(inner) = expression {
        expression = inner;
    }
    expression
}

pub(super) fn callable_base(mut expression: &Expr) -> &Expr {
    while let Expr::Group(inner) | Expr::TypeApply { callee: inner, .. } = expression {
        expression = inner;
    }
    expression
}

pub(super) fn reference_needs_resolution(expression: &Expr, env: &ScopeStack) -> bool {
    let candidates = callable_candidates(expression, env);
    if candidates.is_empty() {
        return false;
    }
    let expression = ungroup(expression);
    // Payload-free enum/variant expressions construct their value directly.
    if !matches!(
        expression,
        Expr::Reference(_) | Expr::BoundReference(..) | Expr::TypeApply { .. }
    ) && static_symbol(expression, env).is_some_and(|s| super::constant_case(&s))
    {
        return false;
    }
    true
}

pub(super) fn reference_type(expression: &Expr, ty: Type) -> Type {
    if matches!(callable_base(expression), Expr::Member { safe: true, .. }) {
        ty.nullable()
    } else {
        ty
    }
}
