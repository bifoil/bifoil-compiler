//! Immutable map lookups are stable after normalizing a constant key's conversions.

use super::compile;
use crate::model::Type;

#[test]
fn constant_primitive_map_keys_share_sound_narrowing_and_presence_proofs() {
    let module = compile(
        r#"
data class Box:
    val value: String?
fun numeric(values: Map<Float, String?>):
    if values[1] == null:
        return 0
    return values[1.0].length
fun text(values: Map<String, String?>):
    if values['a'] == null:
        return 0
    return values["a"].length
fun zero(values: Map<Float, String?>):
    if values[-0.0] == null:
        return 0
    return values[0].length
fun negative(values: Map<Int, String?>):
    if values[-1] == null:
        return 0
    return values[-1].length
fun optional(values: Map<String?, String?>):
    if values[null] == null:
        return 0
    return values[null].length
fun boolean(values: Map<Boolean, String?>):
    if values[true] == null:
        return 0
    return values[true].length
fun unitKey(values: Map<Unit, String?>):
    values[Unit]!!
    return values[Unit].length
fun field(values: Map<String, Box?>):
    if values["box"] == null || values["box"].value == null:
        return 0
    return values["box"].value.length
fun matched(values: Map<String, String?>) = when values["value"]:
    case null:
        0
    else:
        values["value"].length
fun <T> original(values: Map<String, T>, fallback: T) -> T:
    if values["value"] is String:
        return values["value"]
    return fallback
scene start():
    Unit
"#,
    )
    .unwrap();
    for name in [
        "numeric", "text", "zero", "negative", "optional", "boolean", "unitKey", "field", "matched",
    ] {
        assert_eq!(module.functions[&format!("test.{name}")].signature.result, Type::Int);
    }
}

#[test]
fn mutable_maps_changed_keys_and_replaced_roots_cannot_retain_lookup_facts() {
    for source in [
        r#"fun read(values: MutableMap<String, String?>) -> Int:
    if values["value"] != null:
        values["value"] = null
        return values["value"].length
    return 0
"#,
        r#"fun read(values: Map<String, String?>, var key: String) -> Int:
    if values[key] != null:
        key = "other"
        return values[key].length
    return 0
"#,
        r#"fun read(var values: Map<String, String?>) -> Int:
    fun change():
        values = {"value": null}
    if values["value"] != null:
        change()
        return values["value"].length
    return 0
"#,
        r#"fun read(var values: Map<String, String?>) -> Int:
    fun change() -> String:
        values = {"value": null}
        return "Ada"
    return when values["value"]:
        case change():
            values["value"].length
        else:
            0
"#,
    ] {
        let error = compile(source).expect_err("a changing map location cannot retain a proof");
        assert!(error.contains("nullable"), "{error}\n{source}");
    }
}

#[test]
fn map_path_proofs_stop_at_mutable_fields_and_nested_mutable_containers() {
    for source in [
        r#"data class Box:
    var value: String?
fun read(values: Map<String, Box>) -> Int:
    if values["box"].value != null:
        values["box"].value = null
        return values["box"].value.length
    return 0
"#,
        r#"fun read(values: Map<String, MutableMap<String, String?>>) -> Int:
    if values["inner"] == null:
        return 0
    if values["inner"]["value"] != null:
        values["inner"]["value"] = null
        return values["inner"]["value"].length
    return 0
"#,
    ] {
        let error = compile(source).expect_err("immutable outer maps cannot freeze their referenced contents");
        assert!(error.contains("nullable"), "{error}\n{source}");
    }
}

#[test]
fn enum_case_keys_share_proofs_between_qualified_and_expected_type_spellings() {
    let module = compile(
        r#"enum Mood:
    Happy
    Sad
fun qualified(values: Map<Mood, String?>):
    if values[Mood.Happy] == null:
        return 0
    return values[Happy].length
fun shorthand(values: Map<Mood, String?>):
    if values[Happy] == null:
        return 0
    return values[Mood.Happy].length
fun nullable(values: Map<Mood?, String?>):
    values[Mood.Happy]!!
    return values[Happy].length
"#,
    )
    .unwrap();
    for name in ["qualified", "shorthand", "nullable"] {
        assert_eq!(module.functions[&format!("test.{name}")].signature.result, Type::Int);
    }
}

#[test]
fn enum_key_proofs_do_not_transfer_to_other_cases_or_changing_keys() {
    for body in [
        r#"fun read(values: Map<Mood, String?>) -> Int:
    if values[Mood.Happy] != null:
        return values[Mood.Sad].length
    return 0
"#,
        r#"fun read(values: Map<Mood, String?>, var selected: Mood) -> Int:
    if values[selected] != null:
        selected = Mood.Sad
        return values[selected].length
    return 0
"#,
    ] {
        let source = format!("enum Mood:\n    Happy\n    Sad\n{body}");
        let error = compile(&source).expect_err("distinct or changing keys cannot share a proof");
        assert!(error.contains("nullable"), "{error}\n{source}");
    }
}
