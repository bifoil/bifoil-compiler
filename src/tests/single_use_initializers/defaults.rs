//! Defaults retain selected evaluation, entry reads, and immutable-write checks.

use super::compile;

#[test]
fn parameter_helper_default_returns_cannot_skip_writes() {
    let error = compile(
        r#"scene start():
    val x: Int
    fun initialize(value: Int = (if true:
        return
    else:
        3
    )):
        x = value
    initialize()
"#,
    )
    .expect_err("a default cannot return before its protected write");
    assert!(error.contains("captured val"), "{error}");
}

#[test]
fn effectful_default_calls_keep_body_writes_and_results_checked() {
    for source in [
        r#"fun record() -> Int = 3
scene start():
    val result: Int
    fun initialize(value: Int = record()):
        result = value
    initialize()
    prose:
        ${result}
"#,
        r#"fun record() -> Int = 3
scene start():
    val result: Int
    fun initialize(value: Int = record()) -> Int:
        result = value
        return value
    val status = initialize()
    prose:
        ${result}|${status}
"#,
        r#"fun values() -> List<Int> = [3]
scene start():
    val result: List<Int>
    fun initialize(items: List<Int> = values()):
        result = items
    initialize()
    prose:
        ${result[0]}
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn supplied_defaults_skip_their_uninitialized_reads() {
    compile(
        r#"scene start():
    val result: Int
    fun read() -> Int = result
    fun initialize(value: Int = read()):
        result = value
    initialize(7)
    prose:
        ${result}
"#,
    )
    .unwrap();
}

#[test]
fn effectful_defaults_keep_real_reads_and_double_writes_restricted() {
    for source in [
        r#"scene start():
    val result: Int
    val reader: () -> Int = () => result
    fun initialize(value: Int = reader()):
        result = value
    initialize()
"#,
        r#"scene start():
    val result: Int
    fun read() -> Int = result
    fun initialize(value: Int = read()):
        result = value
    initialize()
"#,
        r#"scene start():
    val result: Int
    fun initialize(value: Int = (if true:
        result = 1
        2
    else:
        3
    )):
        result = value
    initialize()
"#,
    ] {
        let error = compile(source).expect_err("default effects cannot bypass readiness or immutable writes");
        assert!(
            error.contains("not initialized") || error.contains("cannot reassign"),
            "{source}\n{error}"
        );
    }
}
