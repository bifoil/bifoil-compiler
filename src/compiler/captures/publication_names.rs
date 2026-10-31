//! Conservative publication summaries over a callable's free lexical names.
//! Reads, invoked helpers, and newly created closures have different retention
//! behavior. Local aliases are covered by their initializers; scope filtering
//! reuses the free-name analysis instead of inventing another name resolver.

use super::names::requirements_only;
use super::publication::{Publication, Publications};
use super::{Requirement, Requirements};
use crate::syntax::{Expr, Pattern, Statement, StatementKind};
use std::collections::BTreeSet;

pub(super) fn body(statements: &[Statement], free: &Requirements) -> Publications {
    let mut effects = Publications::new();
    collect_body(statements, &mut effects);
    retain_free(&mut effects, free);
    effects
}
pub(super) fn expression(value: &Expr, free: &Requirements) -> Publications {
    let mut effects = Publications::new();
    collect_expression(value, &mut effects);
    retain_free(&mut effects, free);
    effects
}
fn retain_free(effects: &mut Publications, free: &Requirements) {
    let names = free.iter().map(|required| required.name()).collect::<BTreeSet<_>>();
    effects.retain(|effect| {
        let mut path = effect.requirement().name();
        loop {
            if names.contains(path) {
                return true;
            }
            let Some((receiver, _)) = path.rsplit_once('.') else {
                return false;
            };
            path = receiver;
        }
    });
}

fn collect_expression(value: &Expr, effects: &mut Publications) {
    match value {
        Expr::Block(statements) => collect_body(statements, effects),
        Expr::Lambda(fun) => {
            effects.extend(requirements_only(fun).body.into_iter().map(Publication::Capture));
        }
        Expr::Name(name) | Expr::Reference(name) => {
            if name != "Unit" {
                effects.insert(Publication::Value(Requirement::Binding(name.clone())));
            }
        }
        Expr::Group(value)
        | Expr::Unary(_, value)
        | Expr::Unwrap(value)
        | Expr::Spread(value)
        | Expr::BoundReference(value, _) => collect_expression(value, effects),
        Expr::TypeApply { callee, .. } => collect_expression(callee, effects),
        Expr::Construct { fields, .. } => {
            for (_, value) in fields {
                collect_expression(value, effects);
            }
        }
        Expr::Member { receiver, .. } => {
            if let Some(path) = super::super::expression_path(value) {
                effects.insert(Publication::Value(Requirement::Binding(path)));
            } else {
                collect_expression(receiver, effects);
            }
        }
        Expr::TypeOp { value, .. } => collect_expression(value, effects),
        Expr::Index { receiver, index, .. } | Expr::Binary(_, receiver, index) => {
            collect_expression(receiver, effects);
            collect_expression(index, effects);
        }
        Expr::Call(callee, arguments) | Expr::SafeCall(callee, arguments) => {
            let base = super::super::overloads::callable_base(callee);
            let name = match base {
                Expr::Reference(name) => Some(name.clone()),
                _ => super::super::expression_path(base),
            };
            if let Some(name) = name {
                effects.insert(Publication::Invoke(Requirement::Call {
                    name,
                    supplied: super::call_profile::SuppliedArguments::new(arguments),
                }));
            } else {
                collect_expression(callee, effects);
            }
            for (_, value) in arguments {
                collect_expression(value, effects);
            }
        }
        Expr::Collection { entries, .. } => {
            for (key, value) in entries {
                if let Some(key) = key {
                    collect_expression(key, effects);
                }
                collect_expression(value, effects);
            }
        }
        Expr::Transfer { value: Some(value), .. } => collect_expression(value, effects),
        Expr::Format(parts) => {
            for part in parts {
                collect_expression(&part.value, effects);
            }
        }
        _ => {}
    }
}
fn collect_body(statements: &[Statement], effects: &mut Publications) {
    for statement in statements {
        match &statement.kind {
            StatementKind::Narrative(body) => collect_body(body, effects),
            StatementKind::Text(value)
            | StatementKind::Yield { value, .. }
            | StatementKind::Variable { value, .. }
            | StatementKind::Assign { value, .. }
            | StatementKind::Expression(value)
            | StatementKind::Return(Some(value)) => {
                collect_expression(value, effects);
            }
            StatementKind::Write { target, value, .. } => {
                collect_expression(target, effects);
                collect_expression(value, effects);
            }
            StatementKind::Function(fun) => {
                effects.extend(
                    requirements_only(fun)
                        .all_requirements()
                        .into_iter()
                        .map(Publication::Capture),
                );
            }
            StatementKind::If { branches, no } => {
                for branch in branches {
                    collect_expression(&branch.condition, effects);
                    collect_body(&branch.body, effects);
                }
                collect_body(no, effects);
            }
            StatementKind::For { iterable, body, .. } => {
                collect_expression(iterable, effects);
                collect_body(body, effects);
            }
            StatementKind::While { condition, body, .. } => {
                collect_expression(condition, effects);
                collect_body(body, effects);
            }
            StatementKind::When { subject, cases, .. } => {
                if let Some(subject) = subject {
                    collect_expression(subject, effects);
                }
                for case in cases {
                    for pattern in &case.patterns {
                        if let Pattern::Value(value) | Pattern::Membership { value, .. } = pattern {
                            collect_expression(value, effects);
                        }
                    }
                    if let Some(guard) = &case.guard {
                        collect_expression(guard, effects);
                    }
                    collect_body(&case.body, effects);
                }
            }
            StatementKind::Choice { options, fallback } => {
                for option in options {
                    collect_expression(&option.visible, effects);
                    collect_expression(&option.enabled, effects);
                    collect_expression(&option.label, effects);
                    collect_body(&option.body, effects);
                }
                if let Some(body) = fallback {
                    collect_body(body, effects);
                }
            }
            StatementKind::Alias(_)
            | StatementKind::ParagraphBreak
            | StatementKind::Uninitialized { .. }
            | StatementKind::Return(None) => {}
        }
    }
}
