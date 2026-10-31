//! Remove unreachable lowered instructions after source checking is complete.
//!
//! Offers retain their deferred selection destinations, not just immediate jumps.
//! Calls and scene transfers conservatively retain their following instructions.

use crate::model::{Instruction, Op};
use std::time::Instant;

pub(super) fn prune(code: &mut Vec<Instruction>, started: Instant) -> Result<(), String> {
    crate::budget(started)?;
    if code.is_empty() {
        return Ok(());
    }
    // Small expression bodies dominate large routine tables. Avoid allocating a
    // graph and address map when no instruction can enter the trailing code.
    if !code
        .iter()
        .any(|instruction| instruction.op.instruction_target().is_some())
    {
        if let Some(end) = code.iter().position(|instruction| terminates(&instruction.op)) {
            code.truncate(end + 1);
        }
        return crate::budget(started);
    }

    for (pc, instruction) in code.iter().enumerate() {
        if pc % 1024 == 0 {
            crate::budget(started)?;
        }
        if instruction
            .op
            .instruction_target()
            .is_some_and(|target| target >= code.len())
        {
            return Err("invalid instruction target during code pruning".into());
        }
    }
    let mut reachable = vec![false; code.len()];
    reachable[0] = true;
    let mut pending = vec![0];
    let mut visited = 0;
    while let Some(pc) = pending.pop() {
        if visited % 1024 == 0 {
            crate::budget(started)?;
        }
        visited += 1;
        let op = &code[pc].op;
        let next = (!terminates(op) && !matches!(op, Op::Jump { .. }) && pc + 1 < code.len()).then_some(pc + 1);
        for target in [op.instruction_target(), next].into_iter().flatten() {
            if !reachable[target] {
                reachable[target] = true;
                pending.push(target);
            }
        }
    }
    if visited == code.len() {
        return crate::budget(started);
    }

    // Each function owns its bounded scratch data. Compact the existing vector
    // in place so retaining instructions does not duplicate their payloads.
    let mut addresses = vec![usize::MAX; code.len()];
    let mut retained = 0;
    for (pc, live) in reachable.iter().enumerate() {
        if *live {
            addresses[pc] = retained;
            retained += 1;
        }
    }
    let mut pc = 0;
    code.retain_mut(|instruction| {
        let live = reachable[pc];
        pc += 1;
        if live && let Some(target) = instruction.op.instruction_target_mut() {
            debug_assert_ne!(addresses[*target], usize::MAX);
            *target = addresses[*target];
        }
        live
    });
    crate::budget(started)
}

fn terminates(op: &Op) -> bool {
    matches!(op, Op::Return | Op::Unreachable)
}

#[cfg(test)]
mod tests;
