//! Stable topological ordering. Cycles are errors, even behind unchosen branches.

use crate::model::SharedDef;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

pub(crate) fn order(shared: &BTreeMap<String, SharedDef>, started: Instant) -> Result<Vec<String>, String> {
    let mut remaining = BTreeMap::new();
    let mut dependents = BTreeMap::<&str, Vec<&str>>::new();
    let mut ready = BTreeSet::new();
    let priority = |id: &str| {
        (
            id.rsplit_once('.').map_or("", |(module, _)| module).to_owned(),
            shared[id].order,
            id.to_owned(),
        )
    };
    for (id, definition) in shared {
        let dependencies: BTreeSet<_> = definition.dependencies.iter().map(String::as_str).collect();
        remaining.insert(id.as_str(), dependencies.len());
        if dependencies.is_empty() {
            ready.insert(priority(id));
        }
        for dependency in dependencies {
            if !shared.contains_key(dependency) {
                return Err(format!("missing shared dependency {dependency} for {id}"));
            }
            dependents.entry(dependency).or_default().push(id);
        }
    }
    let mut result = Vec::with_capacity(shared.len());
    while let Some((_, _, id)) = ready.pop_first() {
        crate::budget(started)?;
        result.push(id.clone());
        for dependent in dependents.get(id.as_str()).into_iter().flatten() {
            let count = remaining.get_mut(dependent).unwrap();
            *count -= 1;
            if *count == 0 {
                ready.insert(priority(dependent));
            }
        }
    }
    if result.len() != shared.len() {
        let blocked = remaining
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        return Err(format!(
            "shared initialization dependency cycle involving {}",
            blocked.join(", ")
        ));
    }
    Ok(result)
}
