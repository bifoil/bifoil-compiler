//! Result inference follows normal condition exits and recorded loop transfers.
//! Scope joins share the lowering pass's rules; this pass emits no instructions.

use super::super::constants::constant_boolean;
use super::super::flow::FlowJoin;
use super::super::{Binding, ScopeStack, infer, insert, invalidate_narrowing, merge_env, narrow_condition};
use super::{Inference, block, expression_returns};
use crate::syntax::{Expr, Statement};
use std::sync::Arc;

pub(super) struct Loop {
    label: Option<String>,
    base: usize,
    keep: usize,
    entry: ScopeStack,
    breaks: FlowJoin,
    continues: Vec<ScopeStack>,
}

impl Inference {
    fn begin_loop(&mut self, label: &Option<String>, entry: &ScopeStack, keep: usize) -> Result<(), String> {
        if label.is_some() && self.loops.iter().any(|state| state.label == *label) {
            return Err("duplicate enclosing loop label".into());
        }
        self.loops.push(Loop {
            label: label.clone(),
            base: entry.len(),
            keep,
            entry: entry.clone(),
            breaks: FlowJoin::default(),
            continues: vec![],
        });
        Ok(())
    }

    pub(super) fn loop_transfer(&mut self, kind: &str, label: &Option<String>, env: &ScopeStack) -> Result<(), String> {
        let index = if label.is_some() {
            self.loops.iter().rposition(|state| state.label == *label)
        } else {
            self.loops.len().checked_sub(1)
        };
        let Some(index) = index else {
            // A value-expression probe has no surrounding loop stack. Its
            // enclosing analysis/lowering resolves and validates the transfer.
            if self.external_loop_transfers {
                return Ok(());
            }
            return Err("jump requires a matching enclosing loop in this function".into());
        };
        let state = &mut self.loops[index];
        let mut exit = env.clone();
        exit.truncate(state.base + if kind == "continue" { state.keep } else { 0 });
        if kind == "continue" {
            state.continues.push(exit);
        } else {
            state.breaks.add(&state.entry, exit);
        }
        Ok(())
    }
}

pub(super) fn for_statement(
    label: &Option<String>,
    name: &str,
    iterable: &Expr,
    body: &[Statement],
    env: &mut ScopeStack,
    analysis: &mut Inference,
) -> Result<bool, String> {
    let iterable_type = infer(iterable, env)?;
    let item = iterable_type.item_type().ok_or("for requires an iterable")?;
    if !expression_returns(iterable, env, analysis)? {
        return Ok(false);
    }
    let guaranteed =
        super::super::loops::nonempty::iterable(iterable, &iterable_type, env, &analysis.immutable_iterables);
    invalidate_narrowing(env);
    let before = env.clone();
    let mut scope = before.clone();
    scope.push(Arc::default());
    insert(
        &mut scope,
        name,
        Binding {
            identity: crate::compiler::bindings::Identity::new(),
            index_key: false,
            global_aliases: None,
            volatile: false,
            implicit_receiver: false,
            escaped: false,
            runtime: name.into(),
            ty: item,
            mutable: false,
            initialized: true,
            may_be_initialized: true,
            owner: String::new(),
            narrow: None,
            field_narrows: None,
            callables: vec![],
            requires: Default::default(),
            callable_requirements: Default::default(),
        },
    )?;
    analysis.begin_loop(label, &before, 0)?;
    let repeat = block(body, &mut scope, analysis)?;
    let state = analysis.loops.pop().unwrap();
    let mut exits = state.breaks;
    if !guaranteed {
        exits.add(&before, before.clone());
    }
    if repeat {
        scope.truncate(before.len());
        exits.add(&before, scope);
    }
    for mut exit in state.continues {
        exit.truncate(before.len());
        exits.add(&before, exit);
    }
    let reaches = !exits.is_empty();
    *env = exits.finish(&before);
    Ok(reaches)
}

pub(super) fn while_statement(
    label: &Option<String>,
    condition: &Expr,
    body: &[Statement],
    post: bool,
    env: &mut ScopeStack,
    analysis: &mut Inference,
) -> Result<bool, String> {
    let before = env.clone();
    invalidate_narrowing(env);
    let known = constant_boolean(condition);
    let mut condition_false = None;
    if !post {
        if !expression_returns(condition, env, analysis)? {
            return Ok(false);
        }
        let mut exit = env.clone();
        narrow_condition(condition, &mut exit, false);
        condition_false = Some(exit);
        narrow_condition(condition, env, true);
    }
    let mut scope = env.clone();
    scope.push(Arc::default());
    analysis.begin_loop(label, &before, usize::from(post))?;
    let body_reaches = (post || known != Some(false)) && block(body, &mut scope, analysis)?;
    if post {
        let mut condition_exits = analysis.loops.last().unwrap().continues.clone();
        if body_reaches {
            condition_exits.push(scope.clone());
        }
        if !condition_exits.is_empty() {
            scope = merge_env(&scope, &condition_exits);
            if expression_returns(condition, &mut scope, analysis)? {
                narrow_condition(condition, &mut scope, false);
                scope.truncate(before.len());
                condition_false = Some(scope);
            }
        }
    }
    let mut exits = analysis.loops.pop().unwrap().breaks;
    if known != Some(true)
        && let Some(exit) = condition_false
    {
        exits.add(&before, exit);
    }
    let reaches = !exits.is_empty();
    *env = exits.finish(&before);
    Ok(reaches)
}
