//! Statically selected overloads must retain only their own deferred reads.

use super::compile;

#[test]
fn known_literal_calls_and_default_ties_skip_unselected_overload_captures() {
    for source in [
        r#"scene start():
    val future: Int
    fun read(value: Int) -> Int = value
    fun read(value: Float) -> Int = future
    fun read(value: String) -> Int = future
    fun integer() -> Int = read(7)
    fun floating() -> Int = read(0.5)
    val first = integer()
    future = 9
    val second = floating()
"#,
        r#"scene start():
    val future: Int
    fun read(value: Int = 7) -> Int = value
    fun read(vararg values: String) -> Int = future
    fun wrapper() -> Int = read()
    val result = wrapper()
    future = 9
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{error}\n{source}"));
    }
}

#[test]
fn changing_literal_profiles_cannot_reuse_an_unrelated_empty_capture_proof() {
    let source = r#"scene start():
    val future: Int
    fun read(value: Int) -> Int = value
    fun read(value: Float) -> Int = future
    fun integer() -> Int = read(7)
    fun floating() -> Int = read(0.5)
    val first = integer()
    val second = floating()
    future = 9
"#;
    let error = compile(source).expect_err("the selected Float overload still reads the future binding");
    assert!(error.contains("initialized"), "{error}");
}
