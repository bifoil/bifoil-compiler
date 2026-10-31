//! Effects that retain caller-owned cells after a local callable returns.
//! A value read retains its payload; a closure capture retains the cell itself.

use super::{EmptyDependencies, Requirement, Requirements, capture_required_slot, empty, mark_escaped};
use crate::compiler::ScopeStack;
use std::collections::BTreeSet;

pub(in crate::compiler) type Publications = BTreeSet<Publication>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::compiler) enum Publication {
    Capture(Requirement),
    Value(Requirement),
    Invoke(Requirement),
}
impl Publication {
    pub(in crate::compiler) fn requirement(&self) -> &Requirement {
        match self {
            Self::Capture(required) | Self::Value(required) | Self::Invoke(required) => required,
        }
    }
    pub(in crate::compiler) fn at_depth(&self, depth: usize) -> Self {
        let required = self.requirement().at_depth(depth);
        match self {
            Self::Capture(_) => Self::Capture(required),
            Self::Value(_) => Self::Value(required),
            Self::Invoke(_) => Self::Invoke(required),
        }
    }
}

/// Follow immutable named-call graphs with their supplied/default profiles.
/// Variable-dependent leaves always use the current scope state. The bounded
/// empty-graph cache never treats a mutable cell's current payload as permanent.
pub(in crate::compiler) fn mark_published(
    publications: &Publications,
    env: &mut ScopeStack,
    empty: &mut EmptyDependencies,
) {
    let mut pending = publications.iter().cloned().collect::<Vec<_>>();
    let mut visited = Publications::new();
    let mut proof = empty::Proof::new();
    while let Some(effect) = pending.pop() {
        let required = effect.requirement();
        let Some((_, _, binding)) = capture_required_slot(env, required) else {
            proof.include(None);
            continue;
        };
        // Globals outlive every caller. Any payload stored there has already
        // crossed its own publication guard; it cannot capture a caller's locals
        // except through arguments, which call lowering checks separately.
        if binding.owner.is_empty() {
            continue;
        }
        let key = matches!(effect, Publication::Invoke(_))
            .then(|| EmptyDependencies::key(required, binding))
            .flatten();
        if key.as_ref().is_some_and(|key| empty.contains(key)) {
            continue;
        }
        if !visited.insert(effect.clone()) {
            proof.revisit(key.as_ref());
            continue;
        }
        match effect {
            Publication::Capture(required) => {
                proof.include(None);
                mark_escaped(&Requirements::from([required]), env);
            }
            Publication::Value(required) => {
                proof.include(None);
                let payload = required.dependencies(binding).into_owned();
                mark_escaped(&payload, env);
            }
            Publication::Invoke(_) if binding.callables.is_empty() => {
                proof.include(None);
                let payload = binding.requires.clone();
                mark_escaped(&payload, env);
                // Invoking an existing function value may retain its captured
                // cells, but it does not retain its mutable source variable.
            }
            Publication::Invoke(required) => {
                proof.include(key);
                let supplied = match required.unscoped() {
                    Requirement::Call { supplied, .. } => Some(supplied),
                    _ => None,
                };
                let selected = supplied.and_then(|supplied| supplied.selected(&binding.callables));
                for (index, signature) in binding.callables.iter().enumerate() {
                    if selected.is_some_and(|selected| selected != index) {
                        continue;
                    }
                    let omitted = if let Some(supplied) = supplied {
                        let Some(omitted) = supplied.omitted(signature) else {
                            continue;
                        };
                        omitted
                    } else {
                        Vec::new()
                    };
                    let Some(captures) = binding.callable_requirements.get(&signature.id) else {
                        proof.include(None);
                        continue;
                    };
                    pending.extend(captures.publications.iter().cloned());
                    for parameter in omitted {
                        if let Some(effects) = captures.default_publications.get(&parameter) {
                            pending.extend(effects.iter().cloned());
                        }
                    }
                }
            }
        }
    }
    proof.finish(empty);
}
