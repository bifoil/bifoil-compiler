//! Expression and function-result inference. This pass emits no instructions.

use super::overloads::{reference_needs_resolution, reference_type};
use super::{
    ScopeStack, access_member, binary_type, constant_case, enum_shorthand, index_types, infer_block_value,
    lambda_definition, lookup, member_type, narrow_condition, numeric, signature, specialize_signature, static_symbol,
};
use crate::model::Type;
use crate::syntax::Expr;

pub(super) fn collection_type(
    family: &Option<String>,
    mutable: bool,
    entries: &[(Option<Expr>, Expr)],
    env: &ScopeStack,
    expected: Option<&Type>,
) -> Result<Type, String> {
    let context = expected.and_then(|t| t.present().collection());
    let family = family
        .as_deref()
        .or(context.map(|context| context.0))
        .ok_or("empty braces require an expected Set or Map type")?;
    let mutable = mutable || context.is_some_and(|(_, _, m)| m);
    let count = if family == "Map" { 2 } else { 1 };
    if context.is_some_and(|(expected_family, _, _)| expected_family != family) {
        return Err("collection literal kind conflicts with expected type".into());
    }
    let mut args = Vec::with_capacity(count);
    for column in 0..count {
        if let Some(argument) = context.and_then(|(_, arguments, _)| arguments.get(column))
            && *argument != Type::Star
        {
            args.push(argument.clone());
            continue;
        }
        // A star hides the eventual argument; it is not an element type that a
        // literal can construct. Infer that concrete slot from its expressions.
        let mut joined = None;
        for (key, value) in entries {
            let expression = if family == "Map" && column == 0 {
                key.as_ref().ok_or("map entry needs a key")?
            } else {
                value
            };
            let ty = infer(expression, env)?;
            joined = Some(if let Some(previous) = joined {
                Type::join(&previous, &ty).ok_or("collection entries have no compatible joined type")?
            } else {
                ty
            });
        }
        let ty = joined.ok_or("empty collection requires a concrete expected type")?;
        if ty == Type::Null {
            return Err("all-null collection requires an expected type".into());
        }
        args.push(ty);
    }
    if args.len() != count {
        return Err("invalid collection type arguments".into());
    }
    if (family == "Set" || family == "Map") && !args[0].stable_key() {
        return Err("set elements and map keys require StableKey".into());
    }
    Ok(Type::named(
        &format!("{}{family}", if mutable { "Mutable" } else { "" }),
        std::mem::take(&mut args),
    ))
}

pub(super) fn infer(expr: &Expr, env: &ScopeStack) -> Result<Type, String> {
    infer_as(expr, env, None)
}

pub(super) fn infer_as(expr: &Expr, env: &ScopeStack, expected: Option<&Type>) -> Result<Type, String> {
    super::members::check_static_receiver(expr, env)?;
    if reference_needs_resolution(expr, env)
        && let Some(resolved) = specialize_signature(expr, None, expected, env)?
    {
        return Ok(reference_type(expr, resolved.signature.ty()));
    }
    if let Some(sym) = static_symbol(expr, env) {
        return Ok(if constant_case(&sym) {
            sym.signature().unwrap().result.clone()
        } else {
            sym.ty
        });
    }
    if let Expr::Name(n) = expr
        && let Some(s) = enum_shorthand(n, env, expected)?
    {
        return Ok(s.signature().unwrap().result.clone());
    }
    match expr {
        Expr::TypeApply { .. } => specialize_signature(expr, None, expected, env)?
            .map(|specialization| specialization.signature.ty())
            .ok_or("type arguments require a generic declaration".into()),
        Expr::Construct { ty, .. } => Ok(ty.clone()),
        Expr::Block(body) => infer_block_value(body, env, expected),
        Expr::Lambda(fun) => {
            let fun = lambda_definition(fun, env, expected)?;
            Ok(signature(&fun, "$lambda".into()).ty())
        }
        Expr::Int(n) => {
            if i32::try_from(*n).is_ok() {
                Ok(Type::Int)
            } else {
                Err("Int literal out of range".into())
            }
        }
        Expr::Float(_) => Ok(Type::Float),
        Expr::Boolean(_) => Ok(Type::Boolean),
        Expr::Char(_) => Ok(Type::Char),
        Expr::Null => Ok(Type::Null),
        Expr::String(_) | Expr::Format(_) => Ok(Type::String),
        Expr::Name(n) if n == "Unit" => Ok(Type::Unit),
        Expr::Name(n) | Expr::Reference(n) => lookup(env, n)
            .map(|s| s.value_type(expected))
            .ok_or_else(|| format!("unknown name '{n}'")),
        Expr::Group(e) => infer_as(e, env, expected),
        Expr::Unary(op, e) => {
            if op == "-" && matches!(e.as_ref(), Expr::Int(2147483648)) {
                return Ok(Type::Int);
            }
            let ty = crate::generics::operation_view(&infer(e, env)?);
            if ty == Type::Pending
                || ty == Type::Nothing
                || (op == "-" && numeric(&ty))
                || (op == "!" && ty == Type::Boolean)
            {
                Ok(ty)
            } else {
                Err(format!("invalid unary '{op}' for {ty}"))
            }
        }
        Expr::Binary(op, a, b) => {
            let left = infer(a, env)?;
            let mut rhs = env.clone();
            super::return_inference::normal_completion(a, &mut rhs)?;
            if op == "&&" || op == "||" {
                narrow_condition(a, &mut rhs, op == "&&");
            }
            let right = if op == "?:" {
                let fallback =
                    expected.or_else(|| (!matches!(left, Type::Null | Type::Nothing)).then(|| left.present()));
                infer_as(b, &rhs, fallback)?
            } else {
                infer(b, &rhs)?
            };
            binary_type(op, &left, &right)
        }
        Expr::Call(callee, args) | Expr::SafeCall(callee, args) => {
            let ty = if let Some(specialization) = specialize_signature(callee, Some(args), expected, env)? {
                specialization.signature.ty()
            } else {
                infer(callee, env)?
            };
            let safe = matches!(expr, Expr::SafeCall(..))
                || matches!(super::overloads::callable_base(callee), Expr::Member { safe: true, .. });
            if !safe && ty.permits_null() {
                return Err("nullable callable requires safe invocation or a null check".into());
            }
            match crate::generics::member_view(&ty) {
                Type::Function { result, .. } => Ok(if safe {
                    result.nullable()
                } else {
                    result.as_ref().clone()
                }),
                Type::Pending => Ok(Type::Pending),
                _ => Err("value is not callable".into()),
            }
        }
        Expr::Collection {
            family,
            mutable,
            entries,
        } => collection_type(family, *mutable, entries, env, expected),
        Expr::Member { receiver, name, safe } => {
            let ty = infer(receiver, env)?;
            if !safe && ty.permits_null() {
                return Err("nullable member receiver requires ?. or a null check".into());
            }
            access_member(ty.present(), name, env)?;
            let member = member_type(ty.present(), name)?;
            let declared = if *safe { member.nullable() } else { member };
            let refined = super::field_proofs::fact(expr, env);
            Ok(super::bindings::contextual_type(&declared, refined.as_ref(), expected))
        }
        Expr::BoundReference(receiver, name) => {
            let ty = infer(receiver, env)?;
            if ty.permits_null() {
                return Err("nullable bound receiver requires a null check".into());
            }
            access_member(&ty, name, env)?;
            member_type(&ty, name)
        }
        Expr::Index { receiver, safe, .. } => {
            let ty = infer(receiver, env)?;
            if !safe && ty.permits_null() {
                return Err("nullable index receiver requires ?[ or a null check".into());
            }
            let (_, item) = index_types(ty.present())?;
            let declared = if *safe { item.nullable() } else { item };
            let refined = super::field_proofs::fact(expr, env);
            Ok(super::bindings::contextual_type(&declared, refined.as_ref(), expected))
        }
        Expr::Unwrap(e) => {
            let source_expected = expected.map(Type::nullable);
            Ok(infer_as(e, env, source_expected.as_ref())?.non_null())
        }
        Expr::Spread(e) => infer(e, env),
        Expr::TypeOp { value, operator, ty } => {
            crate::declarations::validate_type(ty)?;
            let actual = infer(value, env)?;
            if operator == "is" || operator == "!is" {
                return Ok(Type::Boolean);
            }
            if !actual.may_overlap(ty) {
                return Err("cast between provably incompatible concrete types".into());
            }
            Ok(if operator == "as?" { ty.nullable() } else { ty.clone() })
        }
        Expr::Transfer { kind, .. } => Ok(if kind == "visit" { Type::Unit } else { Type::Nothing }),
    }
}

pub(super) fn pending_type(ty: &Type) -> bool {
    ty.any_node(|node| matches!(node, Type::Pending))
}
