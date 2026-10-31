//! Inventory calls that execute in the current expression, keeping deferred code opaque.

use crate::syntax::{Expr, Pattern, Statement, StatementKind};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Inventory<'a> {
    pub calls: Vec<Call<'a>>,
    pub referenced: BTreeSet<String>,
    loop_condition: bool,
}

pub(super) struct Call<'a> {
    pub name: &'a str,
    pub compact: bool,
    pub loop_condition: bool,
}

impl<'a> Inventory<'a> {
    pub fn statement(statement: &'a Statement) -> Self {
        let mut inventory = Self::default();
        inventory.visit_statement(statement);
        inventory
    }

    fn body(&mut self, body: &'a [Statement]) {
        for statement in body {
            self.visit_statement(statement);
        }
    }

    fn visit_statement(&mut self, statement: &'a Statement) {
        match &statement.kind {
            StatementKind::Expression(value)
            | StatementKind::Variable { value, .. }
            | StatementKind::Return(Some(value))
            | StatementKind::Text(value)
            | StatementKind::Yield { value, .. } => self.expression(value),
            StatementKind::Assign { name, value, .. } => {
                self.referenced.insert(name.clone());
                self.expression(value);
            }
            StatementKind::Write { target, value, .. } => {
                self.expression(target);
                self.expression(value);
            }
            StatementKind::Narrative(body) => self.body(body),
            StatementKind::If { branches, no } => {
                for branch in branches {
                    self.expression(&branch.condition);
                    self.body(&branch.body);
                }
                self.body(no);
            }
            StatementKind::When { subject, cases, .. } => {
                if let Some(subject) = subject {
                    self.expression(subject);
                }
                for case in cases {
                    for pattern in &case.patterns {
                        match pattern {
                            Pattern::Value(value) | Pattern::Membership { value, .. } => self.expression(value),
                            Pattern::Type { .. } => {}
                        }
                    }
                    if let Some(guard) = &case.guard {
                        self.expression(guard);
                    }
                    self.body(&case.body);
                }
            }
            StatementKind::Choice { options, fallback } => {
                for option in options {
                    self.expression(&option.visible);
                    self.expression(&option.enabled);
                    self.expression(&option.label);
                    self.body(&option.body);
                }
                if let Some(body) = fallback {
                    self.body(body);
                }
            }
            // Loop-body flow checks validate writes on every backedge.
            StatementKind::For { iterable, body, .. } => {
                self.expression(iterable);
                self.body(body);
            }
            StatementKind::While {
                condition, body, post, ..
            } => {
                // Condition calls require a separate proof: the enclosing
                // loop's entry snapshot already includes the first evaluation.
                let previous = self.loop_condition;
                if !post {
                    self.loop_condition = true;
                    self.expression(condition);
                }
                self.loop_condition = previous;
                self.body(body);
                if *post {
                    self.loop_condition = true;
                    self.expression(condition);
                }
                self.loop_condition = previous;
            }
            // Deferred function bodies remain opaque; retaining the callable
            // can expose captures beyond the current invocation's flow proof.
            StatementKind::Function(_) => {
                self.referenced
                    .extend(super::super::captures::statement_names(statement));
            }
            StatementKind::Alias(_)
            | StatementKind::Uninitialized { .. }
            | StatementKind::ParagraphBreak
            | StatementKind::Return(None) => {}
        }
    }

    fn expression(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Call(callee, arguments) => {
                if let Expr::Name(name) = super::super::overloads::callable_base(callee) {
                    self.calls.push(Call {
                        name,
                        compact: super::plain_call_name(callee, arguments).is_some(),
                        loop_condition: self.loop_condition,
                    });
                } else {
                    self.expression(callee);
                }
                for (_, argument) in arguments {
                    self.expression(argument);
                }
            }
            Expr::Name(name) | Expr::Reference(name) => {
                self.referenced.insert(name.clone());
            }
            Expr::Group(value)
            | Expr::Unary(_, value)
            | Expr::Unwrap(value)
            | Expr::Spread(value)
            | Expr::TypeOp { value, .. }
            | Expr::TypeApply { callee: value, .. }
            | Expr::BoundReference(value, _)
            | Expr::Member { receiver: value, .. } => self.expression(value),
            Expr::Index { receiver, index, .. } | Expr::Binary(_, receiver, index) => {
                self.expression(receiver);
                self.expression(index);
            }
            Expr::SafeCall(callee, arguments) => {
                self.expression(callee);
                for (_, argument) in arguments {
                    self.expression(argument);
                }
            }
            Expr::Construct { fields, .. } => {
                for (_, value) in fields {
                    self.expression(value);
                }
            }
            Expr::Collection { entries, .. } => {
                for (key, value) in entries {
                    if let Some(key) = key {
                        self.expression(key);
                    }
                    self.expression(value);
                }
            }
            Expr::Format(parts) => {
                for part in parts {
                    self.expression(&part.value);
                }
            }
            Expr::Transfer { value, .. } => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
            Expr::Block(body) => self.body(body),
            // Lambda bodies may escape or run repeatedly. Their free names
            // forbid this optional proof rather than becoming eager calls.
            Expr::Lambda(_) => {
                self.referenced
                    .extend(super::super::captures::expression_names(expression));
            }
            Expr::Int(_) | Expr::Float(_) | Expr::Boolean(_) | Expr::String(_) | Expr::Char(_) | Expr::Null => {}
        }
    }
}
