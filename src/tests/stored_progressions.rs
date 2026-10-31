//! Immutable stored progression values retain their construction evidence.

use super::compile;

#[test]
fn immutable_progressions_keep_nonempty_facts_through_aliases() {
    compile(
        r#"scene start():
    val progression = 1..5 step 2
    val alias = progression
    var result: Int
    for ordinal in alias:
        result = ordinal
    val observed = result
"#,
    )
    .unwrap();
}

#[test]
fn escaping_named_readers_keep_immutable_progression_evidence() {
    compile(
        r#"fun make() -> () -> Int:
    val progression = 5 downTo 1 step 2
    fun read() -> Int:
        var result: Int
        for ordinal in progression:
            result = ordinal
        return result
    return ::read
scene start():
    val reader = make()
    val observed = reader()
"#,
    )
    .unwrap();
}

#[test]
fn immutable_value_evidence_survives_explicit_universal_and_nullable_annotations() {
    compile(
        r#"scene start():
    val erased: Any = 1..5 step 2
    if erased is IntProgression:
        val alias = erased
        var first: Int
        for ordinal in alias:
            first = ordinal
        val observed = first
    val nullable: IntProgression? = 5 downTo 1 step 2
    if nullable != null:
        var second: Int
        for ordinal in nullable:
            second = ordinal
        val other = second
"#,
    )
    .unwrap();
}

#[test]
fn unknown_inputs_mutable_aliases_and_reused_names_do_not_inherit_value_evidence() {
    for setup in [
        "    val progression = input\n",
        "    val known = 1..5 step 2\n    var progression = known\n    progression = input\n",
        "    if true:\n        val progression = 1..5 step 2\n        val unused = progression\n    val progression = input\n",
    ] {
        let source = format!(
            "fun run(input: IntProgression) -> Int:\n{setup}    var result: Int\n    for ordinal in progression:\n        result = ordinal\n    return result\nscene start():\n    Unit\n"
        );
        let error = compile(&source)
            .map(|_| ())
            .expect_err("only this immutable value's construction supplies evidence");
        assert!(error.contains("not definitely initialized"), "{source}\n{error}");
    }
}

#[test]
fn nonempty_value_evidence_does_not_grant_untyped_iteration() {
    let error = compile(
        r#"scene start():
    val erased: Any = 1..5 step 2
    var result: Int
    for ordinal in erased:
        result = ordinal
"#,
    )
    .map(|_| ())
    .expect_err("Any still needs a checked iterable type");
    assert!(error.contains("for requires an iterable"), "{error}");
}
