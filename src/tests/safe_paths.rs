//! Non-null safe access proves stable receiver prefixes, without replaying reads.

use super::compile;
use crate::model::Type;

#[test]
fn safe_fields_indexes_strings_and_when_cases_prove_present_receivers() {
    let module = compile(
        r#"data class Person:
    val name: String?
data class House:
    val person: Person?
fun field(person: Person?):
    if person?.name == null:
        return 0
    return person.name.length
fun nested(house: House?):
    house?.person?.name!!
    return house.person.name.length
fun indexed(values: Map<String, Person?>?, key: String):
    if values?[key]?.name == null:
        return 0
    return values[key].name.length
fun character(text: String?, index: Int):
    if text?[index] == null:
        return 0
    return text.length
fun matched(person: Person?) = when person?.name:
    case null:
        0
    else:
        person.name.length
fun checked(person: Person?) -> String:
    person?.name!!
    return person?.name
scene start():
    Unit
"#,
    )
    .unwrap();
    for name in ["field", "nested", "indexed", "character", "matched"] {
        assert_eq!(module.functions[&format!("test.{name}")].signature.result, Type::Int);
    }
}

#[test]
fn safe_path_proofs_keep_generic_identity_and_immutable_captured_values() {
    let module = compile(
        r#"data class Box<T>:
    val value: T
fun <T> choose(box: Box<T>?, fallback: T):
    if box?.value == null:
        return fallback
    return box.value
fun reader(values: Map<String, String?>?, key: String) -> () -> Int:
    if values?[key] == null:
        return () => 0
    return () => values[key].length
scene start():
    Unit
"#,
    )
    .unwrap();
    let signature = &module.functions["test.choose"].signature;
    assert_eq!(signature.result, Type::parameter(&signature.generics[0].name));
}

#[test]
fn nullable_type_tests_effectful_indexes_and_changed_snapshots_do_not_prove_receivers() {
    for source in [
        r#"data class Person:
    val name: String?
fun read(person: Person?) -> Int:
    if person?.name is String?:
        return person.name.length
    return 0
"#,
        r#"fun read(var values: Map<String, String?>?) -> Int:
    fun key() -> String:
        values = null
        return "name"
    if values?[key()] != null:
        return values["name"].length
    return 0
"#,
        r#"data class Person:
    val name: String?
fun read(var person: Person?) -> Int:
    fun change() -> String:
        person = null
        return "Ada"
    return when person?.name:
        case change():
            person.name.length
        else:
            0
"#,
        r#"fun read(values: Map<String, String?>?, var key: String) -> Int:
    values?[key]!!
    key = "missing"
    return values[key].length
"#,
    ] {
        let error = compile(source).unwrap_err();
        assert!(error.contains("nullable"), "{error}\n{source}");
    }
}

#[test]
fn nested_expected_enum_keys_resolve_in_the_typed_path_without_receiver_reinference() {
    let mut nested = String::from("String?");
    let mut lookup = String::from("values");
    for _ in 0..14 {
        nested = format!("Map<Mood, {nested}>");
        lookup.push_str("[Happy]!!");
    }
    compile(&format!(
        "enum Mood:\n    Happy\ntypealias Nested = {nested}\nfun read(values: Nested) -> Int = {lookup}.length\nscene start():\n    Unit\n"
    )).unwrap();
}
