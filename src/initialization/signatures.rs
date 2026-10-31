//! Borrow declaration maps directly instead of flattening every reachable signature.

use crate::model::{Function, ModuleView};
use std::collections::BTreeMap;

pub(super) struct View<'a> {
    own: &'a BTreeMap<String, Function>,
    modules: &'a ModuleView<'a>,
}
impl<'a> View<'a> {
    pub(super) fn new(own: &'a BTreeMap<String, Function>, modules: &'a ModuleView<'a>) -> Self {
        Self { own, modules }
    }
    pub(super) fn get(&self, id: &str) -> Option<&Function> {
        if let Some(function) = self.own.get(id) {
            return Some(function);
        }
        // IDs include member and local suffixes, so the first module prefix
        // need not own the function. Continue through shorter module prefixes.
        for (end, _) in id.rmatch_indices('.') {
            if let Some(module) = self.modules.get(&id[..end])
                && let Some(function) = module.functions.get(id)
            {
                return Some(function);
            }
        }
        None
    }
    pub(super) fn contains_key(&self, id: &str) -> bool {
        self.get(id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Module, SignatureData, Type};
    use std::sync::Arc;

    fn module(name: &str, ids: &[&str]) -> Module {
        let ast = crate::parser::parse("", "src/test.bfl").unwrap();
        let mut module = crate::compiler::seed(&ast, name).unwrap();
        for id in ids {
            module.functions.insert(
                (*id).into(),
                Function {
                    signature: Arc::new(SignatureData {
                        id: (*id).into(),
                        generics: vec![],
                        params: Arc::from([]),
                        result: Type::Int,
                        suspend: false,
                        scene: false,
                    }),
                    code: vec![],
                    init: false,
                    local: false,
                },
            );
        }
        module
    }

    #[test]
    fn borrowed_lookup_handles_member_suffixes_nested_modules_and_own_overrides() {
        let outer = module("pkg", &["pkg.Box.read", "pkg.helper"]);
        let nested = module("pkg.Box", &["pkg.Box.utility"]);
        let modules = BTreeMap::from([("pkg".into(), &outer), ("pkg.Box".into(), &nested)]);
        let empty = BTreeMap::new();
        let view = View::new(&empty, &modules);
        for id in ["pkg.Box.read", "pkg.helper", "pkg.Box.utility"] {
            assert_eq!(view.get(id).unwrap().signature.id, id);
        }
        assert!(!view.contains_key("pkg.Box.missing"));
        assert!(!view.contains_key("pkgExtra.helper"));
        let mut own = outer.functions.clone();
        Arc::make_mut(&mut own.get_mut("pkg.Box.read").unwrap().signature).result = Type::String;
        assert_eq!(
            View::new(&own, &modules).get("pkg.Box.read").unwrap().signature.result,
            Type::String
        );
    }
}
