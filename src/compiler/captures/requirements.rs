//! Initialization dependencies distinguish a value read from a named call.
//! Call profiles survive transitive expansion; function values expose no defaults.

use super::super::Binding;
pub(super) use super::call_profile::SuppliedArguments;
use crate::compiler::overloads::callable_base;
use crate::syntax::Expr;
use std::borrow::Cow;
use std::collections::BTreeSet;

pub(in crate::compiler) type Requirements = BTreeSet<Requirement>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::compiler) enum Requirement {
    Binding(String),
    Write(String),
    Call { name: String, supplied: SuppliedArguments },
    Scoped { depth: usize, required: Box<Requirement> },
}
impl Requirement {
    pub(in crate::compiler) fn name(&self) -> &str {
        match self {
            Self::Binding(name) | Self::Write(name) | Self::Call { name, .. } => name,
            Self::Scoped { required, .. } => required.name(),
        }
    }
    pub(in crate::compiler) fn reads_binding(&self) -> bool {
        !matches!(self.unscoped(), Self::Write(_))
    }
    pub(in crate::compiler) fn unscoped(&self) -> &Self {
        let mut required = self;
        while let Self::Scoped { required: inner, .. } = required {
            required = inner;
        }
        required
    }
    pub(in crate::compiler) fn scope_depth(&self) -> Option<usize> {
        if let Self::Scoped { depth, .. } = self {
            Some(*depth)
        } else {
            None
        }
    }
    /// A deeper caller cannot supply a different binding with this source name.
    /// Retiring scopes may tighten the limit to the binding actually retained.
    pub(in crate::compiler) fn at_depth(&self, depth: usize) -> Self {
        Self::Scoped {
            depth: self.scope_depth().map_or(depth, |earlier| earlier.min(depth)),
            required: Box::new(self.unscoped().clone()),
        }
    }
    pub(in crate::compiler) fn call(callee: &Expr, arguments: &[(Option<String>, Expr)]) -> Option<Self> {
        let callee = callable_base(callee);
        if matches!(callee, Expr::Reference(_) | Expr::BoundReference(..)) {
            return None;
        }
        let name = super::super::expression_path(callee)?;
        Some(Self::Call {
            name,
            supplied: SuppliedArguments::new(arguments),
        })
    }

    /// Shape-compatible overloads are conservative when argument types are
    /// unavailable. Direct lowering still checks the selected overload precisely.
    pub(in crate::compiler) fn dependencies<'a>(&self, binding: &'a Binding) -> Cow<'a, Requirements> {
        // A plain write captures the cell's address without reading either its
        // initial value or any deferred callback stored in that old value.
        let supplied = match self.unscoped() {
            Self::Scoped { .. } => unreachable!("unscoped requirement"),
            Self::Write(_) => return Cow::Owned(Requirements::new()),
            Self::Binding(_) => None,
            Self::Call { supplied, .. } => Some(supplied),
        };
        let selected = supplied.and_then(|supplied| supplied.selected(&binding.callables));
        let mut result: Option<Cow<'a, Requirements>> = None;
        for (index, signature) in binding.callables.iter().enumerate() {
            if selected.is_some_and(|selected| selected != index) {
                continue;
            }
            let omitted = if let Some(supplied) = supplied {
                let Some(omitted) = supplied.omitted(signature) else {
                    continue;
                };
                omitted
            } else {
                Vec::new()
            };
            let Some(capture) = binding.callable_requirements.get(&signature.id) else {
                continue;
            };
            match &mut result {
                None => result = Some(Cow::Borrowed(&capture.body)),
                Some(previous) if !capture.body.is_empty() => previous.to_mut().extend(capture.body.iter().cloned()),
                _ => {}
            }
            for index in omitted {
                if let Some(default) = capture.defaults.get(&index).filter(|names| !names.is_empty()) {
                    result.as_mut().unwrap().to_mut().extend(default.iter().cloned());
                }
            }
        }
        result.unwrap_or(Cow::Borrowed(&binding.requires))
    }
}
