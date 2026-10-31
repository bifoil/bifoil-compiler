//! Static operand rules for arithmetic, comparison, and membership.

use crate::model::Type;

pub(super) fn numeric(ty: &Type) -> bool {
    matches!(ty, Type::Int | Type::Float)
}

pub(super) fn binary_type(op: &str, a: &Type, b: &Type) -> Result<Type, String> {
    // Equality keeps capability-bearing parameters and Elvis keeps their identity.
    // Primitive operations can use a proven, non-null upper value type.
    let primitive = !["==", "!=", "===", "!==", "in", "!in", "?:"].contains(&op);
    let left = (primitive && matches!(a, Type::Parameter { .. })).then(|| crate::generics::operation_view(a));
    let right = (primitive && matches!(b, Type::Parameter { .. })).then(|| crate::generics::operation_view(b));
    let a = left.as_ref().unwrap_or(a);
    let b = right.as_ref().unwrap_or(b);
    if *a == Type::Nothing {
        return Ok(Type::Nothing);
    }
    if ["&&", "||"].contains(&op) && !matches!(a, Type::Boolean | Type::Pending) {
        return Err(format!("operator '{op}' cannot combine {a} and {b}"));
    }
    if *b == Type::Nothing {
        if ["&&", "||"].contains(&op) && *a == Type::Boolean {
            return Ok(Type::Boolean);
        }
        if op != "?:" {
            return Ok(Type::Nothing);
        }
    }
    if *a == Type::Pending || *b == Type::Pending {
        return Ok(Type::Pending);
    }
    if op == "?:" {
        if *a == Type::Null {
            return Ok(b.clone());
        }
        return a
            .non_null()
            .join(b)
            .ok_or_else(|| "fallback types have no compatible join".into());
    }
    if ["..", "..<", "downTo"].contains(&op) && *a == Type::Int && *b == Type::Int {
        return Ok(Type::named(
            if op == "downTo" { "IntProgression" } else { "IntRange" },
            vec![],
        ));
    }
    if op == "step" && matches!(a,Type::Named{name,..} if name=="IntRange"||name=="IntProgression") && *b == Type::Int {
        return Ok(Type::named("IntProgression", vec![]));
    }
    if (op == "in" || op == "!in") && !b.permits_null() {
        let view = crate::generics::member_view(b);
        let b = &view;
        let item = if let Some((_, args, _)) = b.collection() {
            Some(args[0].clone())
        } else {
            b.item_type()
        };
        if item.is_some_and(|t| t.join(a).is_some() && t.equatable() && a.equatable()) {
            return Ok(Type::Boolean);
        }
    }
    if op == "&&" || op == "||" {
        if *a == Type::Boolean && *b == Type::Boolean {
            return Ok(Type::Boolean);
        }
    } else if op == "+" && (*a == Type::String || *b == Type::String) && a.renderable() && b.renderable() {
        return Ok(Type::String);
    } else if ["==", "!=", "===", "!=="].contains(&op) {
        if *a == Type::Null || *b == Type::Null {
            return Ok(Type::Boolean);
        }
        let av = crate::generics::member_view(a);
        let bv = crate::generics::member_view(b);
        let primitive_join = av.renderable() && bv.renderable() && av.join(&bv).is_some();
        // A value bound proves a shared comparison family even when the generic
        // parameter itself can include null. Capability-only Any views cannot
        // establish compatibility with an unrelated operand.
        let bound_join = (matches!(a.present(), Type::Parameter { .. })
            || matches!(b.present(), Type::Parameter { .. }))
            && av != Type::Any
            && bv != Type::Any
            && av.join(&bv).is_some();
        let compatible = a.join(b).is_some()
            || primitive_join
            || bound_join
            || match (a.collection(), b.collection()) {
                (Some((a, aa, _)), Some((b, ba, _))) => a == b && aa.iter().zip(ba).all(|(a, b)| a.join(b).is_some()),
                _ => false,
            };
        if a.equatable() && b.equatable() && (compatible || op == "===" || op == "!==") {
            return Ok(Type::Boolean);
        }
    } else if ["<", "<=", ">", ">="].contains(&op) {
        if (numeric(a) && numeric(b))
            || ([Type::String, Type::Char].contains(a) && [Type::String, Type::Char].contains(b))
        {
            return Ok(Type::Boolean);
        }
    } else if numeric(a) && numeric(b) {
        return match op {
            "%+" => Ok(Type::Int),
            "%-" => Ok(Type::Int),
            "%" if *a == Type::Int && *b == Type::Int => Ok(Type::Int),
            "+" | "-" | "*" | "/" => Ok(if *a == Type::Float || *b == Type::Float {
                Type::Float
            } else {
                Type::Int
            }),
            _ => Err(format!("operator '{op}' is not defined for these operands")),
        };
    }
    Err(format!("operator '{op}' cannot combine {a} and {b}"))
}
