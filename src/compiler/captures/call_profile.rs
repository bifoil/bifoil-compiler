//! Compact argument facts used when expanding deferred named-call reads.
//! Primitive type facts contain no argument values or retained expression trees.

use crate::compiler::overloads::{OverloadRank, arguments, best_match};
use crate::model::{Signature, Type};
use crate::syntax::Expr;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::compiler) struct SuppliedArguments(Vec<SuppliedArgument>);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SuppliedArgument {
    name: Option<String>,
    spread: bool,
    literal: Option<LiteralType>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LiteralType {
    Int,
    Float,
    Boolean,
    String,
    Char,
    Null,
    Unit,
}
impl LiteralType {
    fn of(expression: &Expr) -> Option<Self> {
        match expression {
            Expr::Group(inner) => Self::of(inner),
            Expr::String(_) | Expr::Format(_) => Some(Self::String),
            Expr::Char(_) => Some(Self::Char),
            Expr::Null => Some(Self::Null),
            Expr::Name(name) if name == "Unit" => Some(Self::Unit),
            _ => {
                if let Some((ty, _)) = super::super::constants::constant_number(expression).ok()? {
                    match ty {
                        Type::Int => Some(Self::Int),
                        Type::Float => Some(Self::Float),
                        _ => None,
                    }
                } else {
                    super::super::constants::constant_boolean(expression).map(|_| Self::Boolean)
                }
            }
        }
    }
    fn ty(self) -> Type {
        match self {
            Self::Int => Type::Int,
            Self::Float => Type::Float,
            Self::Boolean => Type::Boolean,
            Self::String => Type::String,
            Self::Char => Type::Char,
            Self::Null => Type::Null,
            Self::Unit => Type::Unit,
        }
    }
}

fn closed_type(ty: &Type) -> bool {
    !ty.any_node(|ty| matches!(ty, Type::Parameter { .. } | Type::Star | Type::Pending))
}
impl SuppliedArguments {
    pub(super) fn new(arguments: &[(Option<String>, Expr)]) -> Self {
        Self(
            arguments
                .iter()
                .map(|(name, value)| SuppliedArgument {
                    name: name.clone(),
                    spread: matches!(value, Expr::Spread(_)),
                    literal: LiteralType::of(value),
                })
                .collect(),
        )
    }
    pub(super) fn bytes(&self) -> usize {
        self.0.len() * std::mem::size_of::<SuppliedArgument>()
            + self
                .0
                .iter()
                .filter_map(|argument| argument.name.as_ref())
                .map(String::len)
                .sum::<usize>()
    }
    fn positions(&self, signature: &Signature) -> Option<Vec<usize>> {
        arguments::positions(
            signature,
            self.0.iter().map(|argument| arguments::Form {
                name: argument.name.as_deref(),
                spread: argument.spread,
            }),
        )
        .ok()
    }
    pub(super) fn omitted(&self, signature: &Signature) -> Option<Vec<usize>> {
        Some(arguments::omitted_defaults(signature, &self.positions(signature)?))
    }
    pub(super) fn selected(&self, declarations: &[Signature]) -> Option<usize> {
        if declarations.len() < 2
            || self.0.iter().any(|argument| argument.literal.is_none())
            || declarations.iter().any(|signature| {
                !signature.generics.is_empty() || signature.params.iter().any(|parameter| !closed_type(&parameter.ty))
            })
        {
            return None;
        }
        let mut candidates = Vec::new();
        for (index, signature) in declarations.iter().enumerate() {
            let Some(positions) = self.positions(signature) else {
                continue;
            };
            let conversions = self
                .0
                .iter()
                .zip(&positions)
                .map(|(argument, index)| {
                    let parameter = &signature.params[*index];
                    let target = if parameter.vararg && (argument.name.is_some() || argument.spread) {
                        Type::named("List", vec![parameter.ty.clone()])
                    } else {
                        parameter.ty.clone()
                    };
                    target.conversion_from(&argument.literal?.ty())
                })
                .collect::<Option<Vec<_>>>();
            let Some(conversions) = conversions else { continue };
            candidates.push((
                index,
                OverloadRank {
                    conversions,
                    omitted_defaults: arguments::omitted_defaults(signature, &positions).len(),
                    variadic: signature.params.last().is_some_and(|parameter| parameter.vararg),
                    generic: false,
                },
            ));
        }
        best_match(candidates.iter().map(|(_, rank)| rank)).map(|index| candidates[index].0)
    }
}
