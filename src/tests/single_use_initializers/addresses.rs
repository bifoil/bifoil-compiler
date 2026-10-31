//! Member/indexed call sites retain target evaluation, reference, and type checks.

use super::compile;

#[test]
fn member_and_indexed_calls_keep_real_captured_writes() {
    for source in [
        r#"data class Box:
    var status: String = "pending"
scene start():
    val result: Int
    val box = Box()
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
    box.status = initialize(3)
    prose:
        ${result}|${box.status}
"#,
        r#"scene start():
    val result: Int
    val items = mutable ["pending"]
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
    items[0] = initialize(3)
    prose:
        ${result}|${items[0]}
"#,
        r#"scene start():
    val result: Int
    val items = mutable {"k": "pending"}
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
    items["k"] = initialize(3)
    prose:
        ${result}|${items["k"]!!}
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn address_targets_cannot_hide_another_invocation_or_reference() {
    for source in [
        r#"scene start():
    val result: Int
    val items = mutable {"k": "pending"}
    fun initialize(value: Int) -> String:
        result = value
        return "k"
    items[initialize(1)] = initialize(2)
"#,
        r#"fun target(callback: () -> String) -> MutableMap<String, String> = mutable {"k": "pending"}
scene start():
    val result: Int
    fun initialize() -> String:
        result = 3
        return "ready"
    target(::initialize)["k"] = initialize()
"#,
    ] {
        let error = compile(source).expect_err("address evaluation is part of the single-use inventory");
        assert!(error.contains("captured val"), "{source}\n{error}");
    }
}

#[test]
fn address_effects_precede_the_protected_call_check() {
    let error = compile(
        r#"scene start():
    val result: Int
    val items = mutable ["pending"]
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
    items[(if true:
        result = 1
        0
    else:
        0
    )] = initialize(3)
"#,
    )
    .expect_err("an index cannot initialize the protected binding before the call");
    assert!(error.contains("possible earlier initialization"), "{error}");
}

#[test]
fn address_calls_keep_result_types_and_container_mutability() {
    for (source, diagnostic) in [
        (
            r#"scene start():
    val result: Int
    val items = mutable [0]
    fun initialize() -> String:
        result = 3
        return "ready"
    items[0] = initialize()
"#,
            "expected Int, found String",
        ),
        (
            r#"scene start():
    val result: Int
    val items: List<String> = ["pending"]
    fun initialize() -> String:
        result = 3
        return "ready"
    items[0] = initialize()
"#,
            "requires a mutable list or map",
        ),
    ] {
        let error = compile(source).expect_err("the write target retains ordinary type and mutability checking");
        assert!(error.contains(diagnostic), "{source}\n{error}");
    }
}
