//! Resolve annotations and turn implicit member references into explicit receivers.

use super::type_names::Scope;
use super::{register_type_aliases, resolve_generic_parameters, resolve_type};
use crate::syntax::{Expr, FunctionAst, Pattern, Statement, StatementKind};
use std::collections::BTreeSet;

/// Implicit instance members are visible bindings. Reject lexical declarations
/// that would hide them before rewriting reads or writes to explicit receivers.
fn check_member_binding(name: &str, members: &BTreeSet<String>) -> Result<(), String> {
    if members.contains(name) {
        Err(format!(
            "declaration '{name}' shadows an implicit instance member; choose a different name"
        ))
    } else {
        Ok(())
    }
}

pub(super) fn normalize_expression(e: &mut Expr, names: &Scope, members: &BTreeSet<String>) -> Result<(), String> {
    match e {
        Expr::Name(n) if members.contains(n) => {
            *e = Expr::Member {
                receiver: Box::new(Expr::Name("this".into())),
                name: n.clone(),
                safe: false,
            }
        }
        Expr::Construct { ty, fields, .. } => {
            resolve_type(ty, names)?;
            for (_, e) in fields {
                normalize_expression(e, names, members)?
            }
        }
        Expr::TypeApply { callee, args } => {
            normalize_expression(callee, names, members)?;
            for t in args {
                resolve_type(t, names)?
            }
        }
        Expr::Lambda(f) => normalize_function(f, names, members)?,
        Expr::Block(b) => normalize_block(b, names, members)?,
        Expr::Group(e) | Expr::Unary(_, e) | Expr::Unwrap(e) | Expr::Spread(e) | Expr::BoundReference(e, _) => {
            normalize_expression(e, names, members)?
        }
        Expr::Binary(_, a, b)
        | Expr::Index {
            receiver: a, index: b, ..
        } => {
            normalize_expression(a, names, members)?;
            normalize_expression(b, names, members)?
        }
        Expr::Member { receiver, .. } => normalize_expression(receiver, names, members)?,
        Expr::TypeOp { value, ty, .. } => {
            normalize_expression(value, names, members)?;
            resolve_type(ty, names)?
        }
        Expr::Call(c, args) | Expr::SafeCall(c, args) => {
            normalize_expression(c, names, members)?;
            for (_, e) in args {
                normalize_expression(e, names, members)?
            }
        }
        Expr::Collection { entries, .. } => {
            for (k, v) in entries {
                if let Some(k) = k {
                    normalize_expression(k, names, members)?
                }
                normalize_expression(v, names, members)?
            }
        }
        Expr::Format(parts) => {
            for part in parts {
                normalize_expression(&mut part.value, names, members)?
            }
        }
        Expr::Transfer { value: Some(e), .. } => normalize_expression(e, names, members)?,
        _ => {}
    }
    Ok(())
}

pub(super) fn normalize_function(f: &mut FunctionAst, names: &Scope, members: &BTreeSet<String>) -> Result<(), String> {
    let names = resolve_generic_parameters(&mut f.generics, names, &format!("{}@{}", f.name, f.line))?;
    let names = &names;

    for p in std::sync::Arc::make_mut(&mut f.params) {
        check_member_binding(&p.name, members)?;
        resolve_type(&mut p.ty, names)?;
        if let Some(e) = &mut p.default {
            normalize_expression(e, names, members)?
        }
    }
    if let Some(t) = &mut f.result {
        resolve_type(t, names)?
    }
    normalize_block(&mut f.body, names, members)
}

pub(super) fn normalize_block(b: &mut [Statement], names: &Scope, members: &BTreeSet<String>) -> Result<(), String> {
    let mut aliases = b
        .iter()
        .filter_map(|s| {
            if let StatementKind::Alias(a) = &s.kind {
                Some(a.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    // A declaration-free block reuses its parent's view. A declaring block
    // stores only its new aliases in a child frame.
    let mut names = if aliases.is_empty() {
        names.clone()
    } else {
        names.child()
    };
    register_type_aliases(&mut aliases, &mut names)?;
    let mut resolved_aliases = aliases.into_iter();
    for statement in b.iter_mut() {
        if let StatementKind::Alias(alias) = &mut statement.kind {
            *alias = resolved_aliases.next().expect("registered local alias");
        }
    }
    let names = &names;
    for s in b {
        match &mut s.kind {
            StatementKind::Variable { name, ty, value, .. } => {
                check_member_binding(name, members)?;
                if let Some(t) = ty {
                    resolve_type(t, names)?
                }
                normalize_expression(value, names, members)?
            }
            StatementKind::Uninitialized { name, ty, .. } => {
                check_member_binding(name, members)?;
                resolve_type(ty, names)?;
            }
            StatementKind::Assign { name, operator, value } => {
                normalize_expression(value, names, members)?;
                if members.contains(name) {
                    s.kind = StatementKind::Write {
                        target: Expr::Member {
                            receiver: Box::new(Expr::Name("this".into())),
                            name: name.clone(),
                            safe: false,
                        },
                        operator: operator.clone(),
                        value: value.clone(),
                    }
                }
            }
            StatementKind::Write { target, value, .. } => {
                normalize_expression(target, names, members)?;
                normalize_expression(value, names, members)?
            }
            StatementKind::Expression(e) | StatementKind::Text(e) | StatementKind::Yield { value: e, .. } => {
                normalize_expression(e, names, members)?
            }
            StatementKind::Return(Some(e)) => normalize_expression(e, names, members)?,
            StatementKind::If { branches, no } => {
                for branch in branches {
                    normalize_expression(&mut branch.condition, names, members)?;
                    normalize_block(&mut branch.body, names, members)?;
                }
                normalize_block(no, names, members)?
            }
            StatementKind::While { condition, body: b, .. } => {
                normalize_expression(condition, names, members)?;
                normalize_block(b, names, members)?
            }
            StatementKind::For {
                name,
                iterable,
                body: b,
                ..
            } => {
                check_member_binding(name, members)?;
                normalize_expression(iterable, names, members)?;
                normalize_block(b, names, members)?
            }
            StatementKind::Function(f) => {
                check_member_binding(&f.name, members)?;
                normalize_function(f, names, members)?;
            }
            StatementKind::Narrative(b) => normalize_block(b, names, members)?,
            StatementKind::Choice { options, fallback } => {
                for o in options {
                    for e in [&mut o.label, &mut o.visible, &mut o.enabled] {
                        normalize_expression(e, names, members)?
                    }
                    normalize_block(&mut o.body, names, members)?
                }
                if let Some(b) = fallback {
                    normalize_block(b, names, members)?
                }
            }
            StatementKind::When {
                subject,
                binding,
                annotation,
                cases,
            } => {
                if let Some(name) = binding {
                    check_member_binding(name, members)?;
                }
                if let Some(e) = subject {
                    normalize_expression(e, names, members)?
                }
                if let Some(t) = annotation {
                    resolve_type(t, names)?
                }
                for c in cases {
                    for p in &mut c.patterns {
                        match p {
                            Pattern::Value(e) | Pattern::Membership { value: e, .. } => {
                                normalize_expression(e, names, members)?
                            }
                            Pattern::Type { ty, bindings, .. } => {
                                resolve_type(ty, names)?;
                                if let Some(bindings) = bindings {
                                    for name in bindings.iter().filter(|name| name.as_str() != "_") {
                                        check_member_binding(name, members)?;
                                    }
                                }
                            }
                        }
                    }
                    if let Some(e) = &mut c.guard {
                        normalize_expression(e, names, members)?
                    }
                    normalize_block(&mut c.body, names, members)?
                }
            }
            _ => {}
        }
    }
    Ok(())
}
