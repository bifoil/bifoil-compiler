//! Traversal proofs distinguish immutable contents from readonly aliases.

use super::compile;

fn expect_initialization_error(source: &str) {
    let error = compile(source).map(|_| ()).expect_err("the traversal may be empty");
    assert!(error.contains("not definitely initialized"), "{source}\n{error}");
}

#[test]
fn nonempty_collection_literals_and_immutable_aliases_initialize_after_traversal() {
    for expression in ["[1, 2]", "{1, 2}", "{1: 2}"] {
        compile(&format!(
            "scene start():\n    val items = {expression}\n    val alias = items\n    var result: Int\n    for item in alias:\n        result = 7\n    val observed = result\n"
        )).unwrap();
    }
}

#[test]
fn fresh_mutable_literals_are_nonempty_at_traversal_start() {
    for expression in ["mutable [1]", "mutable {1}", "mutable {1: 2}"] {
        compile(&format!(
            "scene start():\n    var result: Int\n    for item in {expression}:\n        result = 7\n    val observed = result\n"
        )).unwrap();
    }
}

#[test]
fn nonempty_text_survives_char_widening_and_formatting() {
    for setup in [
        "    val text = \"😀\"\n",
        "    val scalar = 'x'\n    val text: String = scalar\n",
        "    val text = \"prefix ${1}\"\n",
        "    val tail = \"x\"\n    val text = \"\" + tail\n",
    ] {
        compile(&format!(
            "scene start():\n{setup}    var result: Char\n    for character in text:\n        result = character\n    val observed = result\n"
        )).unwrap();
    }
}

#[test]
fn immutable_copy_construction_preserves_nonempty_contents() {
    compile(
        r#"scene start():
    val items: List<Float> = mutable [1, 2]
    val alias: List<Float>? = items
    if alias != null:
        var result: Float
        for item in alias:
            result = item
        val observed = result
"#,
    )
    .unwrap();
}

#[test]
fn immutable_collection_evidence_remains_available_to_escaping_readers() {
    compile(
        r#"fun make() -> () -> Int:
    val items: Any = [1, 2]
    fun read() -> Int:
        if items is List<Int>:
            var result: Int
            for item in items:
                result = item
            return result
        return 0
    return ::read
scene start():
    val reader = make()
    val observed = reader()
"#,
    )
    .unwrap();
}

#[test]
fn empty_literals_and_unknown_collection_inputs_remain_conservative() {
    for (ty, expression) in [
        ("String", "\"\""),
        ("List<Int>", "[]"),
        ("Set<Int>", "{}"),
        ("Map<Int, Int>", "{}"),
    ] {
        expect_initialization_error(&format!(
            "scene start():\n    val items: {ty} = {expression}\n    var result: Int\n    for item in items:\n        result = 7\n    val observed = result\n"
        ));
    }
    expect_initialization_error(
        "fun read(items: List<Int>) -> Int:\n    var result: Int\n    for item in items:\n        result = item\n    return result\nscene start():\n    Unit\n",
    );
}

#[test]
fn readonly_bindings_do_not_freeze_mutable_collection_aliases() {
    for setup in [
        "    val items = mutable [1]\n",
        "    val items: MutableList<Int> = [1]\n",
        "    val original = [1]\n    val items: MutableList<Int> = original\n",
    ] {
        expect_initialization_error(&format!(
            "scene start():\n{setup}    items.clear()\n    var result: Int\n    for item in items:\n        result = item\n    val observed = result\n"
        ));
    }
}

#[test]
fn erased_mutable_collections_and_reused_names_do_not_inherit_nonempty_facts() {
    expect_initialization_error(
        r#"scene start():
    val original = mutable [1]
    val erased: Any = original
    original.clear()
    if erased is MutableList<Int>:
        var result: Int
        for item in erased:
            result = item
        val observed = result
"#,
    );
    expect_initialization_error(
        r#"fun read(input: List<Int>) -> Int:
    if true:
        val items = [1]
        val observed = items
    val items = input
    var result: Int
    for item in items:
        result = item
    return result
scene start():
    Unit
"#,
    );
}
