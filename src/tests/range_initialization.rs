//! Nonempty ranges reach an iteration exit before the loop can finish normally.

use super::compile;

#[test]
fn nonempty_range_body_writes_remain_initialized_after_normal_traversal() {
    compile(
        r#"fun run() -> Int:
    var result: Int
    for ordinal in 1..3:
        result = ordinal
    return result
scene start():
    val observed = run()
"#,
    )
    .unwrap();
}

#[test]
fn nonempty_range_continue_edges_keep_completed_writes() {
    compile(
        r#"fun run() -> Int:
    var result: Int
    for ordinal in 1..3:
        result = ordinal
        continue
    return result
scene start():
    val observed = run()
"#,
    )
    .unwrap();
}

#[test]
fn range_exit_facts_retire_iteration_names_but_keep_captured_cells_alive() {
    compile(
        r#"fun run() -> () -> Int:
    var reader: () -> Int
    for ordinal in 1..3:
        reader = () => ordinal
        continue
    return reader
scene start():
    val reader = run()
    val observed = reader()
"#,
    )
    .unwrap();
}

#[test]
fn range_exit_facts_include_forward_writers_and_outer_labeled_continues() {
    for source in [
        r#"fun run() -> Int:
    var result: Int
    for ordinal in 1..3:
        initialize(ordinal)
    fun initialize(amount: Int) -> Int:
        result = amount
        return result
    return result
scene start():
    val observed = run()
"#,
        r#"fun run() -> Int:
    var result: Int
    outer@ for ordinal in 1..3:
        for other in 1..2:
            result = ordinal
            continue@outer
    return result
scene start():
    val observed = run()
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn possible_zero_iterations_and_unwritten_break_or_continue_exits_remain_uninitialized() {
    for loop_source in [
        "    val empty: List<Int> = []\n    for ordinal in empty:\n        result = ordinal\n",
        "    for ordinal in 1..3:\n        if flag:\n            continue\n        result = ordinal\n",
        "    for ordinal in 1..3:\n        if flag:\n            break\n        result = ordinal\n",
    ] {
        let source = format!(
            "fun run(flag: Boolean) -> Int:\n    var result: Int\n{loop_source}    return result\nscene start():\n    Unit\n"
        );
        let error = compile(&source)
            .map(|_| ())
            .expect_err("every normal iteration exit must establish the value");
        assert!(error.contains("not definitely initialized"), "{source}\n{error}");
    }
}

#[test]
fn retired_iteration_keys_do_not_narrow_a_later_binding_with_the_same_name() {
    let error = compile(
        r#"fun read(values: Map<Int, String?>) -> Int:
    for index in 1..1:
        if values[index] == null:
            return 0
    val index = 0
    return values[index].length
scene start():
    Unit
"#,
    )
    .map(|_| ())
    .expect_err("the later key has its own lexical identity");
    assert!(error.contains("nullable"), "{error}");
}

#[test]
fn guaranteed_range_exits_keep_stable_guards_and_overwrite_old_callback_requirements() {
    compile(
        r#"fun length(values: Map<Int, String?>) -> Int:
    for ordinal in 1..3:
        if values[0] == null:
            return 0
    return values[0].length
fun read() -> Int:
    var dependency: Int
    var reader: () -> Int = () => dependency
    for ordinal in 1..3:
        reader = () => ordinal
        continue
    return reader()
scene start():
    val observed = length({0: "ready"}) + read()
"#,
    )
    .unwrap();
}

#[test]
fn proven_descending_and_stepped_progressions_keep_normal_exit_facts() {
    for expression in [
        "1..5 step 2",
        "5 downTo 1",
        "5 downTo 1 step 2",
        "(1..5 step 2) step 3",
        "base step 2",
    ] {
        let source = format!(
            "scene start():\n    val base: IntRange = 1..5\n    var result: Int\n    for ordinal in {expression}:\n        result = ordinal\n    val observed = result\n"
        );
        compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn unknown_progressions_and_rhs_replacements_do_not_prove_the_captured_value_nonempty() {
    for expression in [
        "progression",
        "progression step (if true:\n        progression = 1..3\n        if progression !is IntRange:\n            return 0\n        1\n    else:\n        return 0\n    )",
    ] {
        let source = format!(
            "fun run(var progression: IntProgression) -> Int:\n    var result: Int\n    for ordinal in {expression}:\n        result = ordinal\n    return result\nscene start():\n    Unit\n"
        );
        let error = compile(&source)
            .map(|_| ())
            .expect_err("later RHS state cannot describe the already captured progression");
        assert!(error.contains("not definitely initialized"), "{source}\n{error}");
    }
}
