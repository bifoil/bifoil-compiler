//! Forward compact calls require existing cells and preserve declaration-time checking.

use super::compile;

#[test]
fn forward_compact_calls_initialize_before_their_declaration() {
    compile(
        r#"scene start():
    val first: Int
    val second: Int
    initialize()
    val observed = first + second
    fun initialize():
        first = 3
        second = first + 2
    prose:
        ${observed}|${first}|${second}
"#,
    )
    .unwrap();
}

#[test]
fn forward_expansion_does_not_reset_the_callers_immutable_flow() {
    for declaration_first in [true, false] {
        let declaration = "    fun initialize():\n        result = 3\n";
        let write = "    result = 4\n";
        let middle = if declaration_first {
            format!("{declaration}{write}")
        } else {
            format!("{write}{declaration}")
        };
        let source = format!("scene start():\n    val result: Int\n    initialize()\n{middle}");
        let error = compile(&source).expect_err("the actual caller cell was already written");
        assert!(error.contains("initialized val"), "{source}\n{error}");
    }
}

#[test]
fn forward_calls_still_require_created_cells_and_a_single_source_use() {
    for source in [
        r#"scene start():
    initialize()
    val result: Int
    fun initialize():
        result = 3
"#,
        r#"scene start():
    val first: Int
    initialize()
    val second: Int
    fun initialize():
        first = 3
        second = 5
"#,
        r#"scene start():
    val result: Int
    initialize()
    fun initialize():
        result = 3
    initialize()
"#,
    ] {
        let error =
            compile(source).expect_err("forward visibility does not create cells or prove repeated writes safe");
        assert!(
            error.contains("initialized") || error.contains("captured val"),
            "{source}\n{error}"
        );
    }
}

#[test]
fn forward_calls_keep_value_types_and_definition_scope_rules() {
    for (source, diagnostic) in [
        (
            r#"scene start():
    val result: Int
    initialize()
    fun initialize():
        result = "wrong"
"#,
            "expected Int, found String",
        ),
        (
            r#"scene start():
    val result: Int
    initialize()
    val later = 3
    fun initialize():
        result = later
"#,
            "unknown name 'later'",
        ),
    ] {
        let error = compile(source).expect_err("expansion cannot bypass the real call's types or visibility");
        assert!(error.contains(diagnostic), "{source}\n{error}");
    }
}
