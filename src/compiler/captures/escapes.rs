//! Publication guards for mutable lexical cells retained by escaping values.

use super::{EmptyDependencies, Requirements, capture_required_slot, check_requirements};
use crate::compiler::ScopeStack;
use std::sync::Arc;

pub(in crate::compiler) fn check_escaping(
    requirements: &Requirements,
    env: &mut ScopeStack,
    owner: &str,
    empty: &mut EmptyDependencies,
) -> Result<(), String> {
    check_requirements(requirements, env, owner, empty)?;
    mark_escaped(requirements, env);
    Ok(())
}

/// Retain the obligation on mutable cells, rather than a snapshot of their
/// current dependency sets. Subsequent writes must discharge their new reads.
/// Named-call profiles still exclude defaults supplied at those call sites.
pub(in crate::compiler) fn mark_escaped(requirements: &Requirements, env: &mut ScopeStack) {
    let mut pending = requirements
        .iter()
        .filter(|required| required.reads_binding())
        .cloned()
        .collect::<Vec<_>>();
    let mut visited = Requirements::new();
    while let Some(required) = pending.pop() {
        if !visited.insert(required.clone()) {
            continue;
        }
        let Some((index, name, binding)) = capture_required_slot(env, &required) else {
            continue;
        };
        pending.extend(
            required
                .dependencies(binding)
                .iter()
                .filter(|required| required.reads_binding())
                .cloned(),
        );
        // A writer cannot expose the cell's existing contents to opaque readers.
        if binding.mutable && !binding.owner.is_empty() && !binding.escaped {
            let name = name.to_owned();
            Arc::make_mut(&mut env[index]).get_mut(&name).unwrap().escaped = true;
        }
    }
}
