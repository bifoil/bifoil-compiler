//! Persistent lexical type-name frames. A child stores only declarations that
//! it adds; aliases and sibling declarations retain immutable parent snapshots.

use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Default)]
pub(super) struct Scope {
    frame: Arc<Frame>,
}
#[derive(Clone, Default)]
struct Frame {
    parent: Option<Arc<Frame>>,
    bindings: BTreeMap<String, String>,
}
impl Scope {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn child(&self) -> Self {
        Self {
            frame: Arc::new(Frame {
                parent: Some(Arc::clone(&self.frame)),
                bindings: BTreeMap::new(),
            }),
        }
    }

    pub(super) fn get(&self, name: &str) -> Option<&String> {
        let mut frame = self.frame.as_ref();
        loop {
            if let Some(value) = frame.bindings.get(name) {
                return Some(value);
            }
            frame = frame.parent.as_deref()?;
        }
    }

    pub(super) fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Return any visible old binding so callers retain their shadowing checks.
    /// A retained parent is never modified by a child's insertion.
    pub(super) fn insert(&mut self, name: String, value: String) -> Option<String> {
        let previous = self.get(&name).cloned();
        Arc::make_mut(&mut self.frame).bindings.insert(name, value);
        previous
    }

    pub(super) fn extend(&mut self, bindings: BTreeMap<String, String>) {
        Arc::make_mut(&mut self.frame).bindings.extend(bindings);
    }

    pub(super) fn values(&self) -> impl Iterator<Item = &String> {
        std::iter::successors(Some(self.frame.as_ref()), |frame| frame.parent.as_deref())
            .flat_map(|frame| frame.bindings.values())
    }
}
