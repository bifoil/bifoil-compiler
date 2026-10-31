//! Scoped reuse of immutable dependency lists without changing their JSON arrays.

use std::collections::BTreeSet;
use std::sync::Arc;

const MAX_RETAINED_NAMES: usize = 32768;
const MAX_RETAINED_LISTS: usize = 4096;

#[derive(Default)]
pub(crate) struct Interner {
    lists: BTreeSet<Arc<[String]>>,
    names: usize,
}
impl Interner {
    pub(crate) fn intern(&mut self, list: Arc<[String]>) -> Arc<[String]> {
        if let Some(existing) = self.lists.get(&list) {
            return Arc::clone(existing);
        }
        if self.lists.len() < MAX_RETAINED_LISTS && self.names + list.len() <= MAX_RETAINED_NAMES {
            self.names += list.len();
            self.lists.insert(Arc::clone(&list));
        }
        list
    }
}
