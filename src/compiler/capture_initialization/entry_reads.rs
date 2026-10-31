//! Entry preconditions for captured cells that a completed helper overwrites.
//! Direct reads need the cell only until a dominating store. Opaque calls,
//! retained closures, and publication remain conservative barriers.

use super::Target;
use crate::compiler::bindings::{ScopeStack, lookup_ref, symbol_mut};
use crate::compiler::captures::capture_required_slot;
use crate::model::{Instruction, Op};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Defaults have their own call-site preconditions, selected only when omitted.
/// Their reads must not become body preconditions for supplied arguments. Keep
/// their actual branches and stores in the separate must-write analysis.
pub(super) fn body_start(code: &[Instruction]) -> Option<usize> {
    let mut start = 0;
    while let Some(Instruction {
        op: Op::Default { target, .. },
        ..
    }) = code.get(start)
    {
        if *target <= start || *target > code.len() {
            return None;
        }
        start = *target;
    }
    Some(start)
}

pub(super) fn record(op: &Op, stores: &BTreeMap<&str, usize>, written: &[u64], needed: &mut [u64]) {
    match op {
        Op::Load { name } => {
            if let Some(bit) = stores.get(name.as_str()) {
                needed[bit / 64] |= (1 << (bit % 64)) & !written[bit / 64];
            }
        }
        // A candidate store's RHS has already been evaluated. Calls and closure
        // creation in that evaluation have their own conservative barriers.
        Op::Store { name } if stores.contains_key(name.as_str()) => {}
        Op::FunctionValue {
            captures: Some(names), ..
        } if names.is_empty() => {}
        // Named closures are installed at block entry and retain binding
        // references. Creation reads no captured value and publishes nothing.
        // Later loads, calls, references, and stores keep their own barriers.
        Op::Closure { .. }
        | Op::Constant { .. }
        | Op::Declare { .. }
        | Op::Uninitialized { .. }
        | Op::Pop
        | Op::Dup
        | Op::Widen
        | Op::Convert { .. }
        | Op::Member { .. }
        | Op::Index
        | Op::AssertPresent
        | Op::TypeTest { .. }
        | Op::Cast { .. }
        | Op::BeginCollection { .. }
        | Op::CollectionAdd
        | Op::CheckKey
        | Op::CollectionPut
        | Op::AddressMember { .. }
        | Op::AddressIndex
        | Op::ReadAddress
        | Op::Range { .. }
        | Op::RangeStep
        | Op::IterStart { .. }
        | Op::IterNext { .. }
        | Op::IterEnd { .. }
        | Op::StackMark { .. }
        | Op::StackReset { .. }
        | Op::StackEnd { .. }
        | Op::Unwind { .. }
        | Op::Unused { .. }
        | Op::Unary { .. }
        | Op::Binary { .. }
        | Op::Enter
        | Op::Exit
        | Op::Specialize { .. }
        | Op::Format { .. }
        | Op::Print
        | Op::Construct { .. }
        | Op::ProseUnwind { .. }
        | Op::ProseStart
        | Op::ProseText
        | Op::ProseFlush
        | Op::ProseEnd
        // Waiting itself reads no captured value. Preparation expressions and
        // resumed option bodies have their own loads, calls, and stores.
        | Op::BeginChoice
        | Op::Offer { .. }
        | Op::Choose { .. }
        | Op::Return
        | Op::Unreachable
        | Op::Jump { .. }
        | Op::JumpFalse { .. }
        | Op::JumpTrue { .. }
        | Op::JumpNull { .. }
        | Op::Default { .. } => {}
        // A new instruction is conservative until reviewed here. Invoked code
        // or published values may observe any cell not yet written on this path.
        _ => {
            for (needed, written) in needed.iter_mut().zip(written) {
                *needed |= !written;
            }
        }
    }
}

pub(in crate::compiler) fn refine(name: &str, id: &str, targets: &[Target], env: &mut ScopeStack) {
    let capture = &lookup_ref(env, name).unwrap().callable_requirements[id];
    let ready = targets
        .iter()
        .filter(|target| target.reads_after_write)
        .map(|target| target.identity)
        .collect::<BTreeSet<_>>();
    if ready.is_empty() {
        return;
    }
    let removed = capture
        .body
        .iter()
        .filter(|required| {
            required.reads_binding()
                && capture_required_slot(env, required).is_some_and(|(_, _, binding)| ready.contains(&binding.identity))
        })
        .cloned()
        .collect::<Vec<_>>();
    if removed.is_empty() {
        return;
    }
    let binding = symbol_mut(env, name).unwrap();
    let capture = Arc::make_mut(binding.callable_requirements.get_mut(id).unwrap());
    for required in removed {
        capture.body.remove(&required);
    }
    binding.requires = binding
        .callable_requirements
        .values()
        .flat_map(|capture| capture.all_requirements())
        .collect();
}
