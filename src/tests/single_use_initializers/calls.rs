//! Typed parameter calls retain argument ordering and protected-cell checks.

use super::compile;

#[test]
fn single_use_parameter_helpers_keep_normal_argument_rules() {
    for source in [
        r#"scene start():
    val x: Int
    fun initialize(var value: Int, increment: Int = 1, vararg extras: Int):
        value = value + increment
        x = value + extras[0]
    initialize(3, extras = [2])
    prose:
        ${x}
"#,
        r#"scene start():
    val x: Int
    fun initialize(first: Int = 2, second: Int = first + 1):
        x = first + second
    initialize()
    prose:
        ${x}
"#,
        r#"fun <T> identity(value: T) -> T:
    val result: T
    fun initialize(item: T):
        result = item
    initialize(value)
    return result
"#,
        r#"scene start():
    for i in 1..3:
        val x: Int
        fun initialize(value: Int):
            x = value
        initialize(i)
        prose:
            ${x}
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn parameter_helper_use_proofs_include_argument_expressions() {
    for source in [
        r#"scene start():
    val x: Int
    fun initialize(value: Unit):
        x = 1
    initialize(initialize(Unit))
"#,
        r#"scene start():
    val x: Int
    fun initialize(value: Int):
        x = value
    initialize(3)
    initialize(4)
"#,
        r#"scene start():
    val x: Int
    fun initialize(value: Int):
        x = value
    val reference = ::initialize
    initialize(3)
"#,
    ] {
        let error = compile(source).expect_err("a live helper cannot escape or acquire a second call");
        assert!(error.contains("captured val"), "{error}");
    }
}

#[test]
fn parameter_helper_arguments_cannot_initialize_the_target_first() {
    let error = compile(
        r#"scene start():
    val x: Int
    fun initialize(value: Int):
        x = value
    initialize((if true:
        x = 1
        2
    else:
        3
    ))
"#,
    )
    .expect_err("argument effects run before the helper's protected write");
    assert!(error.contains("possible earlier initialization"), "{error}");
}

#[test]
fn assignment_call_sites_retain_targets_results_and_real_frames() {
    for source in [
        r#"scene start():
    val result: Int
    fun initialize():
        result = 3
    (initialize)()
    prose:
        ${result}
"#,
        r#"scene start():
    val result: Int
    val status: Unit
    fun initialize():
        result = 3
    status = (initialize())
    prose:
        ${result}|${status == Unit}
"#,
        r#"scene start():
    val result: Int
    var status = "pending"
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
    status = initialize(3)
    prose:
        ${result}|${status}
"#,
        r#"scene start():
    val result: Int
    val status: String
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
    status = initialize(3)
    prose:
        ${result}|${status}
"#,
        r#"scene start():
    val result: Int
    val status: Unit
    fun initialize():
        result = 3
    status = initialize()
    prose:
        ${result}|${status == Unit}
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn return_call_sites_keep_initialized_cells_alive() {
    for source in [
        r#"fun <T> grouped(input: T) -> () -> T:
    val result: T
    fun <U: T> initialize(value: U) -> () -> T:
        result = value
        return () => result
    return (initialize<T>(input))
"#,
        r#"fun run() -> Int:
    val result: Int
    fun initialize(value: Int) -> Int:
        result = value
        return value
    return initialize(3)
"#,
        r#"fun <T> make(input: T) -> () -> T:
    val result: T
    fun <U: T> initialize(value: U) -> () -> T:
        result = value
        return () => result
    return initialize<T>(input)
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn assignment_calls_recheck_the_protected_target_and_argument_effects() {
    for (source, diagnostic) in [
        (
            r#"scene start():
    val result: Int
    fun initialize():
        result = 3
    (initialize<Int>())
"#,
            "nongeneric callable does not accept type arguments",
        ),
        (
            r#"scene start():
    val result: Int
    fun initialize() -> Int:
        result = 3
        return 4
    result = initialize()
"#,
            "cannot reassign initialized val",
        ),
        (
            r#"scene start():
    val result: Int
    var status = "pending"
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
    status = initialize((if true:
        result = 1
        2
    else:
        3
    ))
"#,
            "possible earlier initialization",
        ),
        (
            r#"scene start():
    val result: Int
    var status = 0
    fun initialize() -> String:
        result = 3
        return "ready"
    status = initialize()
"#,
            "expected Int, found String",
        ),
    ] {
        let error = compile(source).expect_err("target and argument rules remain active around the real call");
        assert!(error.contains(diagnostic), "{source}\n{error}");
    }
}

#[test]
fn mixed_call_sites_cannot_hide_another_invocation_or_reference() {
    for usage in [
        "val first = initialize()\n    status = initialize()",
        "val saved = ::initialize\n    status = initialize()",
    ] {
        let source = format!(
            "scene start():\n    val result: Int\n    var status = \"pending\"\n    fun initialize() -> String:\n        result = 3\n        return \"ready\"\n    {usage}\n"
        );
        let error = compile(&source).expect_err("extra calls or references cannot qualify");
        assert!(error.contains("captured val"), "{source}\n{error}");
    }
}
