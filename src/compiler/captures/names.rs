//! Free names in callable bodies and individual default expressions.
//! Default-local declarations stay inside their expression block.

use super::super::bindings::CallableRequirements;
use super::{Requirement, Requirements};
use crate::syntax::{Expr, FunctionAst, Pattern, Statement, StatementKind};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::compiler) fn captured_names(fun: &FunctionAst) -> BTreeSet<String> {
    requirements_only(fun).all_names()
}

/// Free names for a whole statement, including nested/default expressions.
pub(in crate::compiler) fn statement_names(statement: &Statement) -> BTreeSet<String> {
    if let StatementKind::Function(function) = &statement.kind {
        return captured_names(function);
    }
    let mut bound = BTreeSet::new();
    let mut used = Requirements::new();
    capture_body(std::slice::from_ref(statement), &mut bound, &mut used);
    remove_bound(&mut used, &bound);
    used.into_iter().map(|required| required.name().to_owned()).collect()
}

/// Free names in code that shares a body's scope, such as a do/while condition.
pub(in crate::compiler) fn expression_names(value: &Expr) -> BTreeSet<String> {
    let mut used = Requirements::new();
    capture_expression(value, &mut used);
    used.into_iter().map(|required| required.name().to_owned()).collect()
}

/// Captured binding writes in structured bodies. Local declarations stay local;
/// ordinary type/flow checks still reject shadowing or writes by nested helpers.
pub(in crate::compiler) fn assigned_names(fun: &FunctionAst) -> BTreeSet<String> {
    requirements_only(fun)
        .body
        .into_iter()
        .filter_map(|required| match required.unscoped() {
            Requirement::Write(name) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

pub(in crate::compiler) fn callable_captures(fun: &FunctionAst) -> CallableRequirements {
    let mut captures = requirements_only(fun);
    captures.publications = super::publication_names::body(&fun.body, &captures.body);
    for (index, parameter) in fun.params.iter().enumerate() {
        if let Some(value) = &parameter.default {
            captures.default_publications.insert(
                index,
                super::publication_names::expression(value, &captures.defaults[&index]),
            );
        }
    }
    captures
}

/// Nested closure discovery needs only free names, not another recursive pass
/// over publication effects. Keep the two queries separate to bound traversal.
pub(super) fn requirements_only(fun: &FunctionAst) -> CallableRequirements {
    let parameters = fun.params.iter().map(|p| p.name.clone()).collect::<BTreeSet<_>>();
    let mut bound = parameters.clone();
    let mut body = BTreeSet::new();
    capture_body(&fun.body, &mut bound, &mut body);
    remove_bound(&mut body, &bound);
    let mut defaults = BTreeMap::new();
    for (index, parameter) in fun.params.iter().enumerate() {
        if let Some(value) = &parameter.default {
            let mut names = BTreeSet::new();
            capture_expression(value, &mut names);
            remove_bound(&mut names, &parameters);
            defaults.insert(index, names);
        }
    }
    CallableRequirements {
        body,
        defaults,
        ..Default::default()
    }
}

/// Capture requirements describe lexical bindings, not every identifier token.
/// Keep later call-site variables when visible at the function declaration, but
/// leave enum shorthand, built-ins, and invalid names to semantic resolution.
pub(in crate::compiler) fn named_captures<'a>(
    body: &'a [Statement],
    env: &super::super::ScopeStack,
) -> Vec<(&'a FunctionAst, CallableRequirements)> {
    let mut visible = body
        .iter()
        .filter_map(|statement| match &statement.kind {
            StatementKind::Function(fun) => Some(fun.name.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if visible.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    for statement in body {
        match &statement.kind {
            StatementKind::Function(fun) => {
                let mut captures = callable_captures(fun);
                let lexical = |required: &Requirement| {
                    let name = required.name();
                    if super::capture_binding(env, name).is_some() {
                        return true;
                    }
                    let mut path = name;
                    loop {
                        if visible.contains(path) {
                            return true;
                        }
                        let Some((receiver, _)) = path.rsplit_once('.') else {
                            return false;
                        };
                        path = receiver;
                    }
                };
                captures.body.retain(lexical);
                for names in captures.defaults.values_mut() {
                    names.retain(lexical);
                }
                captures.publications.retain(|effect| lexical(effect.requirement()));
                for effects in captures.default_publications.values_mut() {
                    effects.retain(|effect| lexical(effect.requirement()));
                }
                result.push((fun, captures.at_declaration_depth(env.len())));
            }
            StatementKind::Variable { name, .. } | StatementKind::Uninitialized { name, .. } => {
                visible.insert(name.clone());
            }
            _ => {}
        }
    }
    result
}

fn remove_bound(used: &mut Requirements, bound: &BTreeSet<String>) {
    used.retain(|required| !bound.contains(required.name().split('.').next().unwrap()));
}

fn capture_expression(e: &Expr, used: &mut Requirements) {
    match e {
        Expr::Block(part) => {
            let mut bound = BTreeSet::new();
            let mut local = BTreeSet::new();
            capture_body(part, &mut bound, &mut local);
            remove_bound(&mut local, &bound);
            used.extend(local);
        }
        Expr::Lambda(fun) => used.extend(requirements_only(fun).all_requirements()),
        Expr::Name(n) | Expr::Reference(n) => {
            if n != "Unit" {
                used.insert(Requirement::Binding(n.clone()));
            }
        }
        Expr::Group(e) | Expr::Unary(_, e) | Expr::Unwrap(e) | Expr::Spread(e) | Expr::BoundReference(e, _) => {
            capture_expression(e, used)
        }
        Expr::TypeApply { callee, .. } => capture_expression(callee, used),
        Expr::Construct { fields, .. } => {
            for (_, value) in fields {
                capture_expression(value, used);
            }
        }
        Expr::Member { receiver, .. } => {
            if let Some(path) = super::super::expression_path(e) {
                used.insert(Requirement::Binding(path));
            } else {
                capture_expression(receiver, used);
            }
        }
        Expr::TypeOp { value, .. } => capture_expression(value, used),
        Expr::Index { receiver, index, .. } | Expr::Binary(_, receiver, index) => {
            capture_expression(receiver, used);
            capture_expression(index, used);
        }
        Expr::Call(e, args) | Expr::SafeCall(e, args) => {
            if let Some(call) = Requirement::call(e, args) {
                used.insert(call);
            } else {
                capture_expression(e, used);
            }
            for (_, e) in args {
                capture_expression(e, used)
            }
        }
        Expr::Collection { entries, .. } => {
            for (key, value) in entries {
                if let Some(key) = key {
                    capture_expression(key, used)
                }
                capture_expression(value, used)
            }
        }
        Expr::Transfer { value: Some(e), .. } => capture_expression(e, used),
        Expr::Format(parts) => {
            for part in parts {
                capture_expression(&part.value, used)
            }
        }
        _ => {}
    }
}
fn capture_body(statements: &[Statement], bound: &mut BTreeSet<String>, used: &mut Requirements) {
    for s in statements {
        match &s.kind {
            StatementKind::Narrative(part) => capture_body(part, bound, used),
            StatementKind::Text(value) | StatementKind::Yield { value, .. } => capture_expression(value, used),
            StatementKind::ParagraphBreak => {}
            StatementKind::When {
                subject,
                binding,
                cases,
                ..
            } => {
                if let Some(subject) = subject {
                    capture_expression(subject, used)
                }
                if let Some(binding) = binding {
                    bound.insert(binding.clone());
                }
                for case in cases {
                    for pattern in &case.patterns {
                        match pattern {
                            Pattern::Value(e) | Pattern::Membership { value: e, .. } => capture_expression(e, used),
                            Pattern::Type {
                                bindings: Some(names), ..
                            } => {
                                for name in names {
                                    if name != "_" {
                                        bound.insert(name.clone());
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(guard) = &case.guard {
                        capture_expression(guard, used)
                    }
                    capture_body(&case.body, bound, used);
                }
            }
            StatementKind::Variable { name, value, .. } => {
                bound.insert(name.clone());
                capture_expression(value, used);
            }
            StatementKind::Uninitialized { name, .. } => {
                bound.insert(name.clone());
            }
            StatementKind::Assign { name, operator, value } => {
                used.insert(if operator == "=" {
                    Requirement::Write(name.clone())
                } else {
                    Requirement::Binding(name.clone())
                });
                capture_expression(value, used);
            }
            StatementKind::Write { target, value, .. } => {
                capture_expression(target, used);
                capture_expression(value, used);
            }
            StatementKind::Expression(e) | StatementKind::Return(Some(e)) => capture_expression(e, used),
            StatementKind::Function(f) => {
                bound.insert(f.name.clone());
                used.extend(requirements_only(f).all_requirements());
            }
            StatementKind::If { branches, no } => {
                for branch in branches {
                    capture_expression(&branch.condition, used);
                    capture_body(&branch.body, bound, used);
                }
                capture_body(no, bound, used);
            }
            StatementKind::For {
                name,
                iterable,
                body: part,
                ..
            } => {
                bound.insert(name.clone());
                capture_expression(iterable, used);
                capture_body(part, bound, used);
            }
            StatementKind::While {
                condition, body: part, ..
            } => {
                capture_expression(condition, used);
                capture_body(part, bound, used);
            }
            StatementKind::Choice { options, fallback } => {
                for o in options {
                    capture_expression(&o.visible, used);
                    capture_expression(&o.enabled, used);
                    capture_expression(&o.label, used);
                    capture_body(&o.body, bound, used)
                }
                if let Some(part) = fallback {
                    capture_body(part, bound, used)
                }
            }
            StatementKind::Alias(_) => {}
            StatementKind::Return(None) => {}
        }
    }
}
