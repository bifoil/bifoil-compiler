//! Constant indexes in immutable lists denote stable values, not repeated computations.

use super::compile;
use crate::model::Type;

#[test]
fn immutable_list_guards_infer_nonnull_values_and_preserve_generic_identity() {
    let module = compile(
        r#"
data class Box<T>:
    val value: T
fun length(values: List<String?>):
    if values[0] == null:
        return 0
    return values[0].length
fun nested(values: List<List<String?>?>):
    if values[0] == null || values[0][0] == null:
        return 0
    return values[0][0].length
fun field(values: List<Box<Any?>>):
    if values[0].value !is String:
        return 0
    return values[0].value.length
fun checked(values: List<String?>):
    values[0]!!
    return values[0].length
fun matched(values: List<String?>) = when values[0]:
    case null:
        0
    else:
        values[0].length
fun <T> original(values: List<T>) -> T:
    if values[0] is String:
        return values[0]
    return values[0]
fun <T> inferred(values: List<T>):
    if values[0] is String:
        return values[0]
    return values[0]
scene start():
    Unit
"#,
    )
    .unwrap();
    for name in ["length", "nested", "field", "checked", "matched"] {
        assert_eq!(module.functions[&format!("test.{name}")].signature.result, Type::Int);
    }
    assert!(matches!(
        module.functions["test.inferred"].signature.result,
        Type::Parameter { .. }
    ));
}

#[test]
fn mutable_containers_dynamic_indexes_and_changed_roots_do_not_acquire_index_facts() {
    let invalid = [
        r#"fun read(values: MutableList<String?>) -> Int:
    if values[0] != null:
        values.clear()
        return values[0].length
    return 0
"#,
        r#"fun read(values: List<String?>, var index: Int) -> Int:
    if values[index] != null:
        index = index + 1
        return values[index].length
    return 0
"#,
        r#"fun read(var values: List<String?>) -> Int:
    if values[0] != null:
        values = [null]
        return values[0].length
    return 0
"#,
        r#"fun read(var values: List<String?>) -> Int:
    fun change():
        values = [null]
    if values[0] != null:
        change()
        return values[0].length
    return 0
"#,
        r#"fun read(values: List<MutableList<String?>>) -> Int:
    if values[0][0] != null:
        values[0][0] = null
        return values[0][0].length
    return 0
"#,
        r#"data class Box:
    var value: String?
fun read(values: List<Box>) -> Int:
    if values[0].value != null:
        values[0].value = null
        return values[0].value.length
    return 0
"#,
    ];
    for source in invalid {
        let error = compile(source).expect_err("a changing location must not retain a non-null fact");
        assert!(error.contains("nullable"), "{error}\n{source}");
    }
}

#[test]
fn immutable_index_facts_survive_calls_but_when_tests_cannot_retarget_variable_roots() {
    compile(
        r#"
fun read(values: List<String?>):
    fun unrelated() = 1
    if values[0] == null:
        return 0
    unrelated()
    return values[0].length
fun local(var values: List<String?>):
    if values[0] == null:
        return 0
    return values[0].length
scene start():
    Unit
"#,
    )
    .unwrap();
    let error = compile(
        r#"
fun read(var values: List<String?>) -> Int:
    fun change() -> String:
        values = [null]
        return "Ada"
    return when values[0]:
        case change():
            values[0].length
        else:
            0
"#,
    )
    .unwrap_err();
    assert!(error.contains("nullable"), "{error}");
}
