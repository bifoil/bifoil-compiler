//! Bounded memoization for dependency graphs with no captured-value reads.
//! Keys identify immutable declared callables and their complete call profiles.
//! Variable-dependent graphs are always checked against the current environment.

use super::super::Binding;
use super::Requirement;
use super::requirements::SuppliedArguments;
use std::collections::BTreeSet;

const MAX_ENTRIES: usize = 4096;
const MAX_KEY_BYTES: usize = 1_000_000;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Key {
    functions: Vec<String>,
    supplied: Option<SuppliedArguments>,
}
impl Key {
    pub(super) fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + 64
            + self.functions.len() * std::mem::size_of::<String>()
            + self.functions.iter().map(String::len).sum::<usize>()
            + self.supplied.as_ref().map_or(0, SuppliedArguments::bytes)
    }
}

#[derive(Default)]
pub(in crate::compiler) struct EmptyDependencies {
    keys: BTreeSet<Key>,
    bytes: usize,
}
impl EmptyDependencies {
    pub(super) fn key(required: &Requirement, binding: &Binding) -> Option<Key> {
        if binding.owner.is_empty()
            || binding.mutable
            || binding.volatile
            || !binding.initialized
            || binding.callables.is_empty()
            || binding
                .callables
                .iter()
                .any(|signature| !binding.callable_requirements.contains_key(&signature.id))
        {
            return None;
        }
        let supplied = match required.unscoped() {
            Requirement::Scoped { .. } => unreachable!("unscoped requirement"),
            Requirement::Write(_) => return None,
            Requirement::Binding(_) => None,
            Requirement::Call { supplied, .. } => {
                if !binding
                    .callables
                    .iter()
                    .any(|signature| supplied.omitted(signature).is_some())
                {
                    return None;
                }
                Some(supplied)
            }
        };
        let bytes = std::mem::size_of::<Key>()
            + 64
            + binding.callables.len() * std::mem::size_of::<String>()
            + binding
                .callables
                .iter()
                .map(|signature| signature.id.len())
                .sum::<usize>()
            + supplied.map_or(0, SuppliedArguments::bytes);
        if bytes > MAX_KEY_BYTES {
            return None;
        }
        let mut functions = binding
            .callables
            .iter()
            .map(|signature| signature.id.clone())
            .collect::<Vec<_>>();
        functions.sort_unstable();
        Some(Key {
            functions,
            supplied: supplied.cloned(),
        })
    }
    pub(super) fn contains(&self, key: &Key) -> bool {
        self.keys.contains(key)
    }
    pub(super) fn record(&mut self, keys: BTreeSet<Key>) {
        for key in keys {
            let bytes = key.bytes();
            if self.keys.len() < MAX_ENTRIES && self.bytes + bytes <= MAX_KEY_BYTES && self.keys.insert(key) {
                self.bytes += bytes;
            }
        }
    }
}

/// Accumulate one closed proof, without treating a successfully checked variable
/// as an empty dependency. Scope-state checks may succeed now and fail later.
pub(super) struct Proof {
    candidates: BTreeSet<Key>,
    bytes: usize,
    pure: bool,
}
impl Proof {
    pub(super) fn new() -> Self {
        Self {
            candidates: BTreeSet::new(),
            bytes: 0,
            pure: true,
        }
    }
    fn invalidate(&mut self) {
        self.pure = false;
        self.candidates.clear();
        self.bytes = 0;
    }
    pub(super) fn revisit(&mut self, key: Option<&Key>) {
        // Back edges within this proof are safe only if the entire component
        // has no leaves. A prior variable-dependent check proves no emptiness.
        if self.pure && !key.is_some_and(|key| self.candidates.contains(key)) {
            self.invalidate();
        }
    }
    pub(super) fn include(&mut self, key: Option<Key>) {
        if !self.pure {
            return;
        }
        let Some(key) = key else {
            self.invalidate();
            return;
        };
        if self.candidates.contains(&key) {
            return;
        }
        let bytes = key.bytes();
        if self.candidates.len() >= MAX_ENTRIES || self.bytes + bytes > MAX_KEY_BYTES {
            self.invalidate();
            return;
        }
        self.candidates.insert(key);
        self.bytes += bytes;
    }
    pub(super) fn finish(self, empty: &mut EmptyDependencies) {
        if self.pure {
            empty.record(self.candidates);
        }
    }
}
