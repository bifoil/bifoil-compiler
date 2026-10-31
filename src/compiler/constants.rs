//! Provable constant failures and literal-key checks; evaluation order is preserved.

use crate::model::Type;
use crate::syntax::Expr;

/// Facts about scalar constants require no reads or user-code evaluation.
/// They affect proof/reachability only; lowering still evaluates the expression.
pub(super) fn constant_boolean(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::Boolean(value) => Some(*value),
        Expr::Group(inner) => constant_boolean(inner),
        Expr::Unary(operator, inner) if operator == "!" => constant_boolean(inner).map(|value| !value),
        Expr::Binary(operator, left, right) if operator == "&&" => {
            if constant_boolean(left)? {
                constant_boolean(right)
            } else {
                Some(false)
            }
        }
        Expr::Binary(operator, left, right) if operator == "||" => {
            if constant_boolean(left)? {
                Some(true)
            } else {
                constant_boolean(right)
            }
        }
        Expr::Binary(operator, left, right)
            if ["==", "!=", "===", "!==", "<", "<=", ">", ">="].contains(&operator.as_str()) =>
        {
            constant_comparison(operator, left, right)
        }
        _ => None,
    }
}

enum ScalarConstant<'a> {
    Number(Type, f64),
    Text(Type, &'a str),
    Boolean(bool),
    Null,
    Unit,
}
impl ScalarConstant<'_> {
    fn ty(&self) -> Type {
        match self {
            Self::Number(ty, _) | Self::Text(ty, _) => ty.clone(),
            Self::Boolean(_) => Type::Boolean,
            Self::Null => Type::Null,
            Self::Unit => Type::Unit,
        }
    }
}
fn scalar_constant(expression: &Expr) -> Option<ScalarConstant<'_>> {
    match expression {
        Expr::Group(inner) => scalar_constant(inner),
        Expr::String(value) => Some(ScalarConstant::Text(Type::String, value)),
        Expr::Char(value) => Some(ScalarConstant::Text(Type::Char, value)),
        Expr::Null => Some(ScalarConstant::Null),
        Expr::Name(name) if name == "Unit" => Some(ScalarConstant::Unit),
        _ => {
            if let Some((ty, value)) = constant_number(expression).ok().flatten() {
                Some(ScalarConstant::Number(ty, value))
            } else {
                constant_boolean(expression).map(ScalarConstant::Boolean)
            }
        }
    }
}

pub(super) fn constant_comparison(operator: &str, left: &Expr, right: &Expr) -> Option<bool> {
    use std::cmp::Ordering;
    let left = scalar_constant(left)?;
    let right = scalar_constant(right)?;
    let exact = operator == "===" || operator == "!==";
    let order = match (&left, &right) {
        (ScalarConstant::Number(_, a), ScalarConstant::Number(_, b)) => a.partial_cmp(b),
        (ScalarConstant::Text(_, a), ScalarConstant::Text(_, b)) => Some(a.cmp(b)),
        (ScalarConstant::Boolean(a), ScalarConstant::Boolean(b)) => Some(a.cmp(b)),
        (ScalarConstant::Null, ScalarConstant::Null) | (ScalarConstant::Unit, ScalarConstant::Unit) => {
            Some(Ordering::Equal)
        }
        _ => None,
    };
    let equal = order == Some(Ordering::Equal) && (!exact || left.ty() == right.ty());
    match operator {
        "==" | "===" => Some(equal),
        "!=" | "!==" => Some(!equal),
        "<" => order.map(|value| value.is_lt()),
        "<=" => order.map(|value| !value.is_gt()),
        ">" => order.map(|value| value.is_gt()),
        ">=" => order.map(|value| !value.is_lt()),
        _ => None,
    }
}

pub(super) fn constant_number(expr: &Expr) -> Result<Option<(Type, f64)>, String> {
    let result = match expr {
        Expr::Int(n) => Some((Type::Int, *n as f64)),
        Expr::Float(n) => Some((Type::Float, *n)),
        Expr::Group(e) => return constant_number(e),
        Expr::Unary(op, e) if op == "-" => constant_number(e)?.map(|(ty, n)| (ty, -n)),
        Expr::Binary(op, a, b) => {
            let left = constant_number(a)?;
            if matches!(op.as_str(), "%+" | "%-")
                && left.as_ref().is_some_and(|(_, base)| !(0.0..=100.0).contains(base))
            {
                return Err("constant fairmath base must be in 0..100".into());
            }
            let (Some((ta, a)), Some((tb, b))) = (left, constant_number(b)?) else {
                return Ok(None);
            };
            let ty = if ta == Type::Int && tb == Type::Int {
                Type::Int
            } else {
                Type::Float
            };
            let n = match op.as_str() {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" => {
                    if b == 0.0 {
                        return Err("constant division by zero".into());
                    }
                    if ty == Type::Int { (a / b).trunc() } else { a / b }
                }
                "%" => {
                    if b == 0.0 {
                        return Err("constant division by zero".into());
                    }
                    a % b
                }
                "%+" | "%-" => {
                    if !(0.0..=100.0).contains(&a) {
                        return Err("constant fairmath base must be in 0..100".into());
                    }
                    let change = if op == "%-" { -b } else { b };
                    let result = if change > 0.0 {
                        (a + change * ((100.0 - a) / 100.0)).floor().min(99.0)
                    } else {
                        (a - (0.0 - change) * (a / 100.0)).ceil().max(1.0)
                    };
                    return Ok(Some((Type::Int, result)));
                }
                _ => return Ok(None),
            };
            Some((ty, n))
        }
        _ => None,
    };
    if let Some((ty, n)) = &result {
        if !n.is_finite() {
            return Err("constant produces non-finite Float".into());
        }
        if *ty == Type::Int && (*n < i32::MIN as f64 || *n > i32::MAX as f64) {
            // Unary minus may consume the special positive spelling of Int.MIN.
            if !matches!(expr, Expr::Int(2147483648)) {
                return Err("constant Int overflow".into());
            }
        }
    }
    Ok(result)
}

fn ungroup(mut expression: &Expr) -> &Expr {
    while let Expr::Group(inner) = expression {
        expression = inner;
    }
    expression
}
fn literal_length(receiver: &Expr) -> Option<usize> {
    match ungroup(receiver) {
        Expr::String(text) => Some(text.chars().count()),
        Expr::Collection {
            family: Some(family),
            entries,
            ..
        } if family == "List" => Some(entries.len()),
        _ => None,
    }
}

pub(super) fn check_index(receiver: &Expr, index: &Expr, receiver_type: &Type) -> Result<(), String> {
    if *receiver_type != Type::String
        && !receiver_type
            .collection()
            .is_some_and(|(family, _, _)| family == "List")
    {
        return Ok(());
    }
    if let Some((Type::Int, value)) = constant_number(index)?
        && (value < 0.0 || literal_length(receiver).is_some_and(|length| value >= length as f64))
    {
        return Err("constant index is out of bounds".into());
    }
    Ok(())
}

/// Check literal bounds without evaluating source or changing argument order.
/// Named arguments are mapped through the already selected built-in signature.
pub(super) fn check_call_bounds(
    signature: &crate::model::Signature,
    callee: &Expr,
    arguments: &[(Option<String>, Expr)],
) -> Result<(), String> {
    if !["$builtin.substring", "$builtin.removeAt", "$builtin.insert"].contains(&signature.id.as_str()) {
        return Ok(());
    }
    let required = if signature.id == "$builtin.substring" { 2 } else { 1 };
    if signature.params.len() < required {
        return Err("invalid built-in bounds interface".into());
    }
    let receiver = match super::overloads::callable_base(callee) {
        Expr::Member { receiver, .. } | Expr::BoundReference(receiver, _) => receiver,
        _ => return Ok(()),
    };
    let mut constants = vec![None; signature.params.len()];
    for (position, (name, value)) in arguments.iter().enumerate() {
        let index = name
            .as_ref()
            .and_then(|name| signature.params.iter().position(|param| &param.name == name))
            .unwrap_or(position);
        if let Some(slot) = constants.get_mut(index)
            && let Some((Type::Int, value)) = constant_number(value)?
        {
            *slot = Some(value);
        }
    }
    let length = literal_length(receiver).map(|length| length as f64);
    if signature.id == "$builtin.substring" {
        if constants
            .iter()
            .flatten()
            .any(|value| *value < 0.0 || length.is_some_and(|length| *value > length))
            || matches!((constants[0], constants[1]), (Some(start), Some(end)) if start > end)
        {
            return Err("constant substring bounds are invalid".into());
        }
    } else if constants[0].is_some_and(|index| {
        index < 0.0
            || length.is_some_and(|length| {
                if signature.id == "$builtin.insert" {
                    index > length
                } else {
                    index >= length
                }
            })
    }) {
        return Err("constant list method index is out of bounds".into());
    }
    Ok(())
}
