//! Reuse equal callable headers only after source checking or artifact verification.

use crate::model::{Export, Function, Module, Signature};
use std::collections::BTreeMap;
use std::sync::Arc;

fn canonical<'a, C>(signature: &Signature, functions: &'a BTreeMap<String, Function<C>>) -> Option<&'a Signature> {
    let stored = &functions.get(&signature.id)?.signature;
    (!Arc::ptr_eq(signature, stored) && signature == stored).then_some(stored)
}
fn share<C>(signature: &mut Signature, functions: &BTreeMap<String, Function<C>>) {
    if let Some(stored) = canonical(signature, functions) {
        *signature = Arc::clone(stored);
    }
}

/// Header equality is structural, including all parameter flags and type shapes.
/// Reusing the allocation therefore preserves canonical serialized bytes.
pub(crate) fn module<C>(module: &mut Module<BTreeMap<String, Function<C>>>) {
    let functions = &module.functions;
    for export in module.exports.values_mut() {
        match export {
            Export::Function { signature } => share(signature, functions),
            Export::Functions { signatures } => {
                for signature in signatures {
                    share(signature, functions);
                }
            }
            Export::Type {
                constructor: Some(signature),
                ..
            } => share(signature, functions),
            _ => {}
        }
    }
    for definition in module.types.values_mut() {
        if definition
            .methods
            .values()
            .flatten()
            .any(|signature| canonical(signature, functions).is_some())
        {
            for signatures in Arc::make_mut(definition).methods.values_mut() {
                for signature in signatures {
                    share(signature, functions);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn source() -> Module {
        let text = r#"export fun <T> identity(value: T) -> T = value
export fun pick(value: Int) = value
export fun pick(value: String) = value
export data class Box<T>:
    val item: T
    fun read() -> T = item
"#;
        crate::compiler::compile(
            crate::parser::parse(text, "src/shared.bfl").unwrap(),
            "shared",
            "src/shared.bfl",
            &BTreeMap::new(),
            Instant::now(),
        )
        .unwrap()
    }
    fn assert_shared(module: &Module) {
        let mut count = 0;
        for export in module.exports.values() {
            let signatures = match export {
                Export::Function { signature } => std::slice::from_ref(signature),
                Export::Functions { signatures } => signatures.as_slice(),
                Export::Type { constructor, .. } => constructor.as_slice(),
                _ => &[],
            };
            for signature in signatures {
                assert!(Arc::ptr_eq(signature, &module.functions[&signature.id].signature));
                count += 1;
            }
        }
        for signature in module
            .types
            .values()
            .flat_map(|definition| definition.methods.values())
            .flatten()
        {
            assert!(Arc::ptr_eq(signature, &module.functions[&signature.id].signature));
            count += 1;
        }
        assert!(count >= 5);
    }
    #[test]
    fn source_and_verified_artifact_views_share_headers_with_identical_canonical_bytes() {
        let module = source();
        assert_shared(&module);
        let canonical = serde_json::to_vec(&module).unwrap();
        let mut restored: Module = serde_json::from_slice(&canonical).unwrap();
        let Export::Function { signature } = &restored.exports["identity"] else {
            panic!("expected function")
        };
        assert!(!Arc::ptr_eq(signature, &restored.functions[&signature.id].signature));
        crate::verify(&mut restored).unwrap();
        assert_shared(&restored);
        assert_eq!(serde_json::to_vec(&restored).unwrap(), canonical);
    }
    #[test]
    fn detached_export_mismatch_is_rejected_without_changing_other_views() {
        let original = source();
        let canonical = serde_json::to_vec(&original).unwrap();
        let mut invalid = original.clone();
        let Export::Function { signature } = invalid.exports.get_mut("identity").unwrap() else {
            panic!("expected function")
        };
        Arc::make_mut(signature).result = crate::model::Type::Float;
        invalid.digest.clear();
        invalid.digest = crate::model::digest(&invalid).unwrap();
        assert!(
            crate::verify(&mut invalid)
                .unwrap_err()
                .contains("exported function signature does not match")
        );
        assert_eq!(serde_json::to_vec(&original).unwrap(), canonical);
        assert_shared(&original);
    }
}
