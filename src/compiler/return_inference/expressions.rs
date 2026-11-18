//! Collect returns belonging to the current callable, respecting lazy operands.
//! Anonymous functions are separate callable boundaries, even inside arguments.

use super::super::{ScopeStack, infer, invalidate_narrowing, merge_env, narrow_condition};
use super::Inference;
use crate::model::Type;
use crate::syntax::Expr;
use std::sync::Arc;

pub(super) fn returned(value: Option<&Expr>, env: &mut ScopeStack, analysis: &mut Inference) -> Result<(), String> {
    if analysis.flow_only {
        if let Some(value) = value {
            collect(value, env, analysis)?;
        }
        return Ok(());
    }
    if analysis.alternatives.is_some() {
        let options = if let Some(value) = value {
            analysis.alternatives.as_mut().unwrap().expression(value, env)?
        } else {
            vec![Type::Unit.into()]
        };
        if value.is_none()
            || collect(value.unwrap(), env, analysis)?
            || options.iter().any(|value| value.ty == Type::Nothing)
        {
            analysis.choices.add(options)?;
        }
        return Ok(());
    }
    let discard = analysis.expected_return == Some(&Type::Unit);
    let hint = analysis.expected_return.filter(|_| !discard);
    let ty = value
        .map(|value| super::super::result_types::infer_value(value, env, hint))
        .transpose()?
        .unwrap_or_else(|| Type::Unit.into());
    if let Some(expected) = hint
        && ty.ty != Type::Pending
        && !expected.accepts(&ty.ty)
    {
        return Err(format!("return requires {expected}, found {}", ty.ty));
    }
    let ty = if discard && ty.ty != Type::Nothing {
        Type::Unit.into()
    } else {
        ty
    };
    if let Some(value) = value {
        if collect(value, env, analysis)? || ty.ty == Type::Nothing {
            // A bottom-valued return expression differs from having no value
            // return at all. Nothing remains neutral beside actual returned values.
            analysis.types.push(ty);
        }
    } else {
        analysis.types.push(ty);
    }
    Ok(())
}

pub(super) fn collect(expression: &Expr, env: &mut ScopeStack, analysis: &mut Inference) -> Result<bool, String> {
    match expression {
        Expr::Transfer { kind, label, value } => {
            if kind == "return" {
                returned(value.as_deref(), env, analysis)?;
            } else if kind == "break" || kind == "continue" {
                analysis.loop_transfer(kind, label, env)?;
            } else if let Some(value) = value {
                collect(value, env, analysis)?;
            }
            Ok(false)
        }
        Expr::Block(body) => {
            env.push(Arc::default());
            let reaches = super::block(body, env, analysis)?;
            env.pop();
            Ok(reaches)
        }
        Expr::Binary(operator, left, right, ..) => binary_returns(operator, left, right, env, analysis),
        Expr::Call(callee, args, ..) | Expr::SafeCall(callee, args, ..) => {
            call_returns(expression, callee, args, env, analysis)
        }
        Expr::Index {
            receiver, index, safe, ..
        } => index_returns(receiver, index, *safe, env, analysis),
        Expr::Unwrap(value, ..) => {
            if !collect(value, env, analysis)? {
                return Ok(false);
            }
            super::super::flow::narrow_present(value, env);
            Ok(true)
        }
        Expr::Group(value)
        | Expr::Unary(_, value, ..)
        | Expr::Spread(value)
        | Expr::BoundReference(value, ..)
        | Expr::Member { receiver: value, .. }
        | Expr::TypeOp { value, .. }
        | Expr::TypeApply { callee: value, .. } => collect(value, env, analysis),
        Expr::Collection { entries, .. } => collection_returns(entries, env, analysis),
        Expr::Construct { fields, .. } => ordered_returns(fields.iter().map(|(_, value)| value), env, analysis),
        Expr::Format(parts) => ordered_returns(parts.iter().map(|part| &part.value), env, analysis),
        Expr::Lambda(_)
        | Expr::Int(_)
        | Expr::Float(_)
        | Expr::Boolean(_)
        | Expr::String(_)
        | Expr::Char(_)
        | Expr::Null
        | Expr::Name(_)
        | Expr::Reference(_) => Ok(true),
    }
}

fn binary_returns(
    operator: &str,
    left: &Expr,
    right: &Expr,
    env: &mut ScopeStack,
    analysis: &mut Inference,
) -> Result<bool, String> {
    let left_type = (operator == "?:").then(|| infer(left, env).ok()).flatten();
    if !collect(left, env, analysis)? {
        return Ok(false);
    }
    let known = super::super::constants::constant_boolean(left);
    let execute = match operator {
        "&&" => known,
        "||" => known.map(|value| !value),
        "?:" => match left_type {
            Some(Type::Null) => Some(true),
            Some(ty) if !ty.permits_null() => Some(false),
            _ => None,
        },
        _ => return collect(right, env, analysis),
    };
    lazy(execute, env, analysis, |env, analysis| {
        if operator == "&&" || operator == "||" {
            narrow_condition(left, env, operator == "&&");
        }
        collect(right, env, analysis)
    })
}

fn call_returns(
    expression: &Expr,
    callee: &Expr,
    args: &[(Option<String>, Expr)],
    env: &mut ScopeStack,
    analysis: &mut Inference,
) -> Result<bool, String> {
    let resolved = super::super::specialize_signature(callee, Some(args), None, env)
        .ok()
        .flatten();
    let builtin = resolved
        .as_ref()
        .is_some_and(|call| call.signature.id.starts_with("$builtin."));
    let heap_effect = resolved
        .as_ref()
        .map(|call| super::super::members::builtin_heap_effect(&call.signature.id))
        .unwrap_or(super::super::members::BuiltinHeapEffect::Unknown);
    let ty = resolved
        .map(|call| call.signature.ty())
        .or_else(|| infer(callee, env).ok());
    if !collect(callee, env, analysis)? {
        return Ok(false);
    }
    let safe = matches!(expression, Expr::SafeCall(..))
        || matches!(
            super::super::overloads::callable_base(callee),
            Expr::Member { safe: true, .. }
        );
    let execute = if safe { present(ty.as_ref()) } else { Some(true) };
    lazy(execute, env, analysis, |env, analysis| {
        for (_, argument) in args {
            if !collect(argument, env, analysis)? {
                return Ok(false);
            }
        }
        if !builtin {
            invalidate_narrowing(env);
        } else {
            super::super::field_proofs::invalidate_builtin(heap_effect, env);
        }
        Ok(!ty.as_ref().is_some_and(|ty| {
            matches!(
                crate::generics::member_view(ty.present()), Type::Function { result, .. } if *result == Type::Nothing
            )
        }))
    })
}

fn index_returns(
    receiver: &Expr,
    index: &Expr,
    safe: bool,
    env: &mut ScopeStack,
    analysis: &mut Inference,
) -> Result<bool, String> {
    let ty = if safe { infer(receiver, env).ok() } else { None };
    if !collect(receiver, env, analysis)? {
        return Ok(false);
    }
    lazy(
        if safe { present(ty.as_ref()) } else { Some(true) },
        env,
        analysis,
        |env, analysis| collect(index, env, analysis),
    )
}

fn collection_returns(
    entries: &[(Option<Expr>, Expr)],
    env: &mut ScopeStack,
    analysis: &mut Inference,
) -> Result<bool, String> {
    for (key, value) in entries {
        if let Some(key) = key
            && !collect(key, env, analysis)?
        {
            return Ok(false);
        }
        if !collect(value, env, analysis)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn ordered_returns<'a>(
    values: impl IntoIterator<Item = &'a Expr>,
    env: &mut ScopeStack,
    analysis: &mut Inference,
) -> Result<bool, String> {
    for value in values {
        if !collect(value, env, analysis)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn present(ty: Option<&Type>) -> Option<bool> {
    match ty {
        Some(Type::Null) => Some(false),
        Some(ty) if !ty.permits_null() => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

fn lazy(
    execute: Option<bool>,
    env: &mut ScopeStack,
    analysis: &mut Inference,
    operation: impl FnOnce(&mut ScopeStack, &mut Inference) -> Result<bool, String>,
) -> Result<bool, String> {
    match execute {
        Some(true) => operation(env, analysis),
        Some(false) => Ok(true),
        None => {
            let base = env.clone();
            let mut branch = base.clone();
            let reaches = operation(&mut branch, analysis)?;
            let mut exits = vec![base.clone()];
            if reaches {
                exits.push(branch);
            }
            *env = merge_env(&base, &exits);
            Ok(true)
        }
    }
}
