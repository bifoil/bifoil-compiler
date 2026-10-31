//! Per-function instruction state, source spans, jumps, and resource checks.

use super::{Compiler, ScopeStack};
use crate::model::{Instruction, MAX_FUNCTIONS, MAX_INSTRUCTIONS, Op, Span, Type};
use std::sync::Arc;

pub(super) struct LoopState {
    pub(super) stack_mark: usize,
    pub(super) prose_depth: usize,
    pub(super) label: Option<String>,
    pub(super) base: usize,
    pub(super) keep: usize,
    pub(super) iterator: Option<usize>,
    pub(super) breaks: Vec<usize>,
    pub(super) continues: Vec<usize>,
    pub(super) continue_at: Option<usize>,
    pub(super) entry: ScopeStack,
    pub(super) break_envs: super::flow::FlowJoin,
    pub(super) continue_envs: Vec<ScopeStack>,
}
pub(super) struct FunctionContext {
    pub(super) scopes: ScopeStack,
    pub(super) instructions: Vec<Instruction>,
    pub(super) source_file: Arc<str>,
    pub(super) function_id: String,
    pub(super) return_type: Type,
    pub(super) may_suspend: bool,
    pub(super) loop_stack: Vec<LoopState>,
    pub(super) next_iterator_id: usize,
    pub(super) is_reachable: bool,
    pub(super) prose_depth: usize,
    // Requirements belong to the last evaluated value, including closures
    // nested inside collections. Calls and escaping values must discharge them.
    pub(super) value_requirements: super::captures::Requirements,
    pub(super) value_scopes: Vec<(usize, super::captures::Requirements)>,
}
impl FunctionContext {
    pub(super) fn exit_scope(&mut self) -> Result<(), String> {
        if self.is_reachable {
            self.scopes = super::captures::leaving_scopes(&self.scopes, self.scopes.len() - 1)?;
        } else {
            self.scopes.pop();
        }
        Ok(())
    }

    pub(super) fn emit(&mut self, op: Op, line: usize) -> usize {
        let pos = self.instructions.len();
        self.instructions.push(Instruction {
            op,
            span: Span {
                file: self.source_file.clone(),
                line,
            },
        });
        pos
    }
    pub(super) fn patch(&mut self, pos: usize, target: usize) {
        *self.instructions[pos]
            .op
            .instruction_target_mut()
            .expect("patched instruction must have a local target") = target;
    }
    pub(super) fn convert(&mut self, actual: &Type, expected: &Type, line: usize) -> Result<(), String> {
        crate::declarations::validate_type(expected)?;

        if !expected.accepts(actual) {
            return Err(format!("expected {expected}, found {actual}"));
        }
        if actual != expected && !actual.subtype_of(expected) {
            self.emit(Op::Convert { ty: expected.clone() }, line);
        }
        Ok(())
    }
}

impl Compiler {
    pub(super) fn check_budget(&self, ctx: &FunctionContext) -> Result<(), String> {
        crate::budget(self.started)?;
        if ctx.instructions.len() > MAX_INSTRUCTIONS || self.functions.len() > MAX_FUNCTIONS {
            return Err("compilation complexity budget exceeded".into());
        }
        Ok(())
    }
}
