//! One captured val initialization may have several exclusive source call sites.

use super::compile;

#[test]
fn exclusive_calls_after_the_definition_initialize_one_captured_val() {
    compile(
        r#"fun run(select: Boolean) -> Int:
    val result: Int
    fun initialize(value: Int):
        result = value
    if select:
        initialize(1)
    else:
        initialize(2)
    return result
scene start():
    val observed = run(true)
"#,
    )
    .unwrap();
}

#[test]
fn exclusive_forward_calls_keep_real_frames_and_result_values() {
    compile(
        r#"fun run(select: Boolean) -> String:
    val result: Int
    val status = if select:
        initialize(1)
    else:
        initialize(2)
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
    return "${status}|${result}"
scene start():
    val observed = run(false)
"#,
    )
    .unwrap();
}

#[test]
fn when_and_choice_branches_may_supply_the_single_initialization() {
    compile(
        r#"suspend fun run(select: Boolean) -> Int:
    val first: Int
    when select:
        case true:
            initializeFirst(1)
        case false:
            initializeFirst(2)
    fun initializeFirst(value: Int):
        first = value
    val second: Int
    fun initializeSecond(value: Int):
        second = value
    choice:
        option "First":
            initializeSecond(3)
        option "Second":
            initializeSecond(4)
    return first + second
scene start():
    val observed = run(true)
"#,
    )
    .unwrap();
}

#[test]
fn a_fresh_iteration_binding_can_choose_one_of_multiple_initializer_sites() {
    compile(
        r#"scene start():
    var total = 0
    for ordinal in 1..2:
        val result: Int
        if ordinal == 1:
            initialize(ordinal)
        else:
            initialize(ordinal + 1)
        fun initialize(value: Int):
            result = value
        total += result
"#,
    )
    .unwrap();
}

#[test]
fn sequential_and_possibly_repeated_readonly_initializations_still_fail() {
    for body in [
        "    initialize(1)\n    initialize(2)\n",
        "    if select:\n        initialize(1)\n    initialize(2)\n",
        "    for ordinal in 1..2:\n        initialize(ordinal)\n",
    ] {
        let source = format!(
            "fun run(select: Boolean):\n    val result: Int\n    fun initialize(value: Int):\n        result = value\n{body}scene start():\n    Unit\n"
        );
        let error = compile(&source).map(|_| ()).expect_err("one path can initialize twice");
        assert!(
            error.contains("captured val") || error.contains("could be initialized more than once"),
            "{source}\n{error}"
        );
    }
}

#[test]
fn a_missing_branch_write_or_a_published_writer_still_fails() {
    for source in [
        r#"fun run(select: Boolean) -> Int:
    val result: Int
    fun initialize(value: Int):
        result = value
    if select:
        initialize(1)
    return result
scene start():
    Unit
"#,
        r#"scene start():
    val result: Int
    fun initialize(value: Int):
        result = value
    val writer = ::initialize
    if true:
        initialize(1)
    else:
        initialize(2)
"#,
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
}

#[test]
fn readonly_type_and_arity_overloads_keep_separate_write_proofs() {
    compile(
        r#"fun typed(select: Boolean) -> String:
    val result: String
    if select:
        initialize(1)
    else:
        initialize("two")
    fun initialize(value: Int):
        result = "${value}"
    fun initialize(value: String):
        result = value
    return result
fun arity(select: Boolean) -> Int:
    val result: Int
    if select:
        initialize(1)
    else:
        initialize(1, 2)
    fun initialize(value: Int):
        result = value
    fun initialize(first: Int, second: Int):
        result = first + second
    return result
scene start():
    val first = typed(true)
    val second = arity(false)
"#,
    )
    .unwrap();
}

#[test]
fn readonly_generic_overloads_follow_the_selected_declaration() {
    compile(
        r#"fun run(select: Boolean) -> Int:
    val result: Int
    if select:
        initialize(1)
    else:
        initialize<String>(2, "extra")
    fun initialize(value: Int):
        result = value
    fun <T> initialize(value: Int, extra: T):
        result = value
    return result
scene start():
    val observed = run(false)
"#,
    )
    .unwrap();
}

#[test]
fn different_overloads_cannot_initialize_the_same_val_twice() {
    let error = compile(
        r#"scene start():
    val result: String
    fun initialize(value: Int):
        result = "${value}"
    fun initialize(value: String):
        result = value
    initialize(1)
    initialize("second")
"#,
    )
    .map(|_| ())
    .expect_err("overloads still share the captured target");
    assert!(error.contains("captured val"), "{error}");
}
