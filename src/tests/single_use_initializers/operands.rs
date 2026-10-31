//! Expression operands retain ordinary ordering, flow joins, and real call frames.

use super::compile;

#[test]
fn arithmetic_collection_and_argument_calls_initialize_the_capture() {
    for expression in [
        "initialize() + result",
        "consume(initialize(), result)",
        "[initialize(), result][0]",
        "{initialize(): result}[3]!!",
        "\"value ${initialize()}|${result}\"",
        "initialize().length",
    ] {
        let string_result = expression.ends_with(".length");
        let result_type = if string_result { "String" } else { "Int" };
        let returned = if string_result { "\"ready\"" } else { "3" };
        let source = format!(
            r#"fun consume(first: Int, second: Int) -> Int = first + second
scene start():
    val result: Int
    fun initialize() -> {result_type}:
        result = 3
        return {returned}
    val answer = {expression}
    prose:
        ${{result}}
"#
        );
        compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn nested_unit_calls_keep_the_real_immutable_write() {
    compile(
        r#"fun consume(value: Unit):
    Unit
scene start():
    val result: Int
    fun initialize():
        result = 3
    consume(initialize())
    prose:
        ${result}
"#,
    )
    .unwrap();
}

#[test]
fn indexed_target_calls_execute_before_the_rhs() {
    compile(
        r#"scene start():
    val result: Int
    val items = mutable [0]
    fun initialize() -> Int:
        result = 3
        return 0
    items[initialize()] = result
    prose:
        ${items[0]}|${result}
"#,
    )
    .unwrap();
}

#[test]
fn optional_calls_do_not_make_skipped_writes_definite() {
    for expression in [
        "false && initialize()",
        "true || initialize()",
        "present ?: initialize()",
    ] {
        let boolean = expression.contains("&&") || expression.contains("||");
        let result_type = if boolean { "Boolean" } else { "Int" };
        let returned = if boolean { "true" } else { "3" };
        let source = format!(
            r#"scene start():
    val result: Int
    val present: Int? = 2
    fun initialize() -> {result_type}:
        result = 3
        return {returned}
    val answer = {expression}
    prose:
        ${{result}}
"#
        );
        let error = compile(&source).expect_err("a skipped call cannot initialize its capture");
        assert!(error.contains("not definitely initialized"), "{source}\n{error}");
    }
}

#[test]
fn sibling_operands_keep_readiness_order_and_type_checks() {
    for (expression, diagnostic) in [
        ("result + initialize()", "not definitely initialized"),
        ("consume(result, initialize())", "not definitely initialized"),
        ("initialize() + initialize()", "captured val"),
        ("initialize() + true", "operator"),
    ] {
        let source = format!(
            r#"fun consume(first: Int, second: Int) -> Int = first + second
scene start():
    val result: Int
    fun initialize() -> Int:
        result = 3
        return 3
    val answer = {expression}
"#
        );
        let error = compile(&source).expect_err("operand syntax cannot bypass readiness or types");
        assert!(error.contains(diagnostic), "{source}\n{error}");
    }
}

#[test]
fn deferred_or_repeated_uses_remain_outside_the_expression_proof() {
    for use_site in [
        "val saved = () => initialize()\n    val answer = initialize() + 1",
        "val saved = ::initialize\n    val answer = initialize() + 1",
        "while true:\n        val answer = initialize() + 1\n        continue",
        "val answer = if true:\n        while true:\n            initialize()\n            continue\n        1\n    else:\n        0",
    ] {
        let source = format!(
            r#"scene start():
    val result: Int
    fun initialize() -> Int:
        result = 3
        return 3
    {use_site}
"#
        );
        let error = compile(&source).expect_err("deferred bodies and loop backedges do not prove one invocation");
        assert!(
            error.contains("captured val") || error.contains("could be initialized more than once"),
            "{source}\n{error}"
        );
    }
}

#[test]
fn safe_access_arguments_and_indexes_keep_skipped_paths_uninitialized() {
    for (parameters, expression) in [
        ("callback: ((Int) -> Unit)?", "callback?.(initialize())"),
        ("items: List<Int>?", "items?[initialize()]"),
        ("receiver: Receiver?", "receiver?.accept(initialize())"),
    ] {
        let source = format!(
            r#"data class Receiver:
    fun accept(value: Int):
        return
fun invalid({parameters}) -> Int:
    val result: Int
    fun initialize() -> Int:
        result = 3
        return 0
    val ignored = {expression}
    return result
"#
        );
        let error = compile(&source).expect_err("null receivers may skip their arguments or index");
        assert!(error.contains("not definitely initialized"), "{source}\n{error}");
    }
}
