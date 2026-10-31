//! Loop targets and scope/prose cleanup for break and continue.

use super::flow::FlowJoin;
use super::{
    Binding, Compiler, FunctionContext, LoopState, check_loop_initialization, insert, merge_env, narrow_condition,
};
use crate::model::{Op, Type};
use crate::syntax::{Expr, Statement};
use std::sync::Arc;

pub(super) mod nonempty;

struct LoopHeader {
    label: Option<String>,
    base: usize,
    keep: usize,
    iterator: Option<usize>,
    continue_at: Option<usize>,
    stack_mark: usize,
}

impl Compiler {
    pub(super) fn loop_transfer(
        &mut self,
        kind: &str,
        label: &Option<String>,
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        let index = if let Some(label) = label {
            context.loop_stack.iter().rposition(|l| l.label.as_ref() == Some(label))
        } else {
            context.loop_stack.len().checked_sub(1)
        }
        .ok_or("jump requires a matching enclosing loop in this function")?;
        let iterators = context.loop_stack[index + 1..]
            .iter()
            .filter_map(|l| l.iterator)
            .collect::<Vec<_>>();
        for id in iterators {
            context.emit(Op::IterEnd { id }, line);
        }
        context.emit(
            Op::StackReset {
                id: context.loop_stack[index].stack_mark,
            },
            line,
        );
        context.emit(
            Op::ProseUnwind {
                depth: context.loop_stack[index].prose_depth,
            },
            line,
        );
        let base = context.loop_stack[index].base;
        let keep = if kind == "continue" {
            context.loop_stack[index].keep
        } else {
            0
        };
        context.emit(
            Op::Unwind {
                count: context.scopes.len() - base - keep,
            },
            line,
        );
        let target = if kind == "continue" {
            context.loop_stack[index].continue_at.unwrap_or(0)
        } else {
            0
        };
        let jump = context.emit(Op::Jump { target }, line);
        let exit = if context.is_reachable {
            Some(super::captures::leaving_scopes(&context.scopes, base + keep)?)
        } else {
            None
        };
        let state = &mut context.loop_stack[index];
        if kind == "break" {
            state.breaks.push(jump);
            if let Some(exit) = exit {
                state.break_envs.add(&state.entry, exit);
            }
        } else {
            state.continues.push(jump);
            if let Some(exit) = exit {
                state.continue_envs.push(exit);
            }
        }
        context.is_reachable = false;
        Ok(())
    }

    fn check_loop_captures(
        &mut self,
        before: &super::ScopeStack,
        after: &super::ScopeStack,
        repeat: bool,
        continues: &[super::ScopeStack],
        context: &FunctionContext,
    ) -> Result<(), String> {
        for edge in continues.iter().chain(repeat.then_some(after)) {
            for scope in before {
                for (name, previous) in scope.iter() {
                    if let Some(binding) = super::lookup(edge, name)
                        && binding.requires != previous.requires
                    {
                        super::captures::check_requirements(
                            &binding.requires,
                            edge,
                            &context.function_id,
                            &mut self.empty_dependencies,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn push_loop(
        &self,
        header: LoopHeader,
        entry: &super::ScopeStack,
        context: &mut FunctionContext,
    ) -> Result<(), String> {
        if header.label.is_some() && context.loop_stack.iter().any(|state| state.label == header.label) {
            return Err("duplicate enclosing loop label".into());
        }
        context.loop_stack.push(LoopState {
            prose_depth: context.prose_depth,
            stack_mark: header.stack_mark,
            label: header.label,
            base: header.base,
            keep: header.keep,
            iterator: header.iterator,
            continue_at: header.continue_at,
            breaks: vec![],
            continues: vec![],
            entry: entry.clone(),
            break_envs: FlowJoin::default(),
            continue_envs: vec![],
        });
        Ok(())
    }
}

impl Compiler {
    pub(super) fn for_loop(
        &mut self,
        label: &Option<String>,
        name: &str,
        iterable: &Expr,
        body: &[Statement],
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        let iterable_type = self.expression(iterable, context, line)?;
        let item = iterable_type.item_type().ok_or("for requires an iterable")?;
        let guaranteed = nonempty::iterable(iterable, &iterable_type, &context.scopes, &self.immutable_iterables);
        let item_requirements = context.value_requirements.clone();
        // The body may revisit this point after changing a captured var.
        // Fresh checks inside the body can establish new facts each iteration.
        super::invalidate_narrowing(&mut context.scopes);
        let before = context.scopes.clone();
        let incoming = context.is_reachable;
        let id = context.next_iterator_id;
        context.next_iterator_id += 1;
        context.emit(Op::IterStart { id }, line);
        context.emit(Op::StackMark { id }, line);
        let head = context.instructions.len();
        context.emit(Op::Enter, line);
        context.scopes.push(Arc::default());
        insert(
            &mut context.scopes,
            name,
            Binding {
                identity: crate::compiler::bindings::Identity::new(),
                index_key: false,
                global_aliases: None,
                volatile: false,
                implicit_receiver: false,
                escaped: false,
                runtime: name.to_owned(),
                ty: item.clone(),
                mutable: false,
                initialized: true,
                may_be_initialized: true,
                owner: context.function_id.clone(),
                narrow: None,
                field_narrows: None,
                callables: vec![],
                requires: item_requirements,
                callable_requirements: Default::default(),
            },
        )?;
        let next = context.emit(
            Op::IterNext {
                id,
                name: name.to_owned(),
                ty: item,
                target: 0,
            },
            line,
        );
        self.push_loop(
            LoopHeader {
                label: label.clone(),
                base: before.len(),
                keep: 0,
                iterator: Some(id),
                continue_at: Some(head),
                stack_mark: id,
            },
            &before,
            context,
        )?;
        self.body(body, context)?;
        let repeat = context.is_reachable;
        let after = if context.is_reachable {
            super::captures::leaving_scopes(&context.scopes, before.len())?
        } else {
            context.scopes.clone()
        };
        let loop_state = context.loop_stack.pop().unwrap();
        check_loop_initialization(&before, &after, repeat, &loop_state.continue_envs)?;
        self.check_loop_captures(&before, &after, repeat, &loop_state.continue_envs, context)?;
        context.exit_scope()?;
        context.emit(Op::Exit, line);
        context.emit(Op::Jump { target: head }, line);
        context.patch(next, context.instructions.len());
        context.emit(Op::Exit, line);
        let end = context.instructions.len();
        context.emit(Op::IterEnd { id }, line);
        context.emit(Op::StackEnd { id }, line);
        for jump in &loop_state.breaks {
            context.patch(*jump, end)
        }
        for jump in &loop_state.continues {
            context.patch(*jump, head)
        }
        let mut exits = loop_state.break_envs;
        // A proven nonempty iterable must reach a body or continue edge.
        // Other iterables may skip their body entirely.
        if !guaranteed {
            exits.add(&before, before.clone());
        }
        if repeat {
            exits.add(&before, after);
        }
        for exit in loop_state.continue_envs {
            exits.add(&before, exit);
        }
        context.is_reachable = incoming && !exits.is_empty();
        context.scopes = exits.finish(&before);

        Ok(())
    }

    pub(super) fn while_loop(
        &mut self,
        label: &Option<String>,
        condition: &Expr,
        body: &[Statement],
        post: &bool,
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        let before = context.scopes.clone();
        let incoming = context.is_reachable;
        let stack_mark = context.next_iterator_id;
        context.next_iterator_id += 1;
        context.emit(Op::StackMark { id: stack_mark }, line);
        let head = context.instructions.len();
        let mut false_jump = None;
        let mut condition_false = None;
        super::invalidate_narrowing(&mut context.scopes);
        if !post {
            if !Type::Boolean.accepts(&self.expression(condition, context, line)?) {
                return Err("while requires Boolean".into());
            }
            false_jump = Some(context.emit(Op::JumpFalse { target: 0 }, line));
            let mut exit = context.scopes.clone();
            narrow_condition(condition, &mut exit, false);
            condition_false = context.is_reachable.then_some(exit);
            narrow_condition(condition, &mut context.scopes, true);
            context.is_reachable &= super::constants::constant_boolean(condition) != Some(false);
        }
        context.emit(Op::Enter, line);
        context.scopes.push(Arc::default());
        self.push_loop(
            LoopHeader {
                label: label.clone(),
                base: before.len(),
                keep: usize::from(*post),
                iterator: None,
                continue_at: if *post { None } else { Some(head) },
                stack_mark,
            },
            &before,
            context,
        )?;
        self.body_with_tail(body, context, (*post).then_some(condition))?;
        let body_reachable = context.is_reachable;
        let continue_envs = context.loop_stack.last().unwrap().continue_envs.clone();
        let mut condition_exits = continue_envs.clone();
        if body_reachable {
            condition_exits.push(context.scopes.clone());
        }
        let condition_at = if *post {
            let at = context.instructions.len();
            context.scopes = merge_env(&context.scopes, &condition_exits);
            context.is_reachable = !condition_exits.is_empty();
            if !Type::Boolean.accepts(&self.expression(condition, context, line)?) {
                return Err("do/while requires Boolean".into());
            }
            at
        } else {
            head
        };
        let condition_reachable = context.is_reachable;
        let after = if context.is_reachable {
            super::captures::leaving_scopes(&context.scopes, before.len())?
        } else {
            context.scopes.clone()
        };
        let state = context.loop_stack.pop().unwrap();
        if super::constants::constant_boolean(condition) != Some(false) {
            check_loop_initialization(&before, &after, body_reachable, &continue_envs)?;
            self.check_loop_captures(&before, &after, body_reachable, &continue_envs, context)?;
        }
        context.exit_scope()?;
        context.emit(Op::Exit, line);
        if *post {
            context.emit(Op::JumpTrue { target: head }, line);
        } else {
            context.emit(Op::Jump { target: head }, line);
        }
        let end = context.instructions.len();
        context.emit(Op::StackEnd { id: stack_mark }, line);
        if let Some(jump) = false_jump {
            context.patch(jump, end)
        }
        for jump in state.breaks {
            context.patch(jump, end)
        }
        for jump in state.continues {
            context.patch(jump, condition_at)
        }
        let always = super::constants::constant_boolean(condition) == Some(true);
        let mut exits = state.break_envs;
        if !always {
            if *post {
                if condition_reachable {
                    let mut exit = after;
                    narrow_condition(condition, &mut exit, false);
                    exits.add(&before, exit);
                }
            } else if let Some(exit) = condition_false {
                exits.add(&before, exit);
            }
        }
        context.is_reachable = incoming && !exits.is_empty();
        context.scopes = exits.finish(&before);

        Ok(())
    }
}
