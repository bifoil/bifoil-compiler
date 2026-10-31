//! Borrow only the transitive modules explicitly imported by one source module.
//!
//! Loading order must not affect type visibility, initializer candidates, or a
//! reusable artifact's identity. Views borrow completed metadata without copying
//! dependency instruction bodies, type catalogs, or call summaries.

use crate::model::{Module, ModuleView};
use crate::syntax::Ast;
use std::collections::BTreeMap;

fn dependencies<'a>(
    owner: &str,
    roots: impl Iterator<Item = String>,
    available: &'a BTreeMap<String, Module>,
) -> Result<ModuleView<'a>, String> {
    let mut scope = BTreeMap::new();
    let mut pending: Vec<_> = roots.collect();
    while let Some(name) = pending.pop() {
        if scope.contains_key(&name) {
            continue;
        }
        let Some(module) = available.get(&name) else {
            // The owner may be temporarily removed while its summaries are
            // recomputed; its own declarations are supplied separately.
            if name == owner {
                continue;
            }
            return Err(format!("missing compiled module '{name}'"));
        };
        pending.extend(module.dependencies.keys().cloned());
        scope.insert(name, module);
    }
    Ok(scope)
}
pub(crate) fn for_ast<'a>(
    ast: &Ast,
    name: &str,
    available: &'a BTreeMap<String, Module>,
) -> Result<ModuleView<'a>, String> {
    dependencies(name, ast.imports.iter().map(|import| import.module.clone()), available)
}
pub(crate) fn for_module<'a>(
    module: &Module,
    available: &'a BTreeMap<String, Module>,
) -> Result<ModuleView<'a>, String> {
    dependencies(&module.name, module.dependencies.keys().cloned(), available)
}
