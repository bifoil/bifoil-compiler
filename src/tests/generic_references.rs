//! Bound-valid generic references retain safe variance without adapters.

use super::compile;

#[test]
fn bounded_references_keep_input_arguments_under_wider_return_views() {
    compile(
        r#"fun <U: Equatable> identity(value: U) -> U = value
fun <U: Any> present(value: U) -> U = value
fun <U: Any> first(vararg values: U) -> U = values[0]
fun <T: Any> factory() -> (T) -> T? = ::present
scene start():
    val erased: (String) -> Any = ::identity
    val optional: (String) -> String? = ::present
    val effect: suspend (String) -> Any = ::identity
    val grouped: (vararg String) -> String? = ::first
"#,
    )
    .unwrap();
}

#[test]
fn reference_context_can_refine_only_nullable_results_without_changing_inputs() {
    compile(
        r#"fun <U: Any> never() -> U:
    while true:
        Unit
scene start():
    val selected: () -> String? = ::never
"#,
    )
    .unwrap();
    let error = compile(
        r#"fun <U: Any> present(value: U) -> U = value
scene start():
    val invalid: (String?) -> String? = ::present
"#,
    )
    .expect_err("a reference cannot unwrap nullable input arguments");
    assert!(error.contains("does not satisfy Any"), "{error}");
    for source in [
        "val invalid: (Int) -> Float = ::present",
        "val invalid: (String) -> Unit = ::present",
    ] {
        let source = format!("fun <U: Any> present(value: U) -> U = value\nscene start():\n    {source}\n");
        assert!(
            compile(&source).is_err(),
            "named references must not gain adapters:\n{source}"
        );
    }
}

#[test]
fn newly_compatible_generic_reference_candidates_still_require_unique_resolution() {
    let error = compile(
        r#"fun <U: Equatable> choose(value: U) -> U = value
fun choose(value: String) -> Any = value
scene start():
    val ambiguous: (String) -> Any = ::choose
"#,
    )
    .expect_err("both declarations satisfy the requested function view");
    assert!(error.contains("ambiguous"), "{error}");
}

#[test]
fn specialized_receivers_do_not_restore_stale_short_circuit_guards() {
    for condition in [
        "text != null && (clear()::identity<Int>) is (Int) -> Int",
        "!(text == null || !((clear()::identity<Int>) is (Int) -> Int))",
    ] {
        let source = format!(
            r#"data class Source:
    fun <T> identity(item: T) -> T = item
scene start():
    var text: String? = "safe"
    val source = Source()
    fun clear() -> Source:
        text = null
        return source
    if {condition}:
        prose:
            ${{text.length}}
"#
        );
        let error = compile(&source).expect_err("the receiver cleared the checked binding");
        assert!(error.contains("nullable member receiver"), "{error}");
    }
    compile(
        r#"data class Source:
    fun <T> identity(item: T) -> T = item
scene start():
    val text: String? = "safe"
    val source = Source()
    var calls = 0
    fun observe() -> Source:
        calls += 1
        return source
    if text != null && (observe()::identity<Int>) is (Int) -> Int:
        prose:
            ${text.length}
"#,
    )
    .expect("effects cannot replace an immutable lexical binding");
}

#[test]
fn specialized_receivers_break_when_subject_provenance_until_rechecked() {
    let source = r#"data class Source:
    fun <T> identity(item: T) -> T = item
scene start():
    var text: String? = "true"
    val source = Source()
    fun clear() -> Source:
        text = null
        return source
    when text:
        case "${(clear()::identity<Int>) is (Int) -> Int}":
            prose:
                ${text.length}
"#;
    let error = compile(source).expect_err("the captured subject no longer equals its mutable source");
    assert!(error.contains("nullable member receiver"), "{error}");
    let checked = source.replace(
        "            prose:\n                ${text.length}",
        "            if text != null:\n                prose:\n                    ${text.length}",
    );
    compile(&checked).expect("a later check proves the current value");
}
