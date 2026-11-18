//! Scalar expressions, calls, and effects using original Bifoil programs.

use super::compile;
use crate::syntax::{Expr, StatementKind};

fn parsed_expression(source: &str) -> Expr {
    let mut parsed = crate::parser::parse(&format!("fun literal_value() = {source}\n"), "src/literals.bfl").unwrap();
    let statement = parsed.functions.remove(0).body.remove(0);
    match statement.kind {
        StatementKind::Expression(value) | StatementKind::Return(Some(value)) => value,
        _ => panic!("expression fixture lost its function body"),
    }
}

#[test]
fn numeric_literal_prefixes_preserve_radix_and_reject_unrepresentable_values() {
    for (source, expected) in [("0x2A", 42), ("0X2a", 42), ("0b1001", 9), ("0B10_01", 9)] {
        assert!(
            matches!(parsed_expression(source), Expr::Int(value) if value == expected),
            "{source}"
        );
        compile(&format!("fun literal_value() = {source}\n")).unwrap();
    }
    assert!(matches!(parsed_expression("1e3"), Expr::Float(value) if value == 1000.0));
    for (source, message) in [
        ("0x8000000000000000".to_owned(), "invalid hexadecimal Int"),
        (format!("0b1{}", "0".repeat(63)), "invalid binary Int"),
        ("1e999".to_owned(), "non-finite Float literal"),
    ] {
        assert!(
            crate::parser::parse(&format!("fun literal_value() = {source}\n"), "src/literals.bfl")
                .unwrap_err()
                .contains(message),
            "{source}"
        );
    }
}

#[test]
fn character_literals_preserve_one_unicode_scalar_and_reject_interpolation() {
    assert!(matches!(parsed_expression("'😀'"), Expr::Char(value) if value == "😀"));
    for (source, message) in [
        ("'${1}'", "Char cannot interpolate"),
        ("''", "Char must contain exactly one Unicode scalar"),
        ("'ab'", "Char must contain exactly one Unicode scalar"),
    ] {
        assert!(
            crate::parser::parse(&format!("fun literal_value() = {source}\n"), "src/literals.bfl")
                .unwrap_err()
                .contains(message),
            "{source}"
        );
    }
}

#[test]
fn recursive_prefix_depth_and_unfinished_generic_probes_fail_at_the_existing_boundaries() {
    parsed_expression(&format!("{}1", "- ".repeat(62)));
    assert!(
        crate::parser::parse(
            &format!("fun literal_value() = {}1\n", "- ".repeat(66)),
            "src/literals.bfl"
        )
        .unwrap_err()
        .contains("expression nesting budget exceeded")
    );
    assert!(crate::parser::parse("fun literal_value() = callback<Int>\n", "src/literals.bfl").is_err());
}

#[test]
fn fairmath_on_either_side_of_another_operator_requires_explicit_grouping() {
    for operator in ["%+", "%-"] {
        for expression in [format!("1 == 2 {operator} 3"), format!("1 {operator} 2 + 3")] {
            assert!(
                crate::parser::parse(&format!("fun literal_value() = {expression}\n"), "src/literals.bfl")
                    .unwrap_err()
                    .contains("fairmath combinations require explicit parentheses"),
                "{expression}"
            );
        }
        parsed_expression(&format!("1 == (2 {operator} 3)"));
        parsed_expression(&format!("(1 {operator} 2) + 3"));
    }
}

#[test]
fn transfer_keywords_keep_their_selected_kind_labels_and_destination_expressions() {
    for (source, expected, labelled) in [
        ("break@outer", "break", true),
        ("continue@outer", "continue", true),
        ("goto destination()", "goto", false),
        ("visit destination()", "visit", false),
    ] {
        let Expr::Transfer { kind, label, value } = parsed_expression(source) else {
            panic!("transfer disappeared");
        };
        assert_eq!(kind, expected);
        assert_eq!(label.as_deref(), labelled.then_some("outer"));
        assert_eq!(value.is_some(), !labelled);
        if let Some(value) = value {
            assert!(matches!(*value, Expr::Call(..)));
        }
    }
}

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
