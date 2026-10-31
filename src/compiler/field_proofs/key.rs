//! Constant map keys preserve nominal enum identity and ordinary scalar conversions.
//! Their ordered representation identifies proof slots, not language sort order.

use super::{ConstantKey, ScopeStack};
use crate::model::Type;
use crate::syntax::Expr;

impl ConstantKey {
    fn float(number: f64) -> Self {
        // Both zero signs denote the same map key under ordinary value equality.
        Self::Float(if number == 0.0 { 0 } else { number.to_bits() })
    }
    pub(super) fn source(expression: &Expr, env: &ScopeStack, expected: Option<&Type>) -> Option<Self> {
        match expression {
            Expr::Group(value) => Self::source(value, env, expected),
            Expr::String(value) => Some(Self::String(value.clone())),
            Expr::Char(value) => Some(Self::Char(value.clone())),
            Expr::Boolean(value) => Some(Self::Boolean(*value)),
            Expr::Null => Some(Self::Null),
            Expr::Name(name) if name == "Unit" => Some(Self::Unit),
            _ => {
                if let Some((ty, number)) = super::super::constants::constant_number(expression).ok()? {
                    match ty {
                        Type::Int if (i32::MIN as f64..=i32::MAX as f64).contains(&number) => {
                            Some(Self::Int(number as i32))
                        }
                        Type::Float if number.is_finite() => Some(Self::float(number)),
                        _ => None,
                    }
                } else {
                    super::super::constants::constant_boolean(expression)
                        .map(Self::Boolean)
                        .or_else(|| Self::enum_case(expression, env, expected))
                }
            }
        }
    }
    fn enum_case(expression: &Expr, env: &ScopeStack, expected: Option<&Type>) -> Option<Self> {
        use super::super::members;
        let symbol = match expression {
            Expr::Name(name) => members::enum_shorthand(name, env, expected)
                .ok()?
                .or_else(|| super::lookup_ref(env, name).cloned()),
            Expr::Member { .. } => members::static_symbol(expression, env),
            _ => None,
        }?;
        if !members::constant_case(&symbol) {
            return None;
        }
        let signature = symbol.signature()?;
        let Type::Named { name, args } = &signature.result else {
            return None;
        };
        let definition = crate::declarations::definition(name)?;
        // Payload-free sum variants can still carry generic identities. Keep
        // those separate until their key representation includes the arguments.
        if definition.kind != "enum" || !definition.parameters.is_empty() || !args.is_empty() {
            return None;
        }
        Some(Self::Enum {
            declaration: name.clone(),
            case: signature.id.clone(),
        })
    }
    pub(super) fn converted(&self, target: &Type) -> Option<Self> {
        match (self, target.present()) {
            (Self::Null, _) if target.permits_null() => Some(Self::Null),
            (Self::Int(number), Type::Float) => Some(Self::float(*number as f64)),
            (Self::Char(value), Type::String) => Some(Self::String(value.clone())),
            (Self::Enum { declaration, .. }, Type::Named { name, args }) if declaration == name && args.is_empty() => {
                Some(self.clone())
            }
            (Self::Boolean(_), Type::Boolean)
            | (Self::Int(_), Type::Int)
            | (Self::Float(_), Type::Float)
            | (Self::Char(_), Type::Char)
            | (Self::String(_), Type::String)
            | (Self::Unit, Type::Unit) => Some(self.clone()),
            _ => None,
        }
    }
}
