//! A compact Unit result must preserve surrounding evaluation and flow joins.

use super::compile;

#[test]
fn pure_forward_initializers_supply_unit_in_value_contexts() {
    for use_site in [
        "    val done = initialize()\n",
        "    val done = accept(1, (initialize)(), result)\n",
        "    val done = [Unit, initialize(), Unit]\n",
        "    val done = if initialize() == Unit:\n        7\n    else:\n        8\n",
        "    val slots: MutableMap<Int, Unit> = mutable {0: Unit}\n    slots[0] = initialize()\n",
        "    prose:\n        ${accept(1, initialize(), result)}\n",
    ] {
        let source = format!(
            "fun accept(before: Int, token: Unit, after: Int) -> Int = before + after\nscene start():\n    val result: Int\n{use_site}    fun initialize():\n        result = 3\n    val observed = result\n"
        );
        compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn optional_unit_calls_do_not_initialize_the_skipped_path() {
    for use_site in [
        "    val done = false && (initialize() == Unit)\n",
        "    val callback: ((Unit) -> Int)? = null\n    val done = callback?.(initialize())\n",
    ] {
        let source = format!(
            "scene start():\n    val result: Int\n{use_site}    val observed = result\n    fun initialize():\n        result = 3\n"
        );
        let error = compile(&source).expect_err("a skipped expression leaves its cell uninitialized");
        assert!(error.contains("initialized"), "{source}\n{error}");
    }
}

#[test]
fn consumed_unit_calls_keep_earlier_operand_reads_and_second_writes_checked() {
    for (use_site, tail) in [
        ("    val done = accept(result, initialize(), 2)\n", ""),
        ("    val done = initialize()\n", "    result = 4\n"),
        ("    val done = initialize()\n", "    val escaped = ::initialize\n"),
        ("    val done = initialize()\n", "    val again = initialize()\n"),
    ] {
        let source = format!(
            "fun accept(before: Int, token: Unit, after: Int) -> Int = before + after\nscene start():\n    val result: Int\n{use_site}    fun initialize():\n        result = 3\n{tail}"
        );
        let error = compile(&source).expect_err("Unit conversion cannot bypass immutable flow checks");
        assert!(
            error.contains("initialized") || error.contains("captured val"),
            "{source}\n{error}"
        );
    }
}

#[test]
fn compact_value_lowering_preserves_return_and_definition_type_checks() {
    compile(
        r#"fun run():
    val result: Int
    return initialize()
    fun initialize():
        result = 3
scene start():
    run()
"#,
    )
    .unwrap();
    let error = compile(
        r#"scene start():
    val result: Int
    val done = initialize()
    fun initialize() -> Int:
        result = 3
"#,
    )
    .expect_err("an invalid declared return type is still checked");
    assert!(error.contains("must return"), "{error}");
}
