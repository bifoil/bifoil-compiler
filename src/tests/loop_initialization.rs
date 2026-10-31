//! Loop execution repeats mutable writers without duplicating protected val writes.

use super::compile;

#[test]
fn forward_writers_initialize_each_entered_for_body() {
    compile(
        r#"fun run() -> List<Int>:
    var result: Int
    val observed: MutableList<Int> = []
    for ordinal in 1..2:
        initialize(ordinal)
        observed.add(result)
    fun initialize(amount: Int):
        result = amount
    return observed
scene start():
    val observed = run()
"#,
    )
    .unwrap();
}

#[test]
fn forward_condition_writers_initialize_even_when_while_exits_immediately() {
    compile(
        r#"scene start():
    var result: Int
    while initialize():
        break
    fun initialize() -> Boolean:
        result = 3
        return false
    val observed = result
"#,
    )
    .unwrap();
}

#[test]
fn posttest_writer_arguments_retain_their_body_scope_without_capturing_it() {
    compile(
        r#"scene start():
    var result: Int
    do:
        val amount = 3
    while initialize(amount)
    fun initialize(value: Int) -> Boolean:
        result = value
        return false
    val observed = result
"#,
    )
    .unwrap();
}

#[test]
fn a_guaranteed_range_and_break_keep_the_completed_write_at_the_exit() {
    compile(
        r#"scene start():
    var result: Int
    for ordinal in 1..3:
        initialize(ordinal)
        break
    fun initialize(amount: Int):
        result = amount
    val observed = result
"#,
    )
    .unwrap();
}

#[test]
fn while_bodies_and_nested_labeled_jumps_keep_their_completed_writes() {
    for source in [
        r#"scene start():
    var result: Int
    var remaining = 2
    while remaining > 0:
        initialize(remaining)
        val observed = result
        remaining -= 1
    fun initialize(amount: Int) -> Int:
        result = amount
        return result
"#,
        r#"scene start():
    var result: Int
    outer@ for ordinal in 1..2:
        for other in 1..2:
            initialize(ordinal)
            val observed = result
            continue@outer
    fun initialize(amount: Int) -> Int:
        result = amount
        return result
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn repeated_regions_reject_outer_val_writes_but_allow_fresh_iteration_vals() {
    let error = compile(
        r#"scene start():
    val result: Int
    for ordinal in 1..2:
        initialize(ordinal)
    fun initialize(amount: Int):
        result = amount
"#,
    )
    .map(|_| ())
    .expect_err("the same outer val cannot be written on each traversal");
    assert!(error.contains("could be initialized more than once"), "{error}");
    compile(
        r#"scene start():
    for ordinal in 1..2:
        val result: Int
        initialize()
        fun initialize():
            result = ordinal
        val observed = result
"#,
    )
    .unwrap();
}

#[test]
fn skipped_loops_and_short_circuit_conditions_do_not_create_write_facts() {
    for loop_source in [
        "    val empty: List<Int> = []\n    for ordinal in empty:\n        initialize()\n",
        "    while false:\n        initialize()\n",
        "    while false && initialize():\n        Unit\n",
    ] {
        let source = format!(
            "scene start():\n    var result: Int\n{loop_source}    fun initialize() -> Boolean:\n        result = 3\n        return true\n    val invalid = result\n"
        );
        let error = compile(&source)
            .map(|_| ())
            .expect_err("a skipped loop or call cannot initialize its target");
        assert!(error.contains("not definitely initialized"), "{source}\n{error}");
    }
}

#[test]
fn loop_body_writers_keep_initial_reads_checked_on_the_first_iteration() {
    for initialized in [false, true] {
        let setup = if initialized { "    result = 0\n" } else { "" };
        let source = format!(
            "scene start():\n    var result: Int\n{setup}    for ordinal in 1..2:\n        initialize(ordinal)\n        val observed = result\n    fun initialize(amount: Int) -> Int:\n        result = result + amount\n        return result\n"
        );
        if initialized {
            compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
        } else {
            let error = compile(&source)
                .map(|_| ())
                .expect_err("later iterations do not initialize the first read");
            assert!(error.contains("initialized"), "{source}\n{error}");
        }
    }
}

#[test]
fn loop_argument_bindings_do_not_enter_the_writer_definition_scope() {
    let error = compile(
        r#"scene start():
    var result: Int
    for ordinal in 1..2:
        initialize()
    val ordinal = 7
    fun initialize():
        result = ordinal
"#,
    )
    .map(|_| ())
    .expect_err("the loop ordinal cannot supply the later captured root binding");
    assert!(error.contains("unknown name 'ordinal'"), "{error}");
}
