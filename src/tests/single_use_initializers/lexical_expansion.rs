//! Compact lowering must use the helper's lexical scope and checked value types.

use super::compile;
use crate::model::{Op, Type};

#[test]
fn compact_calls_cannot_read_a_call_site_local_in_place_of_a_later_outer_binding() {
    for declaration in ["val hidden = 7", "val hidden: Int"] {
        let source = format!(
            "scene start():\n    val result: Int\n    if true:\n        val hidden = 5\n        initialize()\n    {declaration}\n    fun initialize():\n        result = hidden\n    prose:\n        ${{result}}\n"
        );
        let error = compile(&source)
            .map(|_| ())
            .expect_err("the real captured cell does not yet exist at the call");
        assert!(error.contains("hidden"), "{source}\n{error}");
    }
}

#[test]
fn compact_calls_cannot_change_literal_inference_using_the_callers_type_proofs() {
    let module = compile(
        r#"scene start():
    val result: Any
    var source: Any = 1
    fun initialize():
        result = [source, 2.0]
    if source is Int:
        initialize()
"#,
    )
    .unwrap();
    let types = module
        .functions
        .values()
        .flat_map(|function| &function.code)
        .filter_map(|instruction| match &instruction.op {
            Op::BeginCollection { ty } => Some(ty),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(types.len(), 1);
    assert!(matches!(types[0], Type::Named { name, args } if name == "List" && args.as_ref() == [Type::Any]));
}

#[test]
fn general_pure_reads_retain_their_checked_declaration_types() {
    for (declarations, guard, expression, family, argument) in [
        (
            "var source: String? = \"ready\"",
            "source != null",
            "[source]",
            "List",
            Type::Nullable {
                inner: Box::new(Type::String),
            },
        ),
        (
            "val values: List<Any> = [1]",
            "values[0] is Int",
            "[values[0], 2.0]",
            "List",
            Type::Any,
        ),
        (
            "var source: Any = 1",
            "source is Int",
            "{\"value\": source}",
            "Map",
            Type::Any,
        ),
        (
            "var source: String? = \"ready\"",
            "source != null",
            "{source}",
            "Set",
            Type::Nullable {
                inner: Box::new(Type::String),
            },
        ),
    ] {
        let source = format!(
            "scene start():\n    val result: Any\n    {declarations}\n    fun initialize():\n        result = {expression}\n    if {guard}:\n        initialize()\n"
        );
        let module = compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
        let helper = module
            .functions
            .values()
            .find(|function| function.signature.id.contains("$initialize@"))
            .unwrap();
        assert!(helper.code.iter().any(|instruction| matches!(&instruction.op, Op::BeginCollection { ty: Type::Named { name, args } } if name == family && args.last() == Some(&argument))), "{source}");
    }
}

#[test]
fn compact_target_names_cannot_resolve_to_call_site_locals() {
    let error = compile(
        r#"scene start():
    val result: Int
    if true:
        var hidden = 5
        initialize()
    var hidden = 7
    fun initialize():
        result = 3
        hidden = 8
"#,
    )
    .map(|_| ())
    .expect_err("the captured target is unavailable at the real call");
    assert!(error.contains("hidden"), "{error}");
}

#[test]
fn retained_pure_frames_accept_preexisting_values_and_nullable_targets() {
    compile(
        r#"scene start():
    val source = 3
    val result: Int?
    initialize()
    fun initialize():
        result = source
    val observed = result ?: 0
"#,
    )
    .unwrap();
}

#[test]
fn compact_key_writes_invalidate_proofs_in_nested_caller_scopes() {
    let error = compile(
        r#"scene start():
    var key: Int
    val result: Int
    fun initialize():
        key = 1
        result = 3
    if true:
        val mapping: Map<Int, String?> = {0: "ready", 1: null}
        key = 0
        if mapping[key] != null:
            initialize()
            val invalid = mapping[key].length
"#,
    )
    .map(|_| ())
    .expect_err("changing a lookup key invalidates the caller's earlier lookup");
    assert!(error.contains("nullable"), "{error}");
}

#[test]
fn compact_named_references_retain_their_established_lexical_cells() {
    compile(
        r#"fun makeReader(value: Int) -> () -> Int:
    val captured: Int
    val reader: () -> Int
    fun initialize():
        captured = value
        reader = ::read
    fun read() -> Int = captured
    initialize()
    return reader
scene start():
    val reader = makeReader(9)
    val observed = reader()
"#,
    )
    .unwrap();
    let error = compile(
        r#"scene start():
    val captured: Int
    val reader: () -> Int
    if true:
        val hidden = 5
        initialize()
    val hidden = 7
    fun initialize():
        captured = 3
        reader = ::read
    fun read() -> Int = hidden
"#,
    )
    .map(|_| ())
    .expect_err("a named reader cannot make a later captured cell exist earlier");
    assert!(error.contains("hidden"), "{error}");
}
