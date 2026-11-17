//! Safe field inputs through hidden receiver arguments retain ordinary reads.
use super::compile;
use crate::model::Type;

const HOLDER: &str = r#"data class Holder<T>:
    var value: T?
    var values: List<T>
    var callback: (T) -> String
"#;

#[test]
fn projected_fields_accept_null_empty_covariant_values_and_broad_callbacks() {
    compile(&format!(
        "{HOLDER}{}",
        r#"fun clear(holder: Holder<*>):
    holder.value = null
    holder.values = []
    holder.callback = (item: Any?) => String(item)
scene start():
    Unit
"#
    ))
    .unwrap();
}

#[test]
fn projected_field_reads_keep_upper_views_after_safe_writes() {
    let module = compile(&format!(
        "{HOLDER}{}",
        r#"fun clear(holder: Holder<*>):
    holder.value = null
    return holder.value
"#
    ))
    .unwrap();
    assert_eq!(module.functions["test.clear"].signature.result, Type::Any.nullable());
    let module = compile(
        r#"data class Text<T: String>:
    var value: T?
fun clear(holder: Text<*>):
    holder.value = null
    return holder.value
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.clear"].signature.result, Type::String.nullable());
}

#[test]
fn hidden_field_inputs_do_not_choose_an_argument_or_adapt_callbacks() {
    for assignment in [
        "holder.value = 3",
        "holder.values = [3]",
        "holder.callback = (item: String) => item",
    ] {
        let source = format!("{HOLDER}fun bad(holder: Holder<*>):\n    {assignment}\n");
        assert!(compile(&source).is_err(), "{source}");
    }
    let error =
        compile("data class Fixed<T>:\n    val value: T?\nfun bad(holder: Fixed<*>):\n    holder.value = null\n")
            .unwrap_err();
    assert!(error.contains("cannot assign to val field"), "{error}");
}

#[test]
fn projected_writes_invalidate_mutable_alias_guards() {
    let error = compile(
        r#"data class Box<T>:
    var value: T?
fun bad(original: Box<String>) -> Int:
    val hidden: Box<*> = original
    if original.value != null:
        hidden.value = null
        return original.value.length
    return 0
"#,
    )
    .unwrap_err();
    assert!(error.contains("nullable"), "{error}");
}

#[test]
fn projected_generic_method_bounds_admit_only_proven_bottom_arguments() {
    compile(
        r#"data class Box<T>:
    val value: T
    fun <U: T> unit() -> Int = 1
    fun <U: T?> maybe(input: U) -> Boolean = input == null
fun check(hidden: Box<*>) -> Int:
    val test = hidden.maybe<Nothing?>(null)
    val callback: () -> Int = hidden::unit<Nothing>
    return callback()
"#,
    )
    .unwrap();
}

#[test]
fn projected_generic_method_bounds_do_not_assume_a_hidden_argument() {
    for call in ["hidden.unit<Int>()", "hidden.maybe<Int>(3)"] {
        let source = format!(
            "data class Box<T>:\n    val value: T\n    fun <U: T> unit() -> Int = 1\n    fun <U: T?> maybe(input: U) -> Boolean = input == null\nfun bad(hidden: Box<*>):\n    {call}\n"
        );
        let error = compile(&source).unwrap_err();
        assert!(error.contains("does not satisfy"), "{error}\n{source}");
    }
}
