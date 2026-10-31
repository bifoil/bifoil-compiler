//! Order transparent alias declarations before expanding their bodies or bounds.
//!
//! Nominal declarations are leaves here: their fields are resolved separately.
//! Only aliases in the current lexical group need ordering; imported and enclosing
//! aliases already have normalized definitions. The graph stores indices, not ASTs.

use super::{AliasAst, NameBindings, Type};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn declarations(aliases: &[AliasAst], names: &NameBindings) -> Result<Vec<usize>, String> {
    let ids: BTreeMap<_, _> = aliases
        .iter()
        .enumerate()
        .map(|(index, alias)| (names.get(&alias.name).expect("registered alias").as_str(), index))
        .collect();
    let mut remaining = Vec::with_capacity(aliases.len());
    let mut dependents = vec![Vec::new(); aliases.len()];
    for (index, alias) in aliases.iter().enumerate() {
        let parameters: BTreeSet<_> = alias.generics.iter().map(|parameter| parameter.name.as_str()).collect();
        let mut dependencies = BTreeSet::new();
        for ty in alias
            .generics
            .iter()
            .flat_map(|parameter| &parameter.bounds)
            .chain(std::iter::once(&alias.ty))
        {
            ty.for_each_node(|node| {
                if let Type::Named { name, .. } = node
                    && !parameters.contains(name.as_str())
                {
                    let resolved = names.get(name).map(String::as_str).unwrap_or(name);
                    if let Some(&dependency) = ids.get(resolved) {
                        dependencies.insert(dependency);
                    }
                }
            });
        }
        remaining.push(dependencies.len());
        for dependency in dependencies {
            dependents[dependency].push(index);
        }
    }
    let mut ready: BTreeSet<_> = remaining
        .iter()
        .enumerate()
        .filter_map(|(index, count)| (*count == 0).then_some(index))
        .collect();
    let mut ordered = Vec::with_capacity(aliases.len());
    while let Some(index) = ready.pop_first() {
        ordered.push(index);
        for &dependent in &dependents[index] {
            remaining[dependent] -= 1;
            if remaining[dependent] == 0 {
                ready.insert(dependent);
            }
        }
    }
    if ordered.len() != aliases.len() {
        return Err("type alias expansion cycle".into());
    }
    Ok(ordered)
}
