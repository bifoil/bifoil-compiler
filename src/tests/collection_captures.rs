//! Built-in copies and extraction retain deferred callback requirements.

use super::compile;

#[test]
fn collection_results_and_bound_methods_do_not_erase_deferred_reads() {
    for expression in [
        "callbacks.toList()[0]()",
        "callbacks.toList().toMutableList()[0]()",
        "callbacks.removeAt(0)()",
        "mapping.remove(\"one\")!!()",
        "(callbacks::toList)()[0]()",
    ] {
        let source = format!(
            "scene start():\n    var future: Int\n    val callbacks = mutable [() => future]\n    val mapping = mutable {{\"one\": () => future}}\n    val result = {expression}\n    future = 3\n"
        );
        let error = compile(&source).expect_err("copied or extracted callbacks must retain their reads");
        assert!(error.contains("not initialized"), "{error}\n{source}");
    }
}

#[test]
fn copying_can_precede_initialization_and_container_only_operations_do_not_invoke_callbacks() {
    compile(
        r#"scene start():
    var future: Int
    val callbacks = [() => future]
    val copied = callbacks.toList().toMutableList()
    val extracted = copied.removeAt(0)
    val ignored = mutable [() => future]
    ignored.clear()
    val empty = ignored.isEmpty()
    val saved = callbacks::toList
    future = 3
    val first = extracted()
    val second = saved()[0]()
"#,
    )
    .unwrap();
}
