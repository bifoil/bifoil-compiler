//! Mutable lookup guards remain valid only while their locations stay unchanged.

use super::compile;
use crate::model::Type;

#[test]
fn mutable_list_map_and_nested_lookup_guards_retain_current_values() {
    let module = compile(
        r#"fun list(values: MutableList<String?>):
    if values[0] == null:
        return 0
    return values[0].length
fun mapping(values: MutableMap<Float, String?>):
    if values[1] == null:
        return 0
    return values[1.0].length
fun nested(values: List<MutableMap<String, String?>>):
    if values[0]["value"] == null:
        return 0
    return values[0]["value"].length
fun <T> original(values: MutableList<T>, fallback: T):
    if values[0] is String:
        return values[0]
    return fallback
"#,
    )
    .unwrap();
    for name in ["list", "mapping", "nested"] {
        assert_eq!(module.functions[&format!("test.{name}")].signature.result, Type::Int);
    }
    assert!(matches!(
        module.functions["test.original"].signature.result,
        Type::Parameter { .. }
    ));
}

#[test]
fn readonly_builtin_queries_and_copies_keep_lookup_facts() {
    compile(
        r#"fun length(values: MutableList<String?>) -> Int:
    values[0]!!
    val empty = values.isEmpty()
    val copy = values.toList()
    val contains = values.contains(null)
    val rendered = String(values)
    return values[0].length
"#,
    )
    .unwrap();
}

#[test]
fn all_collection_mutators_invalidate_lookup_proofs_through_aliases() {
    for mutation in [
        "alias.clear()",
        "alias.remove(null)",
        "alias.removeAt(0)",
        "alias.insert(0, null)",
        "alias.add(null)",
        "alias[0] = null",
    ] {
        let error = compile(&format!(
            r#"fun invalid(values: MutableList<String?>) -> Int:
    if values[0] == null:
        return 0
    val alias = values
    {mutation}
    return values[0].length
"#
        ))
        .unwrap_err();
        assert!(error.contains("nullable member receiver"), "{mutation}: {error}");
    }
    for mutation in [
        "alias.clear()",
        r#"alias.remove("value")"#,
        r#"alias.put("value", null)"#,
        r#"alias["value"] = null"#,
    ] {
        let error = compile(&format!(
            r#"fun invalid(values: MutableMap<String, String?>):
    if values["value"] == null:
        return 0
    val alias = values
    {mutation}
    return values["value"].length
"#
        ))
        .unwrap_err();
        assert!(error.contains("nullable member receiver"), "{mutation}: {error}");
    }
}

#[test]
fn changing_keys_and_escaping_readers_cannot_reuse_mutable_lookup_facts() {
    for body in [
        "selected = 1\n    return values[selected].length",
        "val read = () => values[selected].length\n    return read()",
    ] {
        let error = compile(&format!(
            r#"fun invalid(values: MutableList<String?>, var selected: Int) -> Int:
    if values[selected] == null:
        return 0
    {body}
"#
        ))
        .unwrap_err();
        assert!(error.contains("nullable member receiver"), "{error}");
    }
}

#[test]
fn field_and_container_writes_preserve_locations_they_cannot_change() {
    compile(
        r#"data class Box:
    var value: String?
fun length(holder: Box, values: MutableList<String?>) -> Int:
    if values[0] == null:
        return 0
    holder.value = "Ada"
    val first = values[0].length
    if holder.value == null:
        return first
    values.add(null)
    return first + holder.value.length
fun optional(values: MutableList<String?>?) -> Int:
    if values?[0] == null:
        return 0
    return values[0].length
"#,
    )
    .unwrap();
}

#[test]
fn ready_key_assignments_invalidate_lookup_proofs_without_new_binding_facts() {
    let error = super::compile(
        r#"
fun read(items: List<String?>, var key: Int) -> Int:
    if items[key] == null:
        return 0
    key += 1
    return items[key].length
scene start():
    val result = read(["ready", null], 0)
"#,
    )
    .unwrap_err();
    assert!(error.contains("nullable"), "{error}");
}
