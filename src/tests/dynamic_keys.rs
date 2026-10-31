//! Stable immutable index bindings retain facts without confusing lexical cells.

use super::compile;
use crate::model::Type;

#[test]
fn immutable_parameter_keys_and_indexes_retain_null_and_type_guards() {
    let module = compile(
        r#"fun read(values: Map<String, String?>, key: String):
    if values[key] == null:
        return 0
    return values[key].length
fun item(values: List<String?>, index: Int):
    values[index]!!
    return values[index].length
fun kind(values: Map<String, Any?>, key: String):
    if values[key] !is String:
        return 0
    return values[key].length
fun widened(values: Map<String, String?>, key: Char):
    if values[key] != null:
        return values[key].length
    return 0
fun matched(values: Map<String, String?>, key: String) = when values[key]:
    case null:
        0
    else:
        values[key].length
scene start():
    Unit
"#,
    )
    .unwrap();
    for name in ["read", "item", "kind", "widened", "matched"] {
        assert_eq!(module.functions[&format!("test.{name}")].signature.result, Type::Int);
    }
}

#[test]
fn dynamic_key_facts_preserve_generic_identity_and_captured_immutable_values() {
    compile(
        r#"fun <K: StableKey, T> extract(values: Map<K, T>, key: K) -> T:
    values[key]!!
    return values[key]
fun reader(values: Map<String, String?>, key: String) -> () -> Int:
    if values[key] == null:
        return () => 0
    return () => values[key].length
suspend fun delayed(values: Map<String, String?>, key: String) -> Int:
    if values[key] == null:
        return 0
    choice:
        option "Read":
            Unit
    return values[key].length
scene start():
    val read = reader({"name": "Ada"}, "name")
    val result = extract({"name": "Ada"}, "name")
"#,
    )
    .unwrap();
}

#[test]
fn mutable_keys_containers_and_different_bindings_do_not_share_facts() {
    for source in [
        r#"fun read(values: Map<String, String?>, var key: String) -> Int:
    if values[key] != null:
        key = "other"
        return values[key].length
    return 0
"#,
        r#"fun read(values: MutableMap<String, String?>, key: String) -> Int:
    if values[key] != null:
        values[key] = null
        return values[key].length
    return 0
"#,
        r#"fun read(values: Map<String, String?>, first: String, second: String) -> Int:
    if values[first] != null:
        return values[second].length
    return 0
"#,
        r#"data class Selector:
    var key: String
fun read(values: Map<String, String?>, selector: Selector) -> Int:
    if values[selector.key] != null:
        selector.key = "other"
        return values[selector.key].length
    return 0
"#,
    ] {
        let error = compile(source).unwrap_err();
        assert!(error.contains("nullable"), "{error}\n{source}");
    }
}

#[test]
fn a_later_same_named_binding_cannot_reuse_an_earlier_key_proof() {
    for source in [
        r#"scene start():
    val values: Map<String, String?> = {"first": "Ada", "second": null}
    if true:
        val key = "first"
        values[key]!!
    if true:
        val key = "second"
        val size = values[key].length
"#,
        r#"scene start():
    val values: List<String?> = ["Ada", null]
    if true:
        val index = 0
        values[index]!!
    if true:
        val index = 1
        val size = values[index].length
"#,
    ] {
        let error = compile(source).unwrap_err();
        assert!(error.contains("nullable"), "{error}\n{source}");
    }
}

#[test]
fn unchanged_mutable_keys_support_guards_until_the_key_is_written_or_called() {
    let module = compile(
        r#"fun read(values: Map<String, String?>, var key: String):
    var other = 0
    if values[key] == null:
        return 0
    other += 1
    return values[key].length + other
fun item(values: List<String?>, var index: Int):
    values[index]!!
    return values[index].length
fun retained(values: Map<String, String?>, var key: String, effect: () -> Unit):
    val fixed = "name"
    values[fixed]!!
    values[key]!!
    effect()
    return values[fixed].length
fun checkedAgain(values: Map<String, String?>, var key: String):
    values[key]!!
    key = "name"
    values[key]!!
    return values[key].length
scene start():
    Unit
"#,
    )
    .unwrap();
    for name in ["read", "item", "retained", "checkedAgain"] {
        assert_eq!(module.functions[&format!("test.{name}")].signature.result, Type::Int);
    }
}

#[test]
fn mutable_key_facts_do_not_survive_calls_escaping_readers_or_snapshot_guards() {
    for source in [
        r#"fun read(values: Map<String, String?>, var key: String, effect: () -> Unit) -> Int:
    values[key]!!
    effect()
    return values[key].length
"#,
        r#"fun read(values: Map<String, String?>, var key: String) -> () -> Int:
    values[key]!!
    return () => values[key].length
"#,
        r#"fun read(values: Map<String, String?>, var key: String) -> Int:
    fun change() -> String:
        key = "missing"
        return "Ada"
    return when values[key]:
        case change():
            values[key].length
        else:
            0
"#,
        r#"scene start():
    var key = "first"
    val values: Map<String, String?> = {"first": "Ada", "second": null}
    choice:
        option "Read" if values[key] != null:
            val length = values[key].length
"#,
        r#"fun read(values: Map<String, String?>, var key: String) -> Int:
    key = if true:
        values[key]!!
        "missing"
    else:
        "name"
    return values[key].length
"#,
    ] {
        let error = compile(source).unwrap_err();
        assert!(error.contains("nullable"), "{error}\n{source}");
    }
}

#[test]
fn pure_computed_keys_retain_null_type_assertion_and_when_guards() {
    let module = compile(
        r#"fun offset(values: List<String?>, index: Int):
    if values[index + 1] == null:
        return 0
    return values[(index + 1)].length
fun arithmetic(values: List<String?>, first: Int, second: Int):
    values[(first * 2) - second]!!
    return values[(first * 2) - second].length
fun key(values: Map<String, String?>, prefix: String, index: Int):
    if values[prefix + index] != null:
        return values[prefix + index].length
    return 0
fun kind(values: Map<String, Any?>, prefix: String):
    if values[prefix + ":name"] !is String:
        return 0
    return values[prefix + ":name"].length
fun matched(values: List<String?>, index: Int) = when values[index + 1]:
    case null:
        0
    else:
        values[index + 1].length
fun constant(values: Map<String, String?>):
    values["na" + "me"]!!
    return values["na" + "me"].length
scene start():
    Unit
"#,
    )
    .unwrap();
    for name in ["offset", "arithmetic", "key", "kind", "matched", "constant"] {
        assert_eq!(module.functions[&format!("test.{name}")].signature.result, Type::Int);
    }
}

#[test]
fn computed_key_proofs_survive_calls_captures_and_unchanged_suspension() {
    compile(
        r#"fun effect():
    Unit
fun read(values: List<String?>, index: Int) -> () -> Int:
    values[index + 1]!!
    effect()
    return () => values[index + 1].length
suspend fun unchanged(values: List<String?>, var index: Int) -> Int:
    values[index + 1]!!
    choice:
        option "Read":
            Unit
    return values[index + 1].length
suspend fun delayed(values: Map<String, String?>, prefix: String, suffix: Int) -> Int:
    values[prefix + suffix]!!
    choice:
        option "Read":
            Unit
    return values[prefix + suffix].length
scene start():
    Unit
"#,
    )
    .unwrap();
}

#[test]
fn every_mutable_computed_key_dependency_is_invalidated_on_assignment() {
    for assignment in ["first += 1", "second += 1"] {
        let source = format!(
            r#"fun read(values: List<String?>, var first: Int, var second: Int) -> Int:
    values[first + second]!!
    {assignment}
    return values[first + second].length
scene start():
    Unit
"#
        );
        let error = compile(&source).unwrap_err();
        assert!(error.contains("nullable"), "{error}");
    }
    compile(
        r#"fun read(values: List<String?>, var index: Int) -> Int:
    values[index + 1]!!
    index += 1
    values[index + 1]!!
    return values[index + 1].length
scene start():
    Unit
"#,
    )
    .unwrap();
}

#[test]
fn computed_key_proofs_reject_changed_heap_effects_and_lexical_cells() {
    for source in [
        r#"fun read(values: Map<String, String?>, var prefix: String) -> Int:
    fun change() -> String:
        prefix = "missing"
        return "Ada"
    return when values[prefix + 0]:
        case change():
            values[prefix + 0].length
        else:
            0
"#,
        r#"fun read(values: MutableList<String?>, index: Int) -> Int:
    values[index + 1]!!
    val alias = values
    alias[index + 1] = null
    return values[index + 1].length
"#,
        r#"fun read(values: List<String?>, var index: Int, effect: () -> Unit) -> Int:
    values[index + 1]!!
    effect()
    return values[index + 1].length
"#,
        r#"suspend fun read(values: List<String?>, var index: Int) -> Int:
    values[index + 1]!!
    choice:
        option "Wait":
            index += 1
    return values[index + 1].length
"#,
        r#"fun chooseIndex() -> Int = 0
fun read(values: List<String?>) -> Int:
    values[chooseIndex() + 1]!!
    return values[chooseIndex() + 1].length
"#,
        r#"fun read(values: List<String?>) -> Int:
    if true:
        val index = 0
        values[index + 1]!!
    if true:
        val index = 1
        return values[index + 1].length
    return 0
"#,
    ] {
        let error = compile(&format!("{source}scene start():\n    Unit\n")).unwrap_err();
        assert!(error.contains("nullable"), "{error}\n{source}");
    }
}

#[test]
fn computed_key_proof_limits_keep_large_expressions_conservative() {
    let mut expression = "index".to_string();
    for _ in 0..8 {
        expression = format!("({expression} + {expression})");
    }
    let expressions = [
        ("List<String?>", "index: Int", expression),
        (
            "Map<String, String?>",
            "prefix: String",
            format!("prefix + \"{}\"", "x".repeat(4_096)),
        ),
    ];
    for (container, parameter, expression) in expressions {
        let source = format!(
            r#"fun read(values: {container}, {parameter}) -> Int:
    values[{expression}]!!
    return values[{expression}].length
scene start():
    Unit
"#
        );
        let error = compile(&source).unwrap_err();
        assert!(error.contains("nullable"), "{error}");
    }
}

#[test]
fn computed_keys_use_primitive_generic_upper_views_and_null_refinements() {
    compile(
        r#"fun <I: Int> offset(values: List<String?>, index: I) -> Int:
    values[index + 1]!!
    return values[index + 1].length
fun <S: String, I: Int> formatted(values: Map<String, String?>, prefix: S, index: I) -> Int:
    values["${prefix}${index}"]!!
    return values["${prefix}${index}"].length
fun <S: String?> optional(values: Map<String, String?>, prefix: S) -> Int:
    if prefix == null:
        return 0
    values[prefix + "!"]!!
    return values[prefix + "!"].length
fun <I: Int?> optionalIndex(values: List<String?>, index: I) -> Int:
    if index == null:
        return 0
    values[index + 1]!!
    return values[index + 1].length
scene start():
    Unit
"#,
    )
    .unwrap();
}

#[test]
fn computed_text_keys_support_scalar_length_indexing_and_interpolation() {
    compile(
        r#"fun length(values: List<String?>, text: String) -> Int:
    values[text.length]!!
    return values[text.length].length
fun scalar(values: Map<String, String?>, text: String, index: Int) -> Int:
    values["${text[index]}!"]!!
    return values["${text[index]}!"].length
fun concatenated(values: Map<String, String?>, text: String, index: Int) -> Int:
    values[text[index] + "!"]!!
    return values[text[index] + "!"].length
scene start():
    Unit
"#,
    )
    .unwrap();
}

#[test]
fn computed_keys_read_immutable_field_paths_without_freezing_unrelated_fields() {
    compile(
        r#"data class Key:
    val text: String?
    var count: Int = 0
data class Wrapper:
    val key: Key
fun read(values: Map<String, String?>, wrapper: Wrapper) -> () -> Int:
    if wrapper.key.text == null:
        return () => 0
    values["${wrapper.key.text}!"]!!
    wrapper.key.count += 1
    return () => values["${wrapper.key.text}!"].length
fun measured(values: List<String?>, wrapper: Wrapper) -> Int:
    if wrapper.key.text == null:
        return 0
    values[wrapper.key.text.length]!!
    wrapper.key.count += 1
    return values[wrapper.key.text.length].length
scene start():
    Unit
"#,
    )
    .unwrap();
}

#[test]
fn computed_text_keys_reject_changed_sources_and_mutable_field_paths() {
    for source in [
        r#"data class Key:
    var text: String
fun read(values: Map<String, String?>, key: Key) -> Int:
    values["${key.text}!"]!!
    key.text = "other"
    return values["${key.text}!"].length
"#,
        r#"data class Key:
    val text: String
fun read(values: Map<String, String?>, var key: Key) -> Int:
    values["${key.text}!"]!!
    key = Key("other")
    return values["${key.text}!"].length
"#,
        r#"fun read(values: Map<String, String?>, var text: String) -> Int:
    values["${text[0]}!"]!!
    text = "other"
    return values["${text[0]}!"].length
"#,
        r#"fun read(values: Map<String, String?>, text: MutableList<String>) -> Int:
    values["${text[0]}!"]!!
    text[0] = "other"
    return values["${text[0]}!"].length
"#,
        r#"fun key() -> String = "name"
fun read(values: Map<String, String?>) -> Int:
    values["${key()}!"]!!
    return values["${key()}!"].length
"#,
    ] {
        let error = compile(&format!("{source}scene start():\n    Unit\n")).unwrap_err();
        assert!(error.contains("nullable"), "{error}\n{source}");
    }
}
