//! Shared-cell source aliases and assignment facts, independent of value copies.

use super::{Binding, ScopeStack};
use crate::compiler::captures::Requirements;
use std::collections::BTreeMap;
use std::sync::Arc;

/// File-scope imports are complete before body/return inference. Index only
/// groups with multiple names; ordinary bindings allocate no alias container.
pub(in crate::compiler) fn index_global_aliases(env: &mut ScopeStack) {
    let groups = {
        let mut groups = BTreeMap::<&str, Vec<&str>>::new();
        for (name, binding) in env[0].iter() {
            if binding.owner.is_empty() && binding.callables.is_empty() {
                groups.entry(&binding.runtime).or_default().push(name);
            }
        }
        groups
            .into_values()
            .filter(|names| names.len() > 1)
            .map(|names| names.into_iter().map(str::to_owned).collect::<Arc<[String]>>())
            .collect::<Vec<_>>()
    };
    if groups.is_empty() {
        return;
    }
    let root = Arc::make_mut(&mut env[0]);
    for aliases in groups {
        for name in aliases.iter() {
            root.get_mut(name).unwrap().global_aliases = Some(Arc::clone(&aliases));
        }
    }
}

fn assignment_changes(binding: &Binding, requirements: &Requirements) -> bool {
    !binding.initialized
        || !binding.may_be_initialized
        || binding.narrow.is_some()
        || binding.field_narrows.is_some()
        || binding.requires != *requirements
}

fn written(binding: &mut Binding, requirements: Requirements) {
    binding.initialized = true;
    binding.may_be_initialized = true;
    binding.narrow = None;
    binding.field_narrows = None;
    binding.requires = requirements;
}

/// Apply facts after the right-hand side and conversion complete. Shared source
/// aliases observe the same write; local snapshots retain their earlier value.
/// Both lowering and return inference use this operation.
pub(in crate::compiler) fn record_assignment(env: &mut ScopeStack, name: &str, requirements: Requirements) {
    let Some(index) = env.iter().rposition(|scope| scope.contains_key(name)) else {
        return;
    };
    let aliases = env[index][name].global_aliases.clone();
    let inspect = |binding: &Binding| (assignment_changes(binding, &requirements), binding.index_key);
    let (changed, indexed) = if let Some(aliases) = &aliases {
        aliases.iter().fold((false, false), |(changed, indexed), name| {
            let (next_changed, next_indexed) = inspect(&env[index][name]);
            (changed || next_changed, indexed || next_indexed)
        })
    } else {
        inspect(&env[index][name])
    };
    // Runtime stores still occur. Avoid copying a wide frame when its analysis
    // facts are identical; index/key proofs must be invalidated independently.
    if changed {
        let scope = Arc::make_mut(&mut env[index]);
        if let Some(aliases) = aliases {
            for name in aliases.iter() {
                written(
                    scope.get_mut(name).expect("registered shared alias"),
                    requirements.clone(),
                );
            }
        } else {
            written(scope.get_mut(name).unwrap(), requirements);
        }
    }
    if indexed {
        crate::compiler::field_proofs::invalidate_keys(env);
    }
}

/// A check of one source alias checks the same current global value under all
/// its imported names. Mutable/volatile proofs still obey ordinary effect rules.
pub(in crate::compiler) fn narrow_binding(
    env: &mut ScopeStack,
    name: &str,
    tested: Option<&crate::model::Type>,
    stable_only: bool,
) {
    let Some(index) = env.iter().rposition(|scope| scope.contains_key(name)) else {
        return;
    };
    let binding = &env[index][name];
    if binding.volatile || (stable_only && binding.mutable) {
        return;
    }
    let narrowed = tested.cloned().unwrap_or_else(|| binding.value_type(None).non_null());
    let aliases = binding.global_aliases.clone();
    let scope = Arc::make_mut(&mut env[index]);
    if let Some(aliases) = aliases {
        for name in aliases.iter() {
            scope.get_mut(name).expect("registered shared alias").narrow = Some(narrowed.clone());
        }
    } else {
        scope.get_mut(name).unwrap().narrow = Some(narrowed);
    }
}
