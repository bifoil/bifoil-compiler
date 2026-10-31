//! Returned values and expression bodies retain ordinary result and capture checks.

use super::compile;

#[test]
fn returning_helpers_keep_real_calls_and_result_checks() {
    for source in [
        r#"fun <T> make(input: T) -> () -> T:
    val result: T
    fun <U: T> initialize(value: U) -> () -> T:
        result = value
        return () => result
    val saved = initialize<T>(input)
    return saved
"#,
        r#"scene start():
    val result: Int
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
    val status = initialize(3)
    prose:
        ${result}|${status}
"#,
        r#"scene start():
    val result: Int
    fun initialize():
        result = 3
    val status: Unit = initialize()
    prose:
        ${result}|${status == Unit}
"#,
        r#"scene start():
    val result: Int
    fun initialize() -> String:
        result = 3
        return "ready"
    initialize()
    prose:
        ${result}
"#,
        r#"scene start():
    val result: Int
    fun initialize(value: Int):
        result = value
        return "ready"
    val status = initialize(3)
    prose:
        ${result}|${status}
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn returning_helpers_keep_every_return_and_single_invocation_checks() {
    for source in [
        r#"scene start():
    val result: Int
    fun initialize(flag: Boolean) -> String:
        if flag:
            return "skipped"
        result = 3
        return "ready"
    val status = initialize(true)
"#,
        r#"scene start():
    val result: Int
    fun initialize() -> String:
        result = 3
        return "ready"
    val first = initialize()
    val second = initialize()
"#,
        r#"scene start():
    val result: Int
    fun initialize() -> String:
        result = 3
        return "ready"
    val saved = ::initialize
    val status = initialize()
"#,
    ] {
        let error = compile(source).expect_err("a returned value cannot bypass the protected write proof");
        assert!(error.contains("captured val"), "{source}\n{error}");
    }
}

#[test]
fn returning_helpers_keep_argument_writes_and_result_conversions_checked() {
    for (source, diagnostic) in [
        (
            r#"scene start():
    val result: Int
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
    val status: Int = initialize(3)
"#,
            "expected Int, found String",
        ),
        (
            r#"scene start():
    val result: Int
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
    val status = initialize((if true:
        result = 1
        2
    else:
        3
    ))
"#,
            "possible earlier initialization",
        ),
    ] {
        let error = compile(source).expect_err("normal call and variable rules remain in force");
        assert!(error.contains(diagnostic), "{source}\n{error}");
    }
}

#[test]
fn expression_helpers_use_the_same_completed_write_proof() {
    for source in [
        r#"scene start():
    val result: Int
    fun initialize(value: Int) = if value >= 0:
        result = value
        "positive"
    else:
        result = -value
        "negative"
    val status = initialize(3)
    prose:
        ${result}|${status}
"#,
        r#"fun <T> outer(input: T) -> T:
    val result: T
    fun <U: T> initialize(value: U) = if true:
        result = value
        Unit
    else:
        result = value
        Unit
    val status: Unit = initialize<T>(input)
    return result
"#,
        r#"scene start():
    val result: Int
    fun initialize(value: Int) = when value > 0:
        case true:
            result = value
            () => result
        else:
            result = -value
            () => result
    val reader = initialize(3)
    prose:
        ${reader()}
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn expression_helpers_keep_all_branches_and_call_uses_checked() {
    for source in [
        r#"scene start():
    val result: Int
    fun initialize(flag: Boolean) = if flag:
        result = 3
        "ready"
    else:
        "skipped"
    val status = initialize(true)
"#,
        r#"scene start():
    val result: Int
    fun initialize(flag: Boolean) = if flag:
        return "skipped"
    else:
        result = 3
        "ready"
    val status = initialize(false)
"#,
        r#"scene start():
    val result: Int
    fun initialize(value: Int) = if true:
        result = value
        "ready"
    else:
        result = 0
        "ready"
    val first = initialize(3)
    val second = initialize(4)
"#,
    ] {
        let error = compile(source).expect_err("expression syntax cannot bypass mandatory writes or one use");
        assert!(error.contains("captured val"), "{source}\n{error}");
    }
}

#[test]
fn expression_helpers_retain_annotated_result_checking() {
    let error = compile(
        r#"scene start():
    val result: Int
    fun initialize(value: Int) -> String = if true:
        result = value
        1
    else:
        result = 0
        2
    val status = initialize(3)
"#,
    )
    .expect_err("an expression result still must satisfy its annotation");
    assert!(error.contains("branch requires String, found Int"), "{error}");
}
