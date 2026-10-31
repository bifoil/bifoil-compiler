//! Generic headers retain definition checking, specialization, and one invocation.

use super::compile;

#[test]
fn generic_helpers_keep_definition_checked_write_contracts() {
    for source in [
        r#"fun <T> outer(item: T) -> T:
    val result: T
    fun <U: T> initialize(value: U):
        result = value
    initialize<T>(item)
    return result
"#,
        r#"fun run() -> Int:
    val result: Int
    fun <T: Equatable> initialize(value: T, actual: Int):
        if value == value:
            result = actual
        else:
            result = 0
    initialize("hello", 3)
    return result
"#,
        r#"scene start():
    val result: Any?
    fun <T> initialize(value: T):
        result = value
    initialize<Int?>(null)
"#,
        r#"scene start():
    val result: Int
    fun <T> initialize():
        result = 3
    initialize<String>()
    prose:
        ${result}
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn generic_helper_use_inventory_includes_specializations_and_references() {
    for uses in [
        "initialize<Int>(3)\n    initialize<String>(\"other\")",
        "val saved: (Int) -> Unit = ::initialize<Int>\n    initialize(3)",
        "initialize<Unit>(initialize<Int>(3))",
    ] {
        let source = format!(
            "scene start():\n    val result: Any?\n    fun <T> initialize(value: T):\n        result = value\n    {uses}\n"
        );
        let error = compile(&source).expect_err("specializations cannot evade the single invocation proof");
        assert!(error.contains("captured val"), "{source}\n{error}");
    }
}

#[test]
fn generic_helper_proof_keeps_bounds_body_and_argument_checks() {
    for (source, diagnostic) in [
        (
            r#"scene start():
    val result: Int
    fun initialize():
        result = 3
    initialize<Int>()
"#,
            "nongeneric callable does not accept type arguments",
        ),
        (
            r#"scene start():
    val result: Int
    fun <T: Any> initialize(value: T):
        result = 3
    initialize<Int?>(null)
"#,
            "does not satisfy Any",
        ),
        (
            r#"scene start():
    val result: Int
    fun <T> initialize(value: T):
        result = value
    initialize(3)
"#,
            "expected Int, found T",
        ),
        (
            r#"scene start():
    val result: Int
    fun <T> initialize(value: T):
        result = 3
    initialize<Int>((if true:
        result = 1
        2
    else:
        3
    ))
"#,
            "possible earlier initialization",
        ),
    ] {
        let error = compile(source).expect_err("generic helpers keep ordinary validation");
        assert!(error.contains(diagnostic), "{source}\n{error}");
    }
}
