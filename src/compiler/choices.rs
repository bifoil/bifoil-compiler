//! Choice preparation, option snapshots, and selected branches.

mod preparation;

use super::constants::constant_boolean;
use super::flow::{FlowJoin, SkippedPath, narrow_condition, narrow_stable_condition};
use super::{Compiler, FunctionContext};
use crate::model::{Op, Type};
use crate::syntax::{OptionAst, Statement};
use std::sync::Arc;

impl Compiler {
    pub(super) fn choice(
        &mut self,
        options: &[OptionAst],
        fallback: &Option<Vec<Statement>>,
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        if !context.may_suspend {
            return Err("choice requires a suspend context".into());
        }
        if fallback.is_none()
            && options
                .iter()
                .all(|o| constant_boolean(&o.visible) == Some(false) || constant_boolean(&o.enabled) == Some(false))
        {
            return Err("choice has no selectable options".into());
        }
        context.emit(Op::BeginChoice, line);
        let mut offers = vec![];
        let preparation_entry = context.scopes.clone();
        let mut exits = vec![];
        let permitted = context.may_suspend;
        context.may_suspend = false;
        for option in options {
            let consumed = option.once.then(|| SkippedPath::new(context));
            let history = if option.once {
                context.emit(
                    Op::Unused {
                        site: format!("{}:{}:{}", context.source_file, context.function_id, option.line),
                    },
                    line,
                );
                Some(context.emit(Op::JumpFalse { target: 0 }, line))
            } else {
                None
            };
            if !Type::Boolean.accepts(&self.expression(&option.visible, context, option.visible_line)?) {
                return Err("option visibility requires Boolean".into());
            }
            let hidden = context.emit(Op::JumpFalse { target: 0 }, line);
            let visibility = constant_boolean(&option.visible);
            let skipped = (visibility != Some(true)).then(|| SkippedPath::new(context));
            context.is_reachable &= visibility != Some(false);
            narrow_condition(&option.visible, &mut context.scopes, true);
            if !Type::Boolean.accepts(&self.expression(&option.enabled, context, option.enabled_line)?) {
                return Err("enabledIf requires Boolean".into());
            }
            self.expression(&option.label, context, option.line)?;
            let initialized = preparation::Initialized::capture(&preparation_entry, &context.scopes);
            offers.push((
                context.emit(
                    Op::Offer {
                        target: 0,
                        once: option
                            .once
                            .then(|| format!("{}:{}:{}", context.source_file, context.function_id, option.line)),
                    },
                    line,
                ),
                initialized,
            ));
            context.patch(hidden, context.instructions.len());
            if let Some(skipped) = skipped {
                skipped.merge(context);
            }
            if let Some(jump) = history {
                context.patch(jump, context.instructions.len())
            }
            if let Some(consumed) = consumed {
                consumed.merge(context);
            }
        }
        context.may_suspend = permitted;
        let choose = context.emit(Op::Choose { fallback: None }, line);
        let base = context.scopes.clone();
        let incoming = context.is_reachable;
        let mut paths = FlowJoin::default();
        for (option, (offer, initialized)) in options.iter().zip(offers) {
            context.patch(offer, context.instructions.len());
            context.scopes = base.clone();
            context.is_reachable = incoming
                && constant_boolean(&option.visible) != Some(false)
                && constant_boolean(&option.enabled) != Some(false);
            initialized.apply(&mut context.scopes);
            narrow_stable_condition(&option.visible, &mut context.scopes);
            narrow_stable_condition(&option.enabled, &mut context.scopes);
            context.emit(Op::Enter, line);
            context.scopes.push(Arc::default());
            self.body(&option.body, context)?;
            context.exit_scope()?;
            context.emit(Op::Exit, line);
            if context.is_reachable {
                paths.add(&base, context.scopes.clone())
            }
            exits.push(context.emit(Op::Jump { target: 0 }, line));
        }
        if let Some(fallback) = fallback {
            context.instructions[choose].op = Op::Choose {
                fallback: Some(context.instructions.len()),
            };
            context.scopes = base.clone();
            let always_selectable = options.iter().any(|option| {
                !option.once
                    && constant_boolean(&option.visible) == Some(true)
                    && constant_boolean(&option.enabled) == Some(true)
            });
            context.is_reachable = incoming && !always_selectable;
            context.emit(Op::Enter, line);
            context.scopes.push(Arc::default());
            self.body(fallback, context)?;
            context.exit_scope()?;
            context.emit(Op::Exit, line);
            if context.is_reachable {
                paths.add(&base, context.scopes.clone())
            }
        }
        let end = context.instructions.len();
        for jump in exits {
            context.patch(jump, end)
        }
        context.is_reachable = incoming && !paths.is_empty();
        context.scopes = paths.finish(&base);
        Ok(())
    }
}
