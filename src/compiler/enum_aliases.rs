//! Qualified cases of transparent aliases keep their original enum identity.

use super::bindings::lookup_ref;
use super::{ScopeStack, insert};
use crate::model::Type;
use crate::syntax::{AliasAst, Statement, StatementKind};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn register_block(body: &[Statement], env: &mut ScopeStack) -> Result<(), String> {
    register(
        body.iter().filter_map(|statement| match &statement.kind {
            StatementKind::Alias(alias) => Some(alias),
            _ => None,
        }),
        env,
    )
}

pub(super) fn register<'a>(
    aliases: impl IntoIterator<Item = &'a AliasAst>,
    env: &mut ScopeStack,
) -> Result<(), String> {
    let aliases = aliases
        .into_iter()
        .filter_map(|alias| {
            let Type::Named { name, args } = &alias.ty else {
                return None;
            };
            if !alias.generics.is_empty() || !args.is_empty() {
                return None;
            }
            crate::declarations::definition(name)
                .filter(|definition| definition.kind == "enum")
                .map(|definition| (alias, definition))
        })
        .collect::<Vec<_>>();
    if aliases.is_empty() {
        return Ok(());
    }
    // Collect visible cases once. Scanning every lexical binding for every
    // case would make large enum namespaces quadratic.
    let required = aliases
        .iter()
        .flat_map(|(_, definition)| definition.variants.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    let mut bindings = BTreeMap::new();
    for binding in env.iter().flat_map(|scope| scope.values()) {
        if let Some(&id) = required.get(binding.runtime.as_str())
            && super::members::constant_case(binding)
        {
            bindings.entry(id).or_insert_with(|| binding.clone());
        }
    }
    for (alias, definition) in &aliases {
        for variant in &definition.variants {
            let local = format!("{}.{}", alias.name, variant.rsplit('.').next().unwrap());
            // Value-prefix inference revisits the same block while resolving
            // local function returns. Register each namespace only once.
            if lookup_ref(env, &local).is_some_and(|binding| binding.runtime == *variant) {
                continue;
            }
            let binding = bindings
                .get(variant.as_str())
                .cloned()
                .ok_or_else(|| format!("enum alias '{}' has no accessible case '{variant}'", alias.name))?;
            insert(env, &local, binding)?;
        }
    }
    Ok(())
}
