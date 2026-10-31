//! Reachability, branch joins, narrowing, and loop initialization proofs.

use super::bindings::lookup_ref;
use super::{FunctionContext, ScopeStack};
use crate::model::Type;
use crate::syntax::{Expr, Pattern, WhenCase};
use std::sync::Arc;

/// State retained by a lazy expression when its remaining operands are skipped.
/// Effects on the evaluated path cannot become definite facts on the skipped path.
pub(super) struct SkippedPath {
    scopes: ScopeStack,
    reachable: bool,
}
impl SkippedPath {
    pub(super) fn new(context: &FunctionContext) -> Self {
        Self {
            scopes: context.scopes.clone(),
            reachable: context.is_reachable,
        }
    }
    pub(super) fn merge(self, context: &mut FunctionContext) {
        let mut exits = Vec::new();
        if self.reachable {
            exits.push(self.scopes.clone());
        }
        if context.is_reachable {
            exits.push(context.scopes.clone());
        }
        context.scopes = merge_env(&self.scopes, &exits);
        context.is_reachable = !exits.is_empty();
    }
}

/// Merge completed alternatives without retaining a full frame per branch.
/// The first path is anchored to the original declarations; later joins combine
/// only flow facts, whose intersections/unions are associative. Unreachable
/// paths must not be added by the caller.
#[derive(Default)]
pub(super) struct FlowJoin {
    scopes: Option<ScopeStack>,
}
impl FlowJoin {
    pub(super) fn add(&mut self, base: &ScopeStack, path: ScopeStack) {
        self.scopes = Some(match self.scopes.take() {
            Some(previous) => merge_env(&previous, &[previous.clone(), path]),
            None => merge_env(base, std::slice::from_ref(&path)),
        });
    }

    pub(super) fn is_empty(&self) -> bool {
        self.scopes.is_none()
    }

    pub(super) fn finish(self, base: &ScopeStack) -> ScopeStack {
        self.scopes.unwrap_or_else(|| base.clone())
    }
}

pub(super) fn merge_env(base: &ScopeStack, branches: &[ScopeStack]) -> ScopeStack {
    let mut result = base.clone();
    if branches.is_empty() {
        // No reachable successor consumes the resulting facts.
        return result;
    }
    for (index, scope) in base.iter().enumerate() {
        if branches
            .iter()
            .all(|branch| branch.get(index).is_some_and(|other| Arc::ptr_eq(scope, other)))
        {
            continue;
        }
        for (name, original) in scope.iter() {
            let states = branches
                .iter()
                .map(|branch| lookup_ref(branch, name))
                .collect::<Vec<_>>();
            let initialized = states
                .iter()
                .all(|state| state.is_some_and(|binding| binding.initialized));
            let may_be_initialized = states
                .iter()
                .any(|state| state.is_some_and(|binding| binding.may_be_initialized));
            let narrow = states
                .first()
                .and_then(|state| *state)
                .and_then(|binding| binding.narrow.clone())
                .filter(|ty| {
                    states
                        .iter()
                        .all(|state| state.is_some_and(|binding| binding.narrow.as_ref() == Some(ty)))
                });
            let field_narrows = super::field_proofs::merge(&states);
            let escaped = states.iter().any(|state| state.is_some_and(|binding| binding.escaped));
            let index_key = states
                .iter()
                .any(|state| state.is_some_and(|binding| binding.index_key));
            let requires = states
                .iter()
                .filter_map(|state| *state)
                .flat_map(|binding| binding.requires.iter().cloned())
                .collect();
            if original.escaped == escaped
                && original.index_key == index_key
                && original.requires == requires
                && original.initialized == initialized
                && original.may_be_initialized == may_be_initialized
                && original.narrow == narrow
                && original.field_narrows == field_narrows
            {
                continue;
            }
            let target = Arc::make_mut(&mut result[index]).get_mut(name).unwrap();
            target.escaped = escaped;
            target.index_key = index_key;
            target.initialized = initialized;
            target.may_be_initialized = may_be_initialized;
            target.narrow = narrow;
            target.field_narrows = field_narrows;
            target.requires = requires;
        }
    }
    result
}

pub(super) fn narrow_condition(expr: &Expr, env: &mut ScopeStack, positive: bool) {
    narrow_condition_mode(expr, env, positive, false);
}

/// A selected option proves its snapshotted guards, but later preparation may
/// have changed mutable bindings. Immutable bindings retain those facts.
pub(super) fn narrow_stable_condition(expr: &Expr, env: &mut ScopeStack) {
    narrow_condition_mode(expr, env, true, true);
}

fn narrow_condition_mode(expr: &Expr, env: &mut ScopeStack, positive: bool, stable_only: bool) {
    match expr {
        Expr::Group(e) => narrow_condition_mode(e, env, positive, stable_only),
        Expr::Unary(op, e) if op == "!" => narrow_condition_mode(e, env, !positive, stable_only),
        Expr::Binary(op, a, b) if (op == "&&" && positive) || (op == "||" && !positive) => {
            narrow_condition_mode(a, env, positive, stable_only);
            // A later operand may change a captured var after the earlier
            // test. Only facts established after that effect remain valid.
            if !stable_only && !pure_expression(b) {
                invalidate_narrowing(env);
            }
            narrow_condition_mode(b, env, positive, stable_only);
        }
        Expr::Binary(op, a, b) if ["==", "!=", "===", "!=="].contains(&op.as_str()) => {
            let checked = match (a.as_ref(), b.as_ref()) {
                (value, Expr::Null) | (Expr::Null, value) => Some(value),
                _ => None,
            };
            if positive == (op == "!=" || op == "!==")
                && let Some(value) = checked
            {
                narrow_value(value, env, None, stable_only);
            }
        }
        Expr::TypeOp { value, operator, ty } if (operator == "is" && positive) || (operator == "!is" && !positive) => {
            narrow_value(value, env, Some(ty), stable_only);
        }
        _ => {
            if !stable_only && !pure_expression(expr) {
                invalidate_narrowing(env);
            }
        }
    }
}

fn narrow_value(expression: &Expr, env: &mut ScopeStack, tested: Option<&Type>, stable_only: bool) {
    match expression {
        Expr::Group(value) => narrow_value(value, env, tested, stable_only),
        Expr::Name(name) => super::bindings::narrow_binding(env, name, tested, stable_only),
        Expr::Member { .. } | Expr::Index { .. } => super::field_proofs::narrow(expression, env, tested, stable_only),
        _ => {}
    }
}

pub(super) fn invalidate_narrowing(env: &mut ScopeStack) {
    let affected = |symbol: &super::Binding| {
        ((symbol.mutable || symbol.volatile) && (symbol.narrow.is_some() || symbol.field_narrows.is_some()))
            || super::field_proofs::transient_facts(symbol)
    };
    for scope in env {
        let names = scope
            .iter()
            .filter(|(_, symbol)| affected(symbol))
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if names.is_empty() {
            continue;
        }
        let scope = Arc::make_mut(scope);
        for name in names {
            let symbol = scope.get_mut(&name).unwrap();
            if symbol.mutable || symbol.volatile {
                symbol.narrow = None;
                symbol.field_narrows = None;
            } else {
                super::field_proofs::clear_transient(symbol);
            }
        }
    }
}

pub(super) fn check_loop_initialization(
    before: &ScopeStack,
    after: &ScopeStack,
    repeat: bool,
    continues: &[ScopeStack],
) -> Result<(), String> {
    for scope in before {
        for (name, symbol) in scope.iter() {
            if !symbol.mutable && !symbol.initialized {
                let written = |env: &ScopeStack| lookup_ref(env, name).is_some_and(|s| s.may_be_initialized);
                if (repeat && written(after)) || continues.iter().any(written) {
                    return Err(format!("val '{name}' could be initialized more than once by this loop"));
                }
            }
        }
    }
    Ok(())
}

/// Case tests describe the captured value. Value conversions or container
/// copies break its connection to the original source; subtyping preserves it.
pub(super) enum SubjectSource {
    Binding(String),
    Path {
        path: super::field_proofs::ValuePath,
        permanent: bool,
    },
}
impl SubjectSource {
    pub(super) fn capture(expression: &Expr, source: &Type, captured: &Type, env: &ScopeStack) -> Option<Self> {
        if !source.subtype_of(captured) {
            return None;
        }
        match expression {
            Expr::Group(value) | Expr::Unwrap(value) => Self::capture(value, source, captured, env),
            Expr::Name(name) => lookup_ref(env, name).map(|_| Self::Binding(name.clone())),
            _ => super::field_proofs::subject_path(expression, env)
                .map(|(path, permanent)| Self::Path { path, permanent }),
        }
    }

    pub(super) fn original(&self, pure_tests: bool) -> Option<NarrowSource<'_>> {
        match self {
            Self::Binding(name) if pure_tests => Some(NarrowSource::Binding(name)),
            Self::Path { path, permanent } if *permanent || pure_tests => Some(NarrowSource::Path(path)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum NarrowSource<'a> {
    Binding(&'a str),
    Path(&'a super::field_proofs::ValuePath),
}
impl NarrowSource<'_> {
    fn narrow(self, env: &mut ScopeStack, tested: Option<&Type>) {
        match self {
            Self::Binding(name) => super::bindings::narrow_binding(env, name, tested, false),
            Self::Path(path) => super::field_proofs::narrow_path(path, env, tested),
        }
    }
}

pub(super) fn narrow_case(
    case: &WhenCase,
    subject: Option<&Type>,
    binding: Option<&str>,
    original: Option<NarrowSource<'_>>,
    env: &mut ScopeStack,
) {
    let Some(subject) = subject else { return };
    let view = crate::generics::member_view(subject.present());
    let mut narrowed = None;
    for pattern in &case.patterns {
        let ty = match pattern {
            Pattern::Type { ty, negate: false, .. } => Some(ty.clone()),
            // A null branch does not erase a generic subject's identity. It
            // still belongs to T, even when T happens to be nullable at this call.
            Pattern::Value(Expr::Null) if matches!(subject.present(), Type::Parameter { .. }) => Some(subject.clone()),
            Pattern::Value(Expr::Null) => Some(Type::Null),
            Pattern::Value(Expr::Boolean(_)) if view == Type::Boolean => Some(subject.non_null()),
            Pattern::Value(Expr::String(_) | Expr::Char(_) | Expr::Int(_) | Expr::Float(_)) => Some(subject.non_null()),
            Pattern::Value(value) => super::infer_as(value, env, Some(subject))
                .ok()
                .filter(|ty| *ty != Type::Pending && !ty.permits_null())
                .map(|_| subject.non_null()),
            _ => None,
        };
        // Alternatives form an OR: every matching path must support the fact.
        // An unknown/nullable alternative cannot be discarded as a neutral join.
        let Some(ty) = ty else { return };
        narrowed = match narrowed {
            None => Some(ty),
            Some(previous) => previous.join(&ty),
        };
        if narrowed.is_none() {
            return;
        }
    }
    if let Some(ty) = narrowed {
        for source in binding.map(NarrowSource::Binding).into_iter().chain(original) {
            source.narrow(env, Some(&ty));
        }
    }
}

/// Null was handled by an earlier unconditional case. Only an unchanged
/// original binding, or the immutable captured subject, may use that remainder fact.
pub(super) fn narrow_when_remainder(binding: Option<&str>, original: Option<NarrowSource<'_>>, env: &mut ScopeStack) {
    for source in binding.map(NarrowSource::Binding).into_iter().chain(original) {
        source.narrow(env, None);
    }
}

pub(super) fn pure_expression(e: &Expr) -> bool {
    match e {
        Expr::Call(..) | Expr::SafeCall(..) | Expr::Transfer { .. } | Expr::Lambda(..) | Expr::Block(..) => false,
        // Taking a specialized method reference still evaluates its receiver.
        // Type arguments add no effects, but cannot hide a call below this node.
        Expr::TypeApply { callee, .. } => pure_expression(callee),
        Expr::Construct { fields, .. } => fields.iter().all(|(_, value)| pure_expression(value)),
        Expr::Group(e) | Expr::Unary(_, e) | Expr::Unwrap(e) | Expr::Spread(e) | Expr::BoundReference(e, _) => {
            pure_expression(e)
        }
        Expr::Binary(_, a, b)
        | Expr::Index {
            receiver: a, index: b, ..
        } => pure_expression(a) && pure_expression(b),
        Expr::Member { receiver, .. } | Expr::TypeOp { value: receiver, .. } => pure_expression(receiver),
        Expr::Collection { entries, .. } => entries
            .iter()
            .all(|(k, v)| k.as_ref().is_none_or(pure_expression) && pure_expression(v)),
        Expr::Format(parts) => parts.iter().all(|part| pure_expression(&part.value)),
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Boolean(_)
        | Expr::String(_)
        | Expr::Char(_)
        | Expr::Null
        | Expr::Name(_)
        | Expr::Reference(_) => true,
    }
}

/// A successful assertion checks the current value. Field paths require their
/// own stable-field proof; volatile or computed reads do not.
pub(super) fn narrow_present(expression: &Expr, env: &mut ScopeStack) {
    narrow_value(expression, env, None, false);
}

#[cfg(test)]
mod tests;
