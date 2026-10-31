//! Bounded identities for repeated, side-effect-free primitive key expressions.
//! No user code executes here. Normal index checking and lowering retain
//! their conversions, overflow checks, evaluation order, and runtime reads.

use super::{ConstantKey, ScopeStack, lookup_ref};
use crate::compiler::bindings::{Binding, Identity};
use crate::model::Type;
use crate::syntax::Expr;
use std::sync::Arc;

const MAX_NODES: usize = 128;
const MAX_BYTES: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::compiler) struct Key {
    expression: Arc<Expression>,
    bindings: Arc<[Dependency]>,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Dependency {
    identity: Identity,
    name: String,
    mutable: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Expression {
    Constant(ConstantKey),
    Binding(usize),
    Fields(usize, Arc<[String]>),
    Format(Vec<Expression>),
    Length(Box<Expression>),
    TextIndex(Box<Expression>, Box<Expression>),
    Unary(String, Box<Expression>),
    Binary(String, Box<Expression>, Box<Expression>),
}
struct Builder {
    nodes: usize,
    bytes: usize,
    bindings: Vec<Dependency>,
}
impl Key {
    pub(super) fn source(expression: &Expr, env: &ScopeStack) -> Option<Self> {
        let mut builder = Builder {
            nodes: MAX_NODES,
            bytes: MAX_BYTES,
            bindings: Vec::new(),
        };
        let expression = builder.expression(expression, env)?;
        Some(Self {
            expression: Arc::new(expression),
            bindings: builder.bindings.into(),
        })
    }
    pub(super) fn bindings_match(&self, env: &ScopeStack) -> bool {
        self.bindings.iter().all(|dependency| {
            lookup_ref(env, &dependency.name)
                .is_some_and(|binding| binding.identity == dependency.identity && !binding.volatile)
        })
    }
    pub(super) fn mutable(&self) -> bool {
        self.bindings.iter().any(|binding| binding.mutable)
    }
    pub(super) fn mark_mutable_bindings(&self, env: &mut ScopeStack) {
        for dependency in self.bindings.iter().filter(|binding| binding.mutable) {
            super::mark_index_key(dependency.identity, &dependency.name, env);
        }
    }
}
impl Builder {
    fn charge(&mut self, bytes: usize) -> Option<()> {
        self.bytes = self.bytes.checked_sub(bytes)?;
        Some(())
    }
    fn expression(&mut self, expression: &Expr, env: &ScopeStack) -> Option<Expression> {
        if let Expr::Group(inner) = expression {
            return self.expression(inner, env);
        }
        self.nodes = self.nodes.checked_sub(1)?;
        if matches!(expression, Expr::String(text) | Expr::Char(text) if text.len() > self.bytes) {
            return None;
        }
        if let Some(key) = ConstantKey::source(expression, env, None) {
            let bytes = match &key {
                ConstantKey::String(text) | ConstantKey::Char(text) => text.len(),
                ConstantKey::Enum { declaration, case } => declaration.len().checked_add(case.len())?,
                _ => 0,
            };
            self.charge(bytes)?;
            return Some(Expression::Constant(key));
        }
        match expression {
            Expr::Unary(operator, inner) if ["!", "-", "+"].contains(&operator.as_str()) => {
                self.charge(operator.len())?;
                Some(Expression::Unary(
                    operator.clone(),
                    Box::new(self.expression(inner, env)?),
                ))
            }
            Expr::Binary(operator, left, right)
                if [
                    "+", "-", "*", "/", "%", "%+", "%-", "==", "!=", "===", "!==", "<", "<=", ">", ">=", "&&", "||",
                ]
                .contains(&operator.as_str()) =>
            {
                self.charge(operator.len())?;
                let left = self.expression(left, env)?;
                let right = self.expression(right, env)?;
                Some(Expression::Binary(operator.clone(), Box::new(left), Box::new(right)))
            }
            Expr::Format(parts) => Some(Expression::Format(
                parts
                    .iter()
                    .map(|part| self.expression(&part.value, env))
                    .collect::<Option<Vec<_>>>()?,
            )),
            Expr::Member {
                receiver,
                name,
                safe: false,
            } if name == "length" => self
                .value_path(expression, env)
                .or_else(|| Some(Expression::Length(Box::new(self.expression(receiver, env)?)))),
            Expr::Index {
                receiver,
                index,
                safe: false,
            } => Some(Expression::TextIndex(
                Box::new(self.expression(receiver, env)?),
                Box::new(self.expression(index, env)?),
            )),
            Expr::Name(_) | Expr::Member { .. } => self.value_path(expression, env),
            // Calls, mutable/composite heap reads, casts, and block expressions
            // do not establish a proof, even when their result happens to repeat.
            _ => None,
        }
    }

    fn primitive(ty: &Type) -> bool {
        matches!(
            crate::generics::operation_view(ty),
            Type::Int | Type::Float | Type::Boolean | Type::String | Type::Char | Type::Unit
        )
    }
    fn dependency(&mut self, mut name: String, binding: &Binding) -> Option<usize> {
        if binding.volatile {
            return None;
        }
        if let Some(aliases) = &binding.global_aliases {
            name = aliases[0].clone();
        }
        if let Some(index) = self.bindings.iter().position(|item| item.identity == binding.identity) {
            return Some(index);
        }
        self.charge(name.len())?;
        self.bindings.push(Dependency {
            identity: binding.identity,
            name,
            mutable: binding.mutable,
        });
        Some(self.bindings.len() - 1)
    }
    fn value_path(&mut self, expression: &Expr, env: &ScopeStack) -> Option<Expression> {
        // Requiring an ordinary dotted path avoids recursive key resolution here.
        let name = super::super::expression_path(expression)?;
        if let Some(binding) = lookup_ref(env, &name) {
            if !Self::primitive(&binding.value_type(None)) {
                return None;
            }
            return self.dependency(name, binding).map(Expression::Binding);
        }
        let (path, declared) = super::resolved(expression, env)?;
        if !path.safe_prefixes.is_empty() || path.steps.is_empty() {
            return None;
        }
        let ty = super::fact_path(&path, env).unwrap_or(declared);
        if !Self::primitive(&ty) {
            return None;
        }
        let mut names = Vec::with_capacity(path.steps.len());
        for step in &path.steps {
            let super::Step::Field(name) = step else {
                return None;
            };
            self.nodes = self.nodes.checked_sub(1)?;
            self.charge(name.len())?;
            names.push(name.clone());
        }
        let root = lookup_ref(env, &path.root)?;
        let index = self.dependency(path.root, root)?;
        Some(Expression::Fields(index, names.into()))
    }
}
