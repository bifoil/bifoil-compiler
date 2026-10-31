//! Definite captured-cell writes on every normal return of a known local helper.
//! Captured stores publish ready values; call/argument guards protect their retained
//! cells. A proven overwrite can therefore retire the target's old value requirements.

mod entry_reads;
pub(super) use entry_reads::refine as refine_entry_reads;

use super::bindings::CallableRequirements;
use super::captures::{Requirement, capture_required_slot};
use super::{FunctionContext, ScopeStack};
use crate::model::{Instruction, Op};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

const MAX_ANALYSIS_WORDS: usize = 1_000_000;
const MAX_FLOW_STEPS: usize = 200_000;

pub(super) struct Target {
    reads_after_write: bool,
    single_use: bool,
    scope: usize,
    identity: super::bindings::Identity,
    name: String,
    runtime: String,
    owner: Arc<str>,
}

impl Target {
    pub(super) fn single_use_name(&self) -> Option<&str> {
        self.single_use.then_some(self.name.as_str())
    }
}

pub(super) fn candidates(
    capture: &CallableRequirements,
    env: &ScopeStack,
    owner: &str,
    single_use: Option<&super::single_use_initializers::Plan>,
) -> Vec<Target> {
    let retained_owner: Arc<str> = Arc::from(owner);
    capture
        .body
        .iter()
        .filter_map(|required| {
            if !matches!(required.unscoped(), Requirement::Write(_)) {
                return None;
            }
            let (scope, name, binding) = capture_required_slot(env, required)?;
            let single_use = !binding.mutable
                && single_use.is_some_and(|plan| !plan.expands() && plan.permits(name, &binding.owner));
            ((binding.mutable || single_use) && binding.owner == owner).then(|| Target {
                reads_after_write: false,
                single_use,
                scope,
                identity: binding.identity,
                name: name.into(),
                runtime: binding.runtime.clone(),
                owner: Arc::clone(&retained_owner),
            })
        })
        .collect()
}

/// A bounded must-analysis intersects facts at joins and normal returns. When
/// the optional proof exceeds its own budget, ordinary conservative checks remain.
pub(super) fn analyze(code: &[Instruction], targets: Vec<Target>, start: Instant) -> Result<Vec<Target>, String> {
    if targets.is_empty() || code.is_empty() {
        return Ok(vec![]);
    }
    let words = targets.len().div_ceil(64);
    let stores = targets
        .iter()
        .enumerate()
        .map(|(index, target)| (target.runtime.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    if let Some(facts) = linear_writes(code, &stores, words, start)? {
        return Ok(proven_targets(targets, Some(facts.written), &facts.entry_reads));
    }
    let Some(offers) = choice_offers(code, start)? else {
        return Ok(vec![]);
    };
    let Some(blocks) = blocks(code, &stores, words, &offers, start)? else {
        return Ok(vec![]);
    };
    let mut incoming = vec![None::<Vec<u64>>; blocks.len()];
    incoming[0] = Some(vec![0; words]);
    let mut pending = VecDeque::from([0]);
    let mut queued = vec![false; blocks.len()];
    queued[0] = true;
    let mut steps = 0;
    while let Some(index) = pending.pop_front() {
        steps += 1;
        if steps > MAX_FLOW_STEPS {
            return Ok(vec![]);
        }
        if steps % 256 == 0 {
            crate::budget(start)?;
        }
        queued[index] = false;
        let mut written = incoming[index].as_ref().unwrap().clone();
        for (current, generated) in written.iter_mut().zip(&blocks[index].written) {
            *current |= generated;
        }
        for &target in &blocks[index].next {
            let changed = if let Some(previous) = &mut incoming[target] {
                let mut changed = false;
                for (old, new) in previous.iter_mut().zip(&written) {
                    let intersection = *old & new;
                    changed |= intersection != *old;
                    *old = intersection;
                }
                changed
            } else {
                incoming[target] = Some(written.clone());
                true
            };
            if changed && !queued[target] {
                pending.push_back(target);
                queued[target] = true;
            }
        }
    }
    let mut returned: Option<Vec<u64>> = None;
    let mut needed = vec![0; words];
    for (block, incoming) in blocks.iter().zip(incoming) {
        let Some(mut written) = incoming else { continue };
        for ((needed, reads), before) in needed.iter_mut().zip(&block.entry_reads).zip(&written) {
            *needed |= reads & !before;
        }
        if block.returns {
            for (current, generated) in written.iter_mut().zip(&block.written) {
                *current |= generated;
            }
            if let Some(previous) = &mut returned {
                for (old, new) in previous.iter_mut().zip(written) {
                    *old &= new;
                }
            } else {
                returned = Some(written);
            }
        }
    }
    Ok(proven_targets(targets, returned, &needed))
}

/// Preparation cannot suspend or nest another choice. Each lexical preparation
/// therefore supplies exactly the offers for its own Choose instruction, even
/// when option bodies later contain nested choices. An incomplete group leaves
/// this optional proof unavailable rather than inventing a destination.
fn choice_offers(code: &[Instruction], start: Instant) -> Result<Option<BTreeMap<usize, Vec<usize>>>, String> {
    let mut prepared = None::<Vec<usize>>;
    let mut choices = BTreeMap::new();
    for (index, instruction) in code.iter().enumerate() {
        if index % 256 == 0 {
            crate::budget(start)?;
        }
        match instruction.op {
            Op::BeginChoice => {
                if prepared.replace(vec![]).is_some() {
                    return Ok(None);
                }
            }
            Op::Offer { target, .. } => {
                let Some(offers) = prepared.as_mut() else {
                    return Ok(None);
                };
                offers.push(target);
            }
            Op::Choose { .. } => {
                let Some(offers) = prepared.take() else {
                    return Ok(None);
                };
                choices.insert(index, offers);
            }
            _ => {}
        }
    }
    Ok(prepared.is_none().then_some(choices))
}

struct Block {
    written: Vec<u64>,
    entry_reads: Vec<u64>,
    next: Vec<usize>,
    returns: bool,
}

/// Straight regions contribute one generated-write set. Keeping facts per basic
/// block avoids multiplying a large setter's facts by every arithmetic operation.
fn blocks(
    code: &[Instruction],
    stores: &BTreeMap<&str, usize>,
    words: usize,
    offers: &BTreeMap<usize, Vec<usize>>,
    start: Instant,
) -> Result<Option<Vec<Block>>, String> {
    let mut leaders = BTreeSet::from([0]);
    let mut edge_count = 0;
    for (index, instruction) in code.iter().enumerate() {
        if index % 256 == 0 {
            crate::budget(start)?;
        }
        if matches!(control(&instruction.op), Control::Next) {
            continue;
        }
        if index + 1 < code.len() {
            leaders.insert(index + 1);
        }
        let destinations = offers.get(&index).map(Vec::as_slice).unwrap_or_default();
        for target in successors(&instruction.op, index + 1, destinations) {
            if target >= code.len() {
                return Ok(None);
            }
            edge_count += 1;
            if edge_count > MAX_FLOW_STEPS {
                return Ok(None);
            }
            leaders.insert(target);
        }
    }
    if leaders
        .len()
        .checked_mul(words)
        .and_then(|size| size.checked_mul(3))
        .is_none_or(|size| size > MAX_ANALYSIS_WORDS)
    {
        return Ok(None);
    }
    let mut starts = leaders.into_iter().collect::<Vec<_>>();
    starts.push(code.len());
    let mut containing = vec![0; code.len()];
    for (index, bounds) in starts.windows(2).enumerate() {
        containing[bounds[0]..bounds[1]].fill(index);
    }
    let Some(body_start) = entry_reads::body_start(code) else {
        return Ok(None);
    };
    let mut result = Vec::with_capacity(starts.len() - 1);
    for bounds in starts.windows(2) {
        let mut written = vec![0; words];
        let mut entry_reads = vec![0; words];
        for (offset, instruction) in code[bounds[0]..bounds[1]].iter().enumerate() {
            if bounds[0] + offset >= body_start {
                entry_reads::record(&instruction.op, stores, &written, &mut entry_reads);
            }
            if let Op::Store { name } = &instruction.op
                && let Some(bit) = stores.get(name.as_str())
            {
                written[bit / 64] |= 1 << (bit % 64);
            }
        }
        let last = &code[bounds[1] - 1].op;
        let destinations = block_successors(code, bounds[0], bounds[1], offers);
        if destinations.iter().any(|target| *target >= code.len()) {
            return Ok(None);
        }
        result.push(Block {
            written,
            entry_reads,
            next: destinations.into_iter().map(|target| containing[target]).collect(),
            returns: matches!(last, Op::Return),
        });
    }
    Ok(Some(result))
}

/// An immediately preceding Boolean constant fixes this edge only when the
/// branch is inside the same basic block. A short-circuit jump may otherwise
/// enter the branch directly with a different value already on the stack.
fn block_successors(
    code: &[Instruction],
    start: usize,
    end: usize,
    choices: &BTreeMap<usize, Vec<usize>>,
) -> Vec<usize> {
    let op = &code[end - 1].op;
    let offers = choices.get(&(end - 1)).map(Vec::as_slice).unwrap_or_default();
    if end - start >= 2
        && let Op::Constant {
            ty: crate::model::Type::Boolean,
            value,
        } = &code[end - 2].op
        && let Some(value) = value.as_bool()
    {
        let taken = match op {
            Op::JumpFalse { target } => Some((!value, *target)),
            Op::JumpTrue { target } => Some((value, *target)),
            _ => None,
        };
        if let Some((taken, target)) = taken {
            return vec![if taken { target } else { end }];
        }
    }
    successors(op, end, offers)
}

fn proven_targets(targets: Vec<Target>, written: Option<Vec<u64>>, needed: &[u64]) -> Vec<Target> {
    targets
        .into_iter()
        .enumerate()
        .filter_map(|(index, mut target)| {
            target.reads_after_write = needed[index / 64] & (1 << (index % 64)) == 0;
            written
                .as_ref()
                .is_some_and(|written| written[index / 64] & (1 << (index % 64)) != 0)
                .then_some(target)
        })
        .collect()
}

struct LinearFacts {
    written: Vec<u64>,
    entry_reads: Vec<u64>,
}

/// Large straight-line helpers need two bitsets, independent of instruction count.
/// Any branch restarts the general must-analysis from the function entry.
fn linear_writes(
    code: &[Instruction],
    stores: &BTreeMap<&str, usize>,
    words: usize,
    start: Instant,
) -> Result<Option<LinearFacts>, String> {
    let mut written = vec![0; words];
    let mut needed = vec![0; words];
    for (index, instruction) in code.iter().enumerate() {
        if index % 256 == 0 {
            crate::budget(start)?;
        }
        entry_reads::record(&instruction.op, stores, &written, &mut needed);
        if let Op::Store { name } = &instruction.op
            && let Some(bit) = stores.get(name.as_str())
        {
            written[bit / 64] |= 1 << (bit % 64);
        }
        match control(&instruction.op) {
            Control::Next => {}
            Control::Return => {
                return Ok(Some(LinearFacts {
                    written,
                    entry_reads: needed,
                }));
            }
            Control::Stop => {
                written.fill(0);
                return Ok(Some(LinearFacts {
                    written,
                    entry_reads: needed,
                }));
            }
            _ => return Ok(None),
        }
    }
    written.fill(0);
    Ok(Some(LinearFacts {
        written,
        entry_reads: needed,
    }))
}

enum Control {
    Next,
    Return,
    Stop,
    Jump(usize),
    Branch(usize),
    Choose(Option<usize>),
}

/// Include every option for this choice and its optional fallback. Conditions
/// may hide offers at runtime, so all offers in the preparation remain possible.
fn successors(op: &Op, next: usize, offers: &[usize]) -> Vec<usize> {
    match control(op) {
        Control::Next => vec![next],
        Control::Return | Control::Stop => vec![],
        Control::Jump(target) => vec![target],
        Control::Branch(target) => vec![next, target],
        Control::Choose(fallback) => offers.iter().copied().chain(fallback).collect(),
    }
}

fn control(op: &Op) -> Control {
    match op {
        Op::Return => Control::Return,
        Op::Unreachable => Control::Stop,
        Op::Jump { target } => Control::Jump(*target),
        Op::JumpFalse { target }
        | Op::JumpTrue { target }
        | Op::JumpNull { target, .. }
        | Op::Default { target, .. }
        | Op::IterNext { target, .. } => Control::Branch(*target),
        Op::Choose { fallback } => Control::Choose(*fallback),
        Op::SceneCall { transfer, .. } => {
            if *transfer {
                Control::Stop
            } else {
                Control::Next
            }
        }
        Op::LegacyCall { target, .. } => {
            if target.transfer {
                Control::Stop
            } else {
                Control::Next
            }
        }
        // Keep this list exhaustive so a new control-flow instruction requires review.
        Op::Constant { .. }
        | Op::Load { .. }
        | Op::Declare { .. }
        | Op::Store { .. }
        | Op::Uninitialized { .. }
        | Op::Closure { .. }
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
        | Op::WriteAddress
        | Op::Range { .. }
        | Op::RangeStep
        | Op::IterStart { .. }
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
        | Op::Call { .. }
        | Op::Format { .. }
        | Op::Print
        | Op::Construct { .. }
        | Op::ProseUnwind { .. }
        | Op::ProseStart
        | Op::ProseText
        | Op::ProseFlush
        | Op::ProseEnd
        | Op::FunctionValue { .. }
        | Op::BeginChoice
        | Op::Offer { .. } => Control::Next,
    }
}

pub(super) fn apply(targets: &[Target], context: &mut FunctionContext) {
    for target in targets {
        if target.owner.as_ref() != context.function_id {
            continue;
        }
        let Some(scope) = context.scopes.get_mut(target.scope) else {
            continue;
        };
        if scope.get(&target.name).is_some_and(|binding| {
            binding.owner == target.owner.as_ref()
                && binding.runtime == target.runtime
                && binding.identity == target.identity
                && (binding.mutable || (target.single_use && !binding.may_be_initialized))
        }) {
            let binding = Arc::make_mut(scope).get_mut(&target.name).unwrap();
            binding.initialized = true;
            binding.may_be_initialized = true;
            // The helper overwrote this exact cell with a value checked at its
            // captured-store boundary. Old deferred reads no longer describe it.
            binding.requires.clear();
        }
    }
}
