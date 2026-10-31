//! Grouping a discarded call does not change its single-invocation proof.

use super::compile;

#[test]
fn grouped_compact_calls_work_before_or_after_their_definition() {
    for call in ["(initialize())", "(initialize)()", "(((initialize))())"] {
        for definition_first in [false, true] {
            let definition = "    fun initialize():\n        first = 3\n        second = first + 2\n";
            let call = format!("    {call}\n");
            let middle = if definition_first {
                format!("{definition}{call}")
            } else {
                format!("{call}{definition}")
            };
            let source = format!(
                "scene start():\n    val first: Int\n    val second: Int\n{middle}    val observed = first + second\n"
            );
            compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
        }
    }
}

#[test]
fn grouped_forward_calls_keep_type_visibility_and_cell_creation_checks() {
    for (source, diagnostic) in [
        (
            r#"scene start():
    (initialize)()
    val result: Int
    fun initialize():
        result = 3
"#,
            "cannot prove captured binding",
        ),
        (
            r#"scene start():
    val result: Int
    (initialize())
    val later = 3
    fun initialize():
        result = later
"#,
            "unknown name 'later'",
        ),
        (
            r#"scene start():
    val result: Int
    ((initialize)())
    fun initialize():
        result = "wrong"
"#,
            "expected Int, found String",
        ),
    ] {
        let error = compile(source).expect_err("grouping cannot bypass a call-site guard");
        assert!(error.contains(diagnostic), "{source}\n{error}");
    }
}

#[test]
fn grouped_calls_keep_repetition_escapes_and_second_writes_rejected() {
    for tail in [
        "    (initialize())\n",
        "    val escaped = ::initialize\n",
        "    result = 4\n",
    ] {
        let source = format!(
            "scene start():\n    val result: Int\n    (initialize)()\n    fun initialize():\n        result = 3\n{tail}"
        );
        let error = compile(&source).expect_err("parentheses do not create a fresh immutable cell");
        assert!(
            error.contains("captured val") || error.contains("initialized val"),
            "{source}\n{error}"
        );
    }
}
