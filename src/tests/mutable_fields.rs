//! Mutable field guards are valid until an alias write or effect invalidates them.

use super::compile;
use crate::model::Type;

#[test]
fn unchanged_mutable_fields_support_guards_assertions_and_nested_paths() {
    compile(
        r#"data class Person:
    var name: String?
data class House:
    var person: Person?
fun length(home: House) -> Int:
    if home.person?.name == null:
        return 0
    val first = home.person.name.length
    home.person.name!!
    return first + home.person.name.length
fun asserted(person: Person) -> Int:
    person.name!!
    return person.name.length
"#,
    )
    .unwrap();
}

#[test]
fn mutable_field_views_preserve_generic_and_inferred_result_types() {
    let module = compile(
        r#"data class Box<T>:
    var value: T?
fun <T> choose(holder: Box<T>, fallback: T):
    if holder.value != null:
        return holder.value
    return fallback
fun length(holder: Box<String>):
    if holder.value == null:
        return 0
    return holder.value.length
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.length"].signature.result, Type::Int);
    assert!(matches!(
        module.functions["test.choose"].signature.result,
        Type::Parameter { .. }
    ));
}

#[test]
fn alias_writes_calls_and_guarded_when_effects_discard_mutable_field_proofs() {
    for body in [
        "val alias = holder\n    alias.value = null\n    return holder.value.length",
        "val alias = holder\n    mutate(alias)\n    return holder.value.length",
        "val values = mutable [holder]\n    values[0].value = null\n    return holder.value.length",
    ] {
        let source = format!(
            r#"data class Box:
    var value: String?
fun mutate(target: Box):
    target.value = null
fun invalid(holder: Box) -> Int:
    if holder.value == null:
        return 0
    {body}
"#
        );
        let error = compile(&source).unwrap_err();
        assert!(error.contains("nullable member receiver"), "{error}");
    }
}

#[test]
fn escaping_functions_and_prepared_guards_cannot_reuse_mutable_field_proofs() {
    for body in [
        "fun read() -> Int = holder.value.length\n    return read()",
        "val read = () => holder.value.length\n    return read()",
    ] {
        let error = compile(&format!(
            r#"data class Box:
    var value: String?
fun invalid(holder: Box) -> Int:
    if holder.value == null:
        return 0
    {body}
"#
        ))
        .unwrap_err();
        assert!(error.contains("nullable member receiver"), "{error}");
    }
    let error = compile(
        r#"data class Box:
    var value: String?
suspend fun invalid(holder: Box) -> Int:
    choice:
        option "Read" if holder.value != null:
            return holder.value.length
        else:
            return 0
"#,
    )
    .unwrap_err();
    assert!(error.contains("nullable member receiver"), "{error}");
}

#[test]
fn independent_immutable_field_facts_survive_heap_writes() {
    compile(
        r#"data class Box:
    var changing: String?
    val fixed: String?
fun length(holder: Box) -> Int:
    if holder.fixed == null:
        return 0
    holder.changing = null
    return holder.fixed.length
"#,
    )
    .unwrap();
}

#[test]
fn heap_writes_and_lexical_key_writes_preserve_independent_proofs() {
    compile(
        r#"data class Box:
    var value: String?
fun length(holder: Box, values: Map<Int, String?>) -> Int:
    var selected = 0
    if values[selected] == null:
        return 0
    holder.value = null
    val first = values[selected].length
    if holder.value == null:
        return first
    selected = 1
    return first + holder.value.length
"#,
    )
    .unwrap();
}
