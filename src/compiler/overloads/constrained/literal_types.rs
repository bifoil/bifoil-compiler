//! Infer literal shapes without choosing an overloaded element prematurely.
//!
//! The normal collection rules still govern joins, default mutability, empty
//! and all-null slots, and StableKey. These are candidate types, not proof that
//! a source expression can be constructed in the eventual parameter context.

use super::super::super::{ScopeStack, infer};
use super::super::ungroup;
use crate::generics::call_constraints::Failure;
use crate::model::Type;
use crate::syntax::Expr;

const MAX_LITERAL_ALTERNATIVES: usize = 256;

pub(super) fn options(
    expression: &Expr,
    env: &ScopeStack,
    unresolved: &[String],
    hint: Option<&Type>,
    depth: usize,
) -> Result<Vec<Type>, String> {
    if depth > 32 {
        return Err(Failure::Budget.to_string());
    }
    if hint.is_none()
        && !super::expression_options::requires_context(expression, env)
        && let Ok(ty) = infer(expression, env)
    {
        return Ok(vec![ty]);
    }
    let expression = ungroup(expression);
    if let Expr::Collection {
        family: Some(family),
        mutable,
        entries,
    } = expression
    {
        let context = hint
            .and_then(|ty| ty.present().collection())
            .filter(|(kind, _, _)| *kind == family);
        let arguments = context.map(|(_, arguments, _)| arguments);
        let concrete = |index: usize| {
            arguments
                .and_then(|arguments| arguments.get(index))
                .filter(|ty| super::expression_options::complete(ty, unresolved))
                .cloned()
        };
        let mut prefixes = vec![Slots {
            key: (family == "Map").then(|| concrete(0)).flatten(),
            value: concrete(if family == "Map" { 1 } else { 0 }),
        }];
        let mut scope = env.clone();
        for (key, value) in entries {
            if family == "Map" {
                let Some(key) = key else {
                    return Ok(vec![]);
                };
                let keys = super::expression_options::types(
                    key,
                    &scope,
                    unresolved,
                    arguments.and_then(|types| types.first()),
                    depth + 1,
                )?;
                prefixes = extend(prefixes, &keys, true)?;
                super::super::super::return_inference::normal_completion(key, &mut scope)?;
            }
            let values = super::expression_options::types(
                value,
                &scope,
                unresolved,
                arguments.and_then(|types| types.last()),
                depth + 1,
            )?;
            prefixes = extend(prefixes, &values, false)?;
            super::super::super::return_inference::normal_completion(value, &mut scope)?;
        }
        let name = format!(
            "{}{family}",
            if *mutable || context.is_some_and(|(_, _, mutable)| mutable) {
                "Mutable"
            } else {
                ""
            }
        );
        let mut types = vec![];
        for slots in prefixes {
            let Some(value) = slots.value.filter(|ty| *ty != Type::Null) else {
                continue;
            };
            let arguments = if family == "Map" {
                let Some(key) = slots.key.filter(|ty| *ty != Type::Null && ty.stable_key()) else {
                    continue;
                };
                vec![key, value]
            } else {
                if family == "Set" && !value.stable_key() {
                    continue;
                }
                vec![value]
            };
            let ty = Type::named(&name, arguments);
            if !ty.within_depth(32) {
                return Err(Failure::Budget.to_string());
            }
            if !types.contains(&ty) {
                types.push(ty);
            }
        }
        return Ok(types);
    }
    Ok(vec![])
}

#[derive(Default, Clone, PartialEq)]
struct Slots {
    key: Option<Type>,
    value: Option<Type>,
}

fn extend(prefixes: Vec<Slots>, options: &[Type], key: bool) -> Result<Vec<Slots>, String> {
    let mut extended = vec![];
    for prefix in prefixes {
        for option in options {
            let mut next = prefix.clone();
            let slot = if key { &mut next.key } else { &mut next.value };
            let joined = if let Some(previous) = slot {
                previous.join(option)
            } else {
                Some(option.clone())
            };
            let Some(joined) = joined else {
                continue;
            };
            *slot = Some(joined);
            if !extended.contains(&next) {
                if extended.len() == MAX_LITERAL_ALTERNATIVES {
                    return Err(Failure::Budget.to_string());
                }
                extended.push(next);
            }
        }
    }
    Ok(extended)
}

#[cfg(test)]
mod tests;
