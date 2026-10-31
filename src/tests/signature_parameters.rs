//! Signature views retain declaration metadata across independent specializations.

use super::compile;
use crate::generics;
use crate::model::{Signature, Type};
use std::collections::BTreeMap;
use std::sync::Arc;

fn declaration() -> Signature {
    compile(
        r#"export fun <T> gather(var first: T? = null, vararg rest: T) -> T? = first
"#,
    )
    .unwrap()
    .functions["test.gather"]
        .signature
        .clone()
}

#[test]
fn generic_specializations_keep_other_views_and_canonical_declarations_unchanged() {
    let declaration = declaration();
    let canonical = serde_json::to_vec(&declaration).unwrap();
    let unchanged = generics::signature(&declaration, &BTreeMap::new());
    assert!(Arc::ptr_eq(&unchanged, &declaration));
    assert!(Arc::ptr_eq(&unchanged.params, &declaration.params));
    let name = declaration.generics[0].name.clone();
    let integers = generics::signature(&declaration, &BTreeMap::from([(name.clone(), Type::Int)]));
    let strings = generics::signature(&declaration, &BTreeMap::from([(name, Type::String)]));
    assert_eq!(integers.params[0].ty, Type::Int.nullable());
    assert_eq!(integers.params[1].ty, Type::Int);
    assert_eq!(strings.params[0].ty, Type::String.nullable());
    assert_eq!(strings.params[1].ty, Type::String);
    assert_eq!(integers.result, Type::Int.nullable());
    assert_eq!(strings.result, Type::String.nullable());
    for specialized in [&integers, &strings] {
        assert!(specialized.params[0].mutable);
        assert!(specialized.params[0].default);
        assert!(specialized.params[1].vararg);
        assert!(!Arc::ptr_eq(&specialized.params, &declaration.params));
    }
    assert_eq!(serde_json::to_vec(&declaration).unwrap(), canonical);
    assert_eq!(serde_json::to_vec(&unchanged).unwrap(), canonical);
}

#[test]
fn empty_substitution_still_normalizes_nullable_application_in_a_private_header() {
    let mut declaration = declaration();
    Arc::make_mut(&mut declaration).params_mut()[0].ty = Type::Nullable {
        inner: Box::new(Type::Int.nullable()),
    };
    let canonical = serde_json::to_vec(&declaration).unwrap();
    let normalized = generics::signature(&declaration, &BTreeMap::new());
    assert!(matches!(&normalized.params[0].ty, Type::Nullable { inner } if **inner == Type::Int));
    assert_eq!(serde_json::to_vec(&declaration).unwrap(), canonical);
    assert!(!Arc::ptr_eq(&normalized.params, &declaration.params));
}
