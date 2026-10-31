//! Result contexts preserve valid arguments and non-null generic identities.

use super::compile;

#[test]
fn wider_result_contexts_do_not_break_satisfied_generic_bounds() {
    compile(
        r#"fun <U: Any> present(value: U) -> U = value
fun <U: Equatable> equalValue(value: U) -> U = value
fun <U: Int> integer(value: U) -> U = value
fun optional() -> Int? = present(3)
fun erased() -> Any = equalValue("text")
fun widened() -> Float = integer(3)
scene start():
    Unit
"#,
    )
    .unwrap();
}

#[test]
fn nonnull_bounds_preserve_guarded_original_binding_and_field_identities() {
    compile(
        r#"data class Box<T>:
    val value: T
fun <U: Any> present(value: U) -> U = value
fun <T> preserve(value: T) -> T:
    if value is String:
        return present(value)
    return value
fun <T> field(holder: Box<T>) -> T:
    if holder.value is String:
        return present(holder.value)
    return holder.value
fun <T> asserted(value: T) -> T = present(value!!)
scene start():
    Unit
"#,
    )
    .unwrap();
}

#[test]
fn nonnull_contextual_arguments_still_require_a_valid_presence_and_identity_proof() {
    for source in [
        r#"fun <U: Any> present(value: U) -> U = value
fun <T> invalid(value: T) -> T = present(value)
"#,
        r#"fun <U: Any> present(value: U) -> U = value
fun <T> invalid(value: T) -> T:
    if value is String:
        return present(value + "changed")
    return value
"#,
        r#"data class Box<T>:
    var value: T
fun <U: Any> present(value: U) -> U = value
fun <T> invalid(holder: Box<T>, replacement: T) -> T:
    if holder.value is String:
        holder.value = replacement
        return present(holder.value)
    return holder.value
"#,
    ] {
        let source = format!("{source}scene start():\n    Unit\n");
        let error = compile(&source).expect_err("a context cannot fabricate a stable non-null identity");
        assert!(error.contains("argument requires"), "{error}\n{source}");
    }
}
