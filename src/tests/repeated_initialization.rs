//! Mutable writer facts apply independently at each direct source call.

use super::compile;

#[test]
fn repeated_forward_mutable_calls_preserve_read_after_write_facts() {
    compile(
        r#"scene start():
    var result: Int
    val first = initialize(3)
    val observed = result
    val second = initialize(4)
    fun initialize(value: Int) -> String:
        result = value
        return "ready ${result}"
    prose:
        ${first}|${observed}|${second}|${result}
"#,
    )
    .unwrap();
}

#[test]
fn separate_forward_branches_refine_their_own_callable_metadata() {
    compile(
        r#"fun run(flag: Boolean) -> Int:
    var result: Int
    if flag:
        initialize(3)
    else:
        initialize(4)
    fun initialize(value: Int) -> Int:
        result = value
        return result
    return result
scene start():
    val first = run(true)
    val second = run(false)
"#,
    )
    .unwrap();
}

#[test]
fn repeated_branch_results_join_their_mutable_write_facts() {
    compile(
        r#"fun run(flag: Boolean) -> String:
    var result: Int
    val status = if flag:
        initialize(3)
    else:
        initialize(4)
    fun initialize(value: Int) -> String:
        result = value
        return "ready ${result}"
    return "${status}|${result}"
scene start():
    val first = run(true)
    val second = run(false)
"#,
    )
    .unwrap();
}

#[test]
fn forward_write_facts_remain_valid_for_later_calls_after_the_definition() {
    compile(
        r#"scene start():
    var result: Int
    val first = initialize(3)
    fun initialize(value: Int) -> String:
        result = value
        return "ready ${result}"
    val second = initialize(4)
    val observed = result
"#,
    )
    .unwrap();
}

#[test]
fn transferred_arguments_and_nested_calls_use_only_completed_invocations() {
    for source in [
        r#"fun run(flag: Boolean) -> Int:
    var result: Int
    if flag:
        initialize(return -1)
    else:
        initialize(3)
    fun initialize(value: Int) -> Int:
        result = value
        return result
    return result
scene start():
    val first = run(true)
    val second = run(false)
"#,
        r#"scene start():
    var result: Int
    val observed = initialize(initialize(3))
    fun initialize(value: Int) -> Int:
        result = value
        return result
    val actual = result
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn repeated_calls_cannot_claim_partial_writes_or_bypass_early_reads_and_val_rules() {
    for source in [
        r#"scene start():
    var result: Int
    initialize(false)
    initialize(false)
    fun initialize(flag: Boolean):
        if flag:
            result = 3
    val invalid = result
"#,
        r#"scene start():
    var result: Int
    initialize(3)
    initialize(4)
    fun initialize(value: Int):
        result = result + value
"#,
        r#"scene start():
    val result: Int
    initialize(3)
    initialize(4)
    fun initialize(value: Int):
        result = value
"#,
    ] {
        let error = compile(source)
            .map(|_| ())
            .expect_err("each invocation still obeys initialization rules");
        assert!(
            error.contains("initialized") || error.contains("captured val"),
            "{source}\n{error}"
        );
    }
}

#[test]
fn mutable_forward_overloads_select_their_own_write_proofs() {
    for (first, second, definitions) in [
        (
            "initialize(3)",
            "initialize(\"Ada\")",
            "    fun initialize(value: Int) -> Int:\n        result = value\n        return result\n    fun initialize(name: String) -> Int:\n        result = name.length\n        return result\n",
        ),
        (
            "initialize(3)",
            "initialize(3, 4)",
            "    fun initialize(value: Int) -> Int:\n        result = value\n        return result\n    fun initialize(value: Int, increment: Int) -> Int:\n        result = value + increment\n        return result\n",
        ),
        (
            "initialize(3)",
            "initialize(\"Ada\")",
            "    fun initialize(value: Int) -> Int:\n        result = value\n        return result\n    fun <T: Equatable> initialize(value: T) -> Int:\n        if value == value:\n            result = 4\n        else:\n            result = 5\n        return result\n",
        ),
    ] {
        let source = format!(
            "fun run(flag: Boolean) -> Int:\n    var result: Int\n    if flag:\n        {first}\n    else:\n        {second}\n{definitions}    return result\nscene start():\n    val first = run(true)\n    val second = run(false)\n"
        );
        compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}
