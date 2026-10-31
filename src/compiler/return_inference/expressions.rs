//! Collect returns belonging to the current callable, respecting lazy operands.
//! Anonymous functions are separate callable boundaries, even inside arguments.

use super::super::{ScopeStack, infer, invalidate_narrowing, merge_env, narrow_condition};
use super::Inference;
use crate::model::Type;
use crate::syntax::Expr;
use std::sync::Arc;

pub(super) fn returned(value: Option<&Expr>, env: &mut ScopeStack, analysis: &mut Inference) -> Result<(), String> {
    let ty = value
        .map(|value| super::super::result_types::infer_value(value, env, None))
        .transpose()?
        .unwrap_or_else(|| Type::Unit.into());
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
        Expr::Binary(operator, left, right) => {
            let left_type = (operator == "?:").then(|| infer(left, env).ok()).flatten();
            if !collect(left, env, analysis)? {
                return Ok(false);
            }
            let known = super::super::constants::constant_boolean(left);
            let execute = match operator.as_str() {
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
        Expr::Call(callee, args) | Expr::SafeCall(callee, args) => {
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
                Ok(!ty.as_ref().is_some_and(|ty| matches!(
                    crate::generics::member_view(ty.present()), Type::Function { result, .. } if *result == Type::Nothing
                )))
            })
        }
        Expr::Index { receiver, index, safe } => {
            let ty = if *safe { infer(receiver, env).ok() } else { None };
            if !collect(receiver, env, analysis)? {
                return Ok(false);
            }
            lazy(
                if *safe { present(ty.as_ref()) } else { Some(true) },
                env,
                analysis,
                |env, analysis| collect(index, env, analysis),
            )
        }
        Expr::Unwrap(value) => {
            if !collect(value, env, analysis)? {
                return Ok(false);
            }
            super::super::flow::narrow_present(value, env);
            Ok(true)
        }
        Expr::Group(value)
        | Expr::Unary(_, value)
        | Expr::Spread(value)
        | Expr::BoundReference(value, _)
        | Expr::Member { receiver: value, .. }
        | Expr::TypeOp { value, .. }
        | Expr::TypeApply { callee: value, .. } => collect(value, env, analysis),
        Expr::Collection { entries, .. } => {
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
        Expr::Construct { fields, .. } => {
            for (_, value) in fields {
                if !collect(value, env, analysis)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Expr::Format(parts) => {
            for part in parts {
                if !collect(&part.value, env, analysis)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
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

fn present(ty: Option<&Type>) -> Option<bool> {
    match ty {
        Some(Type::Null) => Some(false),
        Some(ty) if !ty.permits_null() => Some(true),
        _ => None,
    }
}

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
