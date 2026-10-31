//! Linked callables retain their defining module's ordered map.
//! Full signatures are consumed into checksums after semantic planning.
//!
//! Package output borrows those maps directly. Inline output merges their
//! ordered iterators with one cursor per module, without copying every key.

use crate::model::Function;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, btree_map};
use std::time::Instant;

#[derive(Clone, Copy)]
pub(crate) struct HeaderDigest(pub(crate) [u8; 32]);

pub(crate) struct FunctionCatalog<T = Function> {
    groups: BTreeMap<String, BTreeMap<String, T>>,
    count: usize,
}
impl<T> Default for FunctionCatalog<T> {
    fn default() -> Self {
        Self {
            groups: BTreeMap::new(),
            count: 0,
        }
    }
}
impl<T> FunctionCatalog<T> {
    pub(crate) fn insert(&mut self, module: String, functions: BTreeMap<String, T>) -> Result<(), String> {
        if self.groups.contains_key(&module) {
            return Err(format!("duplicate linked module identity {module}"));
        }
        self.count += functions.len();
        self.groups.insert(module, functions);
        Ok(())
    }

    /// Check global uniqueness before entry resolution or initializer planning.
    /// Module names and member prefixes cannot establish function ownership.
    pub(crate) fn validate(&self, start: Instant) -> Result<(), String> {
        let mut previous = None;
        for (index, (id, _)) in self.sorted().enumerate() {
            if index % 1024 == 0 {
                crate::budget(start)?;
            }
            if previous == Some(id) {
                return Err(format!("duplicate linked function identity {id}"));
            }
            previous = Some(id);
        }
        Ok(())
    }

    pub(crate) fn get(&self, id: &str) -> Option<&T> {
        self.groups.values().find_map(|functions| functions.get(id))
    }
    pub(crate) fn len(&self) -> usize {
        self.count
    }
    pub(crate) fn groups(&self) -> impl Iterator<Item = (&str, &BTreeMap<String, T>)> {
        self.groups
            .iter()
            .map(|(module, functions)| (module.as_str(), functions))
    }
    pub(crate) fn values(&self) -> impl Iterator<Item = &T> {
        self.groups.values().flat_map(|functions| functions.values())
    }
    pub(crate) fn sorted(&self) -> Sorted<'_, T> {
        let heads = self
            .groups
            .values()
            .filter_map(|functions| {
                let mut remaining = functions.iter();
                let (id, function) = remaining.next()?;
                Some(Head {
                    id,
                    function,
                    remaining,
                })
            })
            .collect();
        Sorted { heads }
    }
}

impl FunctionCatalog<Function> {
    /// All body checks and shared-initializer queries must finish before this
    /// consuming transition. Publication needs IDs and exact header digests,
    /// rather than retaining every parameter/type graph until the root is written.
    pub(crate) fn freeze(self, start: Instant) -> Result<FunctionCatalog<HeaderDigest>, String> {
        let expected = self.count;
        let mut frozen = FunctionCatalog::default();
        for (module, functions) in self.groups {
            let mut records = BTreeMap::new();
            for (index, (id, function)) in functions.into_iter().enumerate() {
                if index % 256 == 0 {
                    crate::budget(start)?;
                }
                if function.signature.id != id {
                    return Err(format!("stored function header changed: {id}"));
                }
                if !function.code.is_empty() {
                    return Err(format!("cannot freeze unspooled function {id}"));
                }
                records.insert(id, HeaderDigest(crate::code_store::header_checksum(&function)?));
            }
            frozen.insert(module, records)?;
        }
        if frozen.count != expected {
            return Err("linked function count changed during publication preparation".into());
        }
        crate::budget(start)?;
        Ok(frozen)
    }
}

pub(crate) struct Sorted<'a, T> {
    heads: BinaryHeap<Head<'a, T>>,
}
impl<'a, T> Iterator for Sorted<'a, T> {
    type Item = (&'a str, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        let mut head = self.heads.pop()?;
        let result = (head.id.as_str(), head.function);
        if let Some((id, function)) = head.remaining.next() {
            head.id = id;
            head.function = function;
            self.heads.push(head);
        }
        Some(result)
    }
}

struct Head<'a, T> {
    id: &'a String,
    function: &'a T,
    remaining: btree_map::Iter<'a, String, T>,
}
impl<T> PartialEq for Head<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl<T> Eq for Head<'_, T> {}
impl<T> PartialOrd for Head<'_, T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Head<'_, T> {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max heap; reverse keys to emit canonical ascending order.
        other.id.cmp(self.id)
    }
}

#[cfg(test)]
mod tests;
