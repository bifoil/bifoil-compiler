//! Scalar expressions, calls, and effects using original Bifoil programs.

use super::compile;

#[test]
fn constant_failures_and_nesting_have_diagnostics() {
    assert!(
        compile(
            r#"fun bad() = 2147483647 + 1
"#
        )
        .unwrap_err()
        .contains("overflow")
    );
    assert!(
        compile(
            r#"fun bad() = 101 %+ 1
"#
        )
        .unwrap_err()
        .contains("fairmath base")
    );
    assert!(
        compile(
            r#"fun bad() = 1 / 0
"#
        )
        .unwrap_err()
        .contains("division by zero")
    );
    assert!(
        compile(&format!(
            r#"fun bad() = 1{}
"#,
            "+1".repeat(100)
        ))
        .unwrap_err()
        .contains("budget")
    );
    assert!(
        compile(&format!(
            r#"fun bad(value: {}Int{}):
    return
"#,
            "() -> ".repeat(100),
            ""
        ))
        .unwrap_err()
        .contains("budget")
    );
}
#[test]
fn minimum_integer_and_fairmath() {
    compile(
        r#"export fun minimum() -> Int = -2147483648
export fun fair(value: Int) -> Int = (value %+ 20) + 1
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"fun bad() = 2147483648
"#
        )
        .is_err()
    );
    assert!(
        compile(
            r#"fun bad(value: Int) = value %+ 20 + 1
"#
        )
        .is_err()
    );
}
#[test]
fn immutable_and_boolean_checks() {
    assert!(
        compile(
            r#"scene start():
    val score = 1
    score = 2
"#
        )
        .unwrap_err()
        .contains("cannot reassign")
    );
    assert!(
        compile(
            r#"scene start():
    if 1:
        return
"#
        )
        .unwrap_err()
        .contains("Boolean")
    );
}
#[test]
fn defaults_and_effects() {
    compile(
        r#"export fun add(value: Int, amount: Int = value) -> Int = value + amount
scene start():
    val result = add(amount = 2, value = 3)
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"suspend fun ask() -> Int = 1
fun bad() -> Int = ask()
"#
        )
        .unwrap_err()
        .contains("suspend context")
    );
    assert!(
        compile(
            r#"fun add(value: Int) = value
scene start():
    add()
"#
        )
        .is_err()
    );
}
#[test]
fn null_literal_fallback_uses_only_the_present_result_type() {
    compile(
        r#"fun value() -> Int = null ?: 3
"#,
    )
    .unwrap();
    compile(
        r#"fun value() -> Int = null ?: if true:
    3
else:
    4
"#,
    )
    .unwrap();
    compile(
        r#"fun value() -> Int:
    return null ?: return 3
"#,
    )
    .unwrap();
}
#[test]
fn safe_method_results_remain_nullable_and_bound_references_check_access() {
    compile(
        r#"data class Box:
    fun read() -> Int = 1
fun valid(value: Box?) -> Int = value?.read() ?: 0
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"data class Box:
    fun read() -> Int = 1
fun invalid(value: Box?) -> Int = value?.read()
"#
        )
        .unwrap_err()
        .contains("expected Int, found Int?")
    );
    assert!(
        compile(
            r#"data class Box:
    val callback: () -> Int
fun invalid(value: Box?) = value::callback
"#
        )
        .unwrap_err()
        .contains("nullable")
    );
    assert!(
        compile(
            r#"data class Box:
    private val callback: () -> Int
fun invalid(value: Box) = value::callback
"#
        )
        .unwrap_err()
        .contains("private")
    );
    compile(
        r#"fun <T: Map<String, Int>> hasKey(items: T) -> Boolean = "key" in items
"#,
    )
    .unwrap();
}
#[test]
fn suspended_defaults_follow_the_declared_function_effect() {
    compile(
        r#"suspend fun ask() -> Int:
    choice:
        option "Continue":
            return 3
suspend fun value(first: Int, second: Int = ask() + first) -> Int = second
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"suspend fun ask() -> Int:
    choice:
        option "Continue":
            return 3
fun invalid(value: Int = ask()) -> Int = value
"#
        )
        .unwrap_err()
        .contains("suspend context")
    );
}
#[test]
fn supplied_arguments_keep_default_skipping_paths_reachable() {
    for (source, diagnostic) in [
        (
            r#"fun never() -> Nothing:
    while true:
        continue
fun read(ignored: Int = never()) -> Int:
    val value: Int
    return value
scene start():
    return
"#,
            "initialized",
        ),
        (
            r#"fun length(name: String?, ignored: Int =
    if name == null:
        return 0
    else:
        1
) -> Int:
    return name.length
scene start():
    return
"#,
            "nullable",
        ),
    ] {
        let error = compile(source).expect_err(source);
        assert!(error.contains(diagnostic), "{error}");
    }
}
