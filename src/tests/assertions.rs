//! Checked assertions preserve contextual types and unchanged generic identities.

use super::compile;
use crate::model::Type;

#[test]
fn non_null_assertions_preserve_contextual_generic_binding_and_field_identity() {
    compile(
        r#"data class Box<T>:
    val value: T
fun <T> binding(value: T) -> T:
    if value is String:
        return value!!
    return value
fun <T> field(holder: Box<T>) -> T:
    if holder.value is String:
        return holder.value!!
    return holder.value
scene start():
    val first = binding("kept")
    val second = field(Box(7))
"#,
    )
    .unwrap();
}

#[test]
fn inferred_assertions_retain_the_non_null_original_type() {
    let module = compile(
        r#"data class Box<T>:
    val value: T
fun <T> binding(value: T):
    if value is String:
        return value!!
    return value!!
fun <T> field(holder: Box<T>):
    if holder.value is String:
        return holder.value!!
    return holder.value!!
scene start():
    Unit
"#,
    )
    .unwrap();
    for name in ["test.binding", "test.field"] {
        let signature = &module.functions[name].signature;
        assert_eq!(
            signature.result,
            Type::parameter(&signature.generics[0].name).non_null()
        );
    }
}

#[test]
fn assertions_supply_nullable_context_to_enum_and_collection_operands() {
    compile(
        r#"enum Mood:
    Happy
    Sad
fun chosen() -> Mood = Happy!!
fun empty() -> List<Int> = []!!
fun mutableEmpty() -> MutableList<Int> = []!!
scene start():
    val selected = chosen()
    val items = mutableEmpty()
    items.add(7)
"#,
    )
    .unwrap();
}

#[test]
fn asserting_a_computed_value_does_not_assign_it_a_generic_identity() {
    let error = compile(
        r#"fun <T> invalid(value: T) -> T:
    if value is String:
        return (value + "")!!
    return value
scene start():
    Unit
"#,
    )
    .unwrap_err();
    assert!(error.contains("expected T, found String"), "{error}");
}
