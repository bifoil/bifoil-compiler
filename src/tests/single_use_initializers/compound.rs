//! Compound sites keep the original read, result checks, and one-use write proof.

use super::compile;

#[test]
fn compound_calls_preserve_the_original_read_value() {
    compile(
        r#"scene start():
    val result: Int
    var status = "pending"
    fun initialize() -> String:
        status = "changed"
        result = 3
        return "ready"
    status += (initialize())
    prose:
        ${result}|${status}
"#,
    )
    .unwrap();
}

#[test]
fn compound_fairmath_calls_keep_guarded_read_types() {
    for source in [
        r#"suspend fun run() -> String:
    var score: Int? = 50
    if score != null:
        val result: Int
        suspend fun initialize() -> Int:
            score = null
            choice:
                option "Cleared ${score == null}":
                    Unit
            result = 7
            return 20
        score %+= initialize()
        return "${result}|${score == 60}"
    else:
        return "unexpected"
"#,
        r#"data class Box:
    var score: Int? = 50
suspend fun run() -> String:
    val box = Box()
    if box.score != null:
        val result: Int
        suspend fun initialize() -> Int:
            box.score = null
            choice:
                option "Cleared ${box.score == null}":
                    Unit
            result = 7
            return 20
        box.score %+= initialize()
        return "${result}|${box.score == 60}"
    else:
        return "unexpected"
"#,
        r#"suspend fun run() -> String:
    val items: MutableList<Int?> = mutable [50]
    if items[0] != null:
        val result: Int
        suspend fun initialize() -> Int:
            items[0] = null
            choice:
                option "Cleared ${items[0] == null}":
                    Unit
            result = 7
            return 20
        items[0] %+= initialize()
        return "${result}|${items[0] == 60}"
    else:
        return "unexpected"
"#,
        r#"suspend fun run() -> String:
    val items: MutableMap<String, Int?> = mutable {"k": 50}
    if items["k"] != null:
        val result: Int
        suspend fun initialize() -> Int:
            items["k"] = null
            choice:
                option "Cleared":
                    Unit
            result = 7
            return 20
        items["k"] %+= initialize()
        return "${result}|${items["k"] == 60}"
    else:
        return "unexpected"
"#,
    ] {
        compile(source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn compound_sites_keep_readiness_and_single_invocation_checks() {
    for (source, diagnostic) in [
        (
            r#"scene start():
    val result: Int
    fun initialize() -> Int:
        result = 3
        return 1
    result += initialize()
"#,
            "compound update reads uninitialized",
        ),
        (
            r#"scene start():
    val result: Int
    var status = "pending"
    fun initialize() -> String:
        result = 3
        return "ready"
    status += initialize()
    status += initialize()
"#,
            "captured val",
        ),
        (
            r#"scene start():
    val result: Int
    val items = mutable {"k": "pending"}
    fun initialize(value: Int) -> String:
        result = value
        return "k"
    items[initialize(1)] += initialize(2)
"#,
            "captured val",
        ),
    ] {
        let error = compile(source).expect_err("compound syntax cannot bypass reads, references, or repeat checks");
        assert!(error.contains(diagnostic), "{source}\n{error}");
    }
}

#[test]
fn compound_call_results_still_fit_the_target_type() {
    let error = compile(
        r#"scene start():
    val result: Int
    var count = 0
    fun initialize() -> String:
        result = 3
        return "ready"
    count += initialize()
"#,
    )
    .expect_err("String concatenation does not assign its result to an Int target");
    assert!(error.contains("expected Int, found String"), "{error}");
}
