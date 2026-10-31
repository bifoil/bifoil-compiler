//! Monotonic initialization facts implied by selecting a prepared option.

use super::super::ScopeStack;
use super::super::bindings::Identity;
use std::sync::Arc;

pub(super) struct Initialized(Vec<(usize, String, Identity)>);

impl Initialized {
    pub(super) fn capture(entry: &ScopeStack, offered: &ScopeStack) -> Self {
        let mut facts = Vec::new();
        for (index, (before, after)) in entry.iter().zip(offered).enumerate() {
            if Arc::ptr_eq(before, after) {
                continue;
            }
            for (name, binding) in before.iter().filter(|(_, binding)| !binding.initialized) {
                if after
                    .get(name)
                    .is_some_and(|current| current.identity == binding.identity && current.initialized)
                {
                    facts.push((index, name.clone(), binding.identity));
                }
            }
        }
        Self(facts)
    }

    pub(super) fn apply(&self, current: &mut ScopeStack) {
        for (index, name, identity) in &self.0 {
            let Some(scope) = current.get_mut(*index) else {
                continue;
            };
            let Some(binding) = scope.get(name).filter(|binding| binding.identity == *identity) else {
                continue;
            };
            if !binding.initialized {
                let mut initialized = binding.clone();
                initialized.initialized = true;
                initialized.may_be_initialized = true;
                // Later preparation can change values, dependencies, aliases,
                // or type facts. Preserve all of that current state; selecting
                // this option proves only that this exact cell was initialized.
                Arc::make_mut(scope).insert_mut(name.clone(), initialized);
            }
        }
    }
}
