//! Branches, choices, loops, and suspension preserve mandatory-write proofs.

use super::compile;

#[test]
fn structured_helpers_join_all_normal_return_writes() {
    for source in [
        r#"scene start():
    val x: Int
    val y: Int
    fun initialize(flag: Boolean, value: Int):
        if flag:
            x = value
        else:
            x = value + 1
        y = x + 1
    initialize(true, 3)
    prose:
        ${x}|${y}
"#,
        r#"scene start():
    val x: Int
    fun initialize():
        if true:
            x = 3
    initialize()
    prose:
        ${x}
"#,
        r#"scene start():
    val x: Int
    fun initialize():
        while true:
            x = 3
            break
    initialize()
    prose:
        ${x}
"#,
        r#"scene start():
    val x: Int
    fun initialize():
        do:
            x = 3
            break
        while false
    initialize()
    prose:
        ${x}
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn structured_helpers_keep_partial_writes_and_short_circuit_entries_conservative() {
    for condition in ["flag", "flag && true"] {
        let source = format!(
            "scene start():\n    val x: Int\n    fun initialize(flag: Boolean):\n        if {condition}:\n            x = 3\n    initialize(true)\n"
        );
        let error = compile(&source).expect_err("not every normal return writes the val");
        assert!(error.contains("cannot prove every captured val"), "{error}");
    }
    let error = compile(
        r#"scene start():
    val x: Int
    fun initialize(flag: Boolean):
        if flag || false:
            Unit
        else:
            x = 3
    initialize(true)
"#,
    )
    .expect_err("a jump entering the branch can bypass its preceding false constant");
    assert!(error.contains("cannot prove every captured val"), "{error}");
    let error = compile(
        r#"scene start():
    val x: Int
    fun initialize():
        while true:
            x = 3
    initialize()
"#,
    )
    .expect_err("a single helper call cannot write an immutable cell repeatedly");
    assert!(error.contains("initialized more than once"), "{error}");
}

#[test]
fn suspending_helpers_can_initialize_in_stages() {
    compile(
        r#"scene start():
    val x: Int
    val y: Int
    suspend fun initialize(value: Int):
        x = value
        choice:
            option "Finish ${x}":
                Unit
        y = x + 1
    initialize(6)
    prose:
        ${x}|${y}
"#,
    )
    .expect("normal completion proves both captured writes across suspension");
}

#[test]
fn choice_write_proofs_keep_each_preparation_separate() {
    for source in [
        r#"suspend fun run() -> Int:
    val result: Int
    suspend fun initialize():
        choice:
            option "Before":
                Unit
        result = 3
        choice:
            option "After":
                Unit
    initialize()
    return result
"#,
        r#"suspend fun run(flag: Boolean) -> Int:
    val result: Int
    suspend fun initialize():
        choice:
            option "Outer":
                choice:
                    option "Inner" if flag:
                        result = 3
                    else:
                        result = 4
        choice:
            option "After ${result}":
                Unit
    initialize()
    return result
"#,
        r#"suspend fun <T> keep(input: T) -> T:
    val result: T
    suspend fun <U: T> initialize(value: U):
        choice:
            option "Before":
                Unit
        result = value
        choice:
            option "After":
                Unit
    initialize<T>(input)
    return result
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn separate_choice_proofs_still_require_every_normal_return() {
    for nested in [false, true] {
        let branches = if nested {
            "            option \"Outer\":\n                choice:\n                    option \"Write\":\n                        result = 3\n                    option \"Skip\":\n                        Unit"
        } else {
            "            option \"Write\":\n                result = 3\n            option \"Skip\":\n                Unit"
        };
        let source = format!(
            "suspend fun run() -> Int:\n    val result: Int\n    suspend fun initialize():\n        choice:\n{branches}\n        choice:\n            option \"After\":\n                Unit\n    initialize()\n    return result\n"
        );
        let error = compile(&source).expect_err("a later choice cannot hide an earlier skipped write");
        assert!(error.contains("cannot prove every captured val"), "{source}\n{error}");
    }
}

#[test]
fn choice_waits_do_not_hide_real_entry_reads() {
    for expression in ["result", "read()"] {
        let source = format!(
            "suspend fun run() -> Int:\n    val result: Int\n    fun read() -> Int = result\n    suspend fun initialize():\n        choice:\n            option \"Before ${{{expression}}}\":\n                Unit\n        result = 3\n        choice:\n            option \"After\":\n                Unit\n    initialize()\n    return result\n"
        );
        let error = compile(&source).expect_err("preparation reads and calls still require the old value");
        assert!(error.contains("not initialized"), "{source}\n{error}");
    }
}
