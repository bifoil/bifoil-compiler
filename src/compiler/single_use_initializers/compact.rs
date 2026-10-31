//! A compact body must mean the same thing in its caller's checking environment.
//!
//! General pure expressions can infer different collection types or read a local
//! visible only at the call. Retain their frames. Scalar expressions expand only
//! when every name has its fixed declaration type and exists at both source sites.

use crate::compiler::{ScopeStack, bindings::lookup_ref};
use crate::model::Type;
use crate::syntax::{Expr, FunctionAst, Statement, StatementKind};
use std::collections::BTreeMap;

pub(super) struct Bindings<'a> {
    entry: &'a ScopeStack,
    body: &'a [Statement],
    declarations: &'a BTreeMap<&'a str, usize>,
    delayed: BTreeMap<&'a str, (usize, &'a Type)>,
}

impl<'a> Bindings<'a> {
    pub(super) fn new(
        body: &'a [Statement],
        entry: &'a ScopeStack,
        declarations: &'a BTreeMap<&'a str, usize>,
    ) -> Self {
        Self {
            entry,
            body,
            declarations,
            delayed: body
                .iter()
                .enumerate()
                .filter_map(|(index, statement)| match &statement.kind {
                    StatementKind::Uninitialized { name, ty, .. } => Some((name.as_str(), (index, ty))),
                    _ => None,
                })
                .collect(),
        }
    }

    pub(super) fn permits(&self, function: &FunctionAst, before: usize) -> bool {
        function.body.iter().all(|statement| {
            matches!(&statement.kind, StatementKind::Assign { name, value, .. }
                if self.declared_type(name, before).is_some() && self.scalar(value, before))
        })
    }

    fn declared_type(&self, name: &str, before: usize) -> Option<&Type> {
        if let Some(binding) = lookup_ref(self.entry, name) {
            return (!binding.implicit_receiver && binding.callables.is_empty()).then_some(&binding.ty);
        }
        self.delayed
            .get(name)
            .filter(|(index, _)| *index < before)
            .map(|(_, ty)| *ty)
    }

    /// A named reference loads the same block-entry callable at either source
    /// site. Its captured cells must also exist in this lexical environment.
    fn reference(&self, name: &str, before: usize) -> bool {
        if self.declarations.get(name) != Some(&1) {
            return false;
        }
        self.body.iter().any(|statement| {
            let StatementKind::Function(function) = &statement.kind else {
                return false;
            };
            function.name == name
                && function.generics.is_empty()
                && crate::compiler::captures::captured_names(function)
                    .iter()
                    .all(|name| self.declared_type(name, before).is_some())
        })
    }

    fn scalar(&self, value: &Expr, before: usize) -> bool {
        match value {
            Expr::Int(_) | Expr::Float(_) | Expr::Boolean(_) | Expr::String(_) | Expr::Char(_) | Expr::Null => true,
            Expr::Name(name) => self.declared_type(name, before).is_some_and(|ty| {
                matches!(
                    ty,
                    Type::Int | Type::Float | Type::Boolean | Type::String | Type::Char | Type::Unit
                )
            }),
            Expr::Reference(name) => self.reference(name, before),
            Expr::Group(value) | Expr::Unary(_, value) => self.scalar(value, before),
            Expr::Binary(_, left, right) => self.scalar(left, before) && self.scalar(right, before),
            Expr::Format(parts) => parts.iter().all(|part| self.scalar(&part.value, before)),
            _ => false,
        }
    }
}
