//! A small abstract interpreter that tracks callable origins across basic blocks.
//!
//! Unknown callable origins are never assumed harmless. Parameter values, captured
//! bindings, container reads and returned function values may require the caller's
//! conservative target set. This keeps analysis finite even with recursive calls.

use crate::model::{Function, Op, SharedDef, Type};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Origin {
    Known(Arc<BTreeSet<String>>),
    Unknown,
}
impl Origin {
    fn empty() -> Self {
        Self::Known(Arc::default())
    }
    fn function(id: &str) -> Self {
        Self::Known(Arc::new(BTreeSet::from([id.into()])))
    }
    fn join(&mut self, other: &Self) -> bool {
        if self == other || matches!(self, Self::Unknown) {
            return false;
        }
        if let (Self::Known(current), Self::Known(incoming)) = (&mut *self, other) {
            let previous = current.len();
            Arc::make_mut(current).extend(incoming.iter().cloned());
            current.len() != previous
        } else {
            *self = Self::Unknown;
            true
        }
    }
}
#[derive(Clone, Default)]
struct State {
    stack: Vec<Origin>,
    // Missing bindings represent Unknown; retain only known callable facts.
    locals: Arc<BTreeMap<String, Origin>>,
    stack_marks: BTreeMap<usize, usize>,
}
impl State {
    fn bind(&mut self, name: &str, origin: Origin) {
        if matches!(origin, Origin::Unknown) {
            if self.locals.contains_key(name) {
                Arc::make_mut(&mut self.locals).remove(name);
            }
        } else if self.locals.get(name) != Some(&origin) {
            Arc::make_mut(&mut self.locals).insert(name.into(), origin);
        }
    }
    fn pop(&mut self) -> Result<Origin, String> {
        self.stack
            .pop()
            .ok_or_else(|| "invalid operand stack in shared dependency analysis".into())
    }
    fn discard(&mut self, count: usize) -> Result<(), String> {
        if self.stack.len() < count {
            return Err("invalid operand stack in shared dependency analysis".into());
        }
        self.stack.truncate(self.stack.len() - count);
        Ok(())
    }
    fn join(&mut self, other: &Self) -> Result<bool, String> {
        if self.stack.len() != other.stack.len() {
            return Err("inconsistent operand stack in shared dependency analysis".into());
        }
        if self.stack_marks != other.stack_marks {
            return Err("inconsistent loop stack marks in shared dependency analysis".into());
        }
        let mut changed = false;
        for (current, incoming) in self.stack.iter_mut().zip(&other.stack) {
            changed |= current.join(incoming);
        }
        // Unknown joined with any origin remains unknown. Only facts present
        // in this predecessor can survive; missing incoming facts erase them.
        // Keep the shared map untouched when joining does not change a fact.
        if !Arc::ptr_eq(&self.locals, &other.locals) {
            let previous = Arc::clone(&self.locals);
            for (name, origin) in previous.iter() {
                let incoming = other.locals.get(name).unwrap_or(&Origin::Unknown);
                let mut merged = origin.clone();
                if merged.join(incoming) {
                    self.bind(name, merged);
                    changed = true;
                }
            }
        }
        Ok(changed)
    }
}

use crate::model::FunctionSummary as Summary;

fn may_be_callable(ty: &Type) -> bool {
    match ty {
        Type::Function { .. } | Type::Any | Type::Star | Type::Parameter { .. } | Type::Pending => true,
        Type::Nullable { inner } => may_be_callable(inner),
        _ => false,
    }
}

pub(super) fn analyze(
    function: &Function,
    functions: &super::signatures::View<'_>,
    shared: &BTreeMap<&str, &SharedDef>,
    started: Instant,
) -> Result<Summary, String> {
    let code = &function.code;
    let mut leaders = BTreeSet::from([0]);
    for (pc, instruction) in code.iter().enumerate() {
        let target = match &instruction.op {
            Op::Jump { target }
            | Op::JumpFalse { target }
            | Op::JumpTrue { target }
            | Op::JumpNull { target, .. }
            | Op::Default { target, .. }
            | Op::IterNext { target, .. } => Some(*target),
            _ => None,
        };
        if let Some(target) = target {
            if target >= code.len() {
                return Err("invalid jump in shared dependency analysis".into());
            }
            leaders.insert(target);
            if pc + 1 < code.len() {
                leaders.insert(pc + 1);
            }
        }
    }
    let mut entries = BTreeMap::from([(0, State::default())]);
    let mut pending = VecDeque::from([0]);
    let mut queued = BTreeSet::from([0]);
    let mut summary = Summary::default();
    while let Some(start) = pending.pop_front() {
        crate::budget(started)?;
        queued.remove(&start);
        let mut state = entries[&start].clone();
        let mut pc = start;
        let mut successors = Vec::new();
        loop {
            let instruction = code
                .get(pc)
                .ok_or("initializer control flow runs past its function body")?;
            match &instruction.op {
                Op::Unreachable => break,
                Op::StackMark { id } => {
                    if state.stack_marks.insert(*id, state.stack.len()).is_some() {
                        return Err("duplicate loop stack mark in shared dependency analysis".into());
                    }
                }
                Op::StackReset { id } => {
                    let depth = *state.stack_marks.get(id).ok_or("missing loop stack mark")?;
                    if depth > state.stack.len() {
                        return Err("invalid loop operand depth".into());
                    }
                    state.stack.truncate(depth);
                    state.stack_marks.retain(|mark, _| mark <= id);
                }
                Op::StackEnd { id } => {
                    state.stack_marks.remove(id).ok_or("missing loop stack mark")?;
                }
                Op::Load { name } => {
                    let origin = if let Some(definition) = shared.get(name.as_str()) {
                        summary.bindings.insert(name.clone());
                        if may_be_callable(&definition.ty) {
                            Origin::Unknown
                        } else {
                            Origin::empty()
                        }
                    } else if functions.contains_key(name.as_str()) || name.starts_with("$builtin.") {
                        Origin::function(name)
                    } else {
                        state.locals.get(name).cloned().unwrap_or(Origin::Unknown)
                    };
                    state.stack.push(origin);
                }
                Op::FunctionValue { function, .. } => state.stack.push(Origin::function(function)),
                Op::Closure { name, function, .. } => {
                    state.bind(name, Origin::function(function));
                }
                Op::Declare { name, .. } | Op::Store { name } => {
                    let origin = state.pop()?;
                    if shared.contains_key(name.as_str()) {
                        summary.bindings.insert(name.clone());
                    }
                    state.bind(name, origin);
                }
                Op::Uninitialized { name, .. } => {
                    state.bind(name, Origin::Unknown);
                }
                Op::Member { function, ty, .. } => {
                    state.pop()?;
                    state
                        .stack
                        .push(function.as_deref().map(Origin::function).unwrap_or_else(|| {
                            if may_be_callable(ty) {
                                Origin::Unknown
                            } else {
                                Origin::empty()
                            }
                        }));
                }
                Op::LegacyCall { arguments, .. } => {
                    state.discard(*arguments)?;
                    state.stack.push(Origin::empty());
                }
                Op::Call { names, .. } => {
                    state.discard(names.len())?;
                    let callable = state.pop()?;
                    let mut result = Origin::empty();
                    match callable {
                        Origin::Unknown => {
                            summary.indirect_call = true;
                            result = Origin::Unknown;
                        }
                        Origin::Known(targets) => {
                            for target in targets.iter() {
                                if target.starts_with("$builtin.") {
                                    // Collection removal/put operations can return stored functions.
                                    if ["$builtin.remove", "$builtin.removeAt", "$builtin.put"]
                                        .contains(&target.as_str())
                                    {
                                        result.join(&Origin::Unknown);
                                    }
                                } else {
                                    let callee = functions
                                        .get(target.as_str())
                                        .ok_or_else(|| format!("missing initializer call target {target}"))?;
                                    summary.calls.insert(target.clone());
                                    if may_be_callable(&callee.signature.result) {
                                        result.join(&Origin::Unknown);
                                    }
                                }
                            }
                        }
                    }
                    // A called closure may reassign a captured local callable.
                    // Values already captured on the operand stack stay unchanged.
                    state.locals = Arc::default();
                    state.stack.push(result);
                }
                Op::Pop | Op::Print | Op::ProseText | Op::IterStart { .. } => {
                    state.pop()?;
                }
                Op::Dup => {
                    state
                        .stack
                        .push(state.stack.last().ok_or("invalid dependency stack duplicate")?.clone());
                }
                Op::Constant { .. } | Op::Unused { .. } | Op::BeginCollection { .. } => {
                    state.stack.push(Origin::empty())
                }
                Op::Index => {
                    state.discard(2)?;
                    state.stack.push(Origin::Unknown);
                }
                Op::Binary { .. } | Op::Range { .. } | Op::RangeStep | Op::AddressIndex => {
                    state.discard(2)?;
                    state.stack.push(Origin::empty());
                }
                Op::Unary { .. } | Op::TypeTest { .. } | Op::AddressMember { .. } => {
                    state.pop()?;
                    state.stack.push(Origin::empty());
                }
                Op::ReadAddress => state.stack.push(Origin::Unknown),
                Op::WriteAddress | Op::CollectionPut => state.discard(2)?,
                Op::CollectionAdd => {
                    state.pop()?;
                }
                Op::Format { count } => {
                    state.discard(*count)?;
                    state.stack.push(Origin::empty());
                }
                Op::Construct { fields, .. } => {
                    state.discard(fields.len())?;
                    state.stack.push(Origin::empty());
                }
                Op::Jump { target } => {
                    successors.push(*target);
                    break;
                }
                Op::JumpFalse { target } | Op::JumpTrue { target } | Op::JumpNull { target, .. } => {
                    state.pop()?;
                    successors.extend([*target, pc + 1]);
                    break;
                }
                Op::Default { target, .. } => {
                    successors.extend([*target, pc + 1]);
                    break;
                }
                Op::IterNext { target, name, .. } => {
                    state.bind(name, Origin::Unknown);
                    successors.extend([*target, pc + 1]);
                    break;
                }
                Op::Return => {
                    state.pop()?;
                    break;
                }
                Op::SceneCall { .. } | Op::BeginChoice | Op::Offer { .. } | Op::Choose { .. } => {
                    return Err("shared initialization cannot enter story control flow".into());
                }
                Op::Widen
                | Op::Convert { .. }
                | Op::AssertPresent
                | Op::Cast { .. }
                | Op::Specialize { .. }
                | Op::CheckKey
                | Op::Enter
                | Op::Exit
                | Op::IterEnd { .. }
                | Op::Unwind { .. }
                | Op::ProseStart
                | Op::ProseFlush
                | Op::ProseEnd
                | Op::ProseUnwind { .. } => {}
            }
            pc += 1;
            if leaders.contains(&pc) {
                successors.push(pc);
                break;
            }
        }
        for successor in successors {
            if successor >= code.len() {
                return Err("invalid initializer control flow successor".into());
            }
            let changed = if let Some(previous) = entries.get_mut(&successor) {
                previous.join(&state)?
            } else {
                entries.insert(successor, state.clone());
                true
            };
            if changed && queued.insert(successor) {
                pending.push_back(successor);
            }
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests;
