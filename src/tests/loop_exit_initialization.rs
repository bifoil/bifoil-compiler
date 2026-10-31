//! Loop flow can prove an enclosing val is written before an unconditional exit.

use super::compile;

#[test]
fn a_for_body_can_initialize_an_outer_val_before_breaking() {
    compile(
        r#"scene start():
    val result: Int
    for ordinal in 1..3:
        initialize(ordinal)
        break
    fun initialize(value: Int):
        result = value
    val observed = result
"#,
    )
    .unwrap();
}

#[test]
fn an_always_entered_while_body_can_initialize_before_breaking() {
    compile(
        r#"scene start():
    val result: Int
    fun initialize(value: Int):
        result = value
    while true:
        initialize(3)
        break
    val observed = result
"#,
    )
    .unwrap();
}

#[test]
fn a_posttest_body_with_a_false_condition_can_initialize_once() {
    for transfer in ["", "        continue\n"] {
        let source = format!(
            "scene start():\n    val result: Int\n    fun initialize(value: Int):\n        result = value\n    do:\n        initialize(3)\n{transfer}    while false\n    val observed = result\n"
        );
        compile(&source).unwrap();
    }
}

#[test]
fn a_labeled_break_keeps_the_enclosing_single_initialization_proof() {
    compile(
        r#"scene start():
    val result: Int
    outer@ for first in 1..3:
        for second in 1..3:
            initialize(first + second)
            break@outer
    fun initialize(value: Int):
        result = value
    val observed = result
"#,
    )
    .unwrap();
}

#[test]
fn prose_directives_preserve_exit_and_interpolation_effect_order() {
    compile(
        r#"scene start():
    val result: Int
    fun initialize() -> Int:
        result = 3
        return result
    prose:
        @while true:
            ${initialize()}
            @break
    val observed = result
"#,
    )
    .unwrap();
}

#[test]
fn reaching_a_backedge_after_writing_an_outer_val_is_still_rejected() {
    for transfer in ["", "        continue\n"] {
        let source = format!(
            "scene start():\n    val result: Int\n    fun initialize(value: Int):\n        result = value\n    for ordinal in 1..3:\n        initialize(ordinal)\n{transfer}"
        );
        assert!(compile(&source).is_err(), "{source}");
    }
}

#[test]
fn repeated_while_conditions_cannot_claim_a_single_initializer_call() {
    let error = compile(
        r#"scene start():
    val result: Int
    fun initialize() -> Boolean:
        result = 3
        return true
    while initialize():
        break
"#,
    )
    .map(|_| ())
    .expect_err("condition invocation needs a separate repeat proof");
    assert!(error.contains("captured val"), "{error}");
}
