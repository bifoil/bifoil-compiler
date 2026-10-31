//! Completed helpers may read a captured var after a dominating ready store.

use super::compile;

#[test]
fn helpers_write_before_reading_through_branches_loops_and_early_returns() {
    for body in [
        "value = 7\n        return value",
        "if flag:\n            value = 7\n        else:\n            value = 8\n        return value",
        "do:\n            value = 7\n        while value < 0\n        return value",
        "if flag:\n            value = 7\n            return value\n        value = 8\n        return value",
    ] {
        compile(&format!(
            "scene start():\n    var value: Int\n    fun initialize(flag: Boolean) -> Int:\n        {body}\n    val result = initialize(true)\n    val copied = value\n"
        )).unwrap();
    }
}

#[test]
fn callback_and_collection_overwrites_do_not_read_the_previous_payload() {
    compile(
        r#"scene start():
    var future: Int
    var callback: () -> Int = () => future
    var items: List<Int>
    fun initialize() -> Int:
        callback = () => 7
        items = [3, 4]
        return callback() + items[0]
    val result = initialize()
    val size = items.size
"#,
    )
    .unwrap();
}

#[test]
fn initializing_function_values_can_outlive_their_uninitialized_cell() {
    compile(
        r#"fun create() -> () -> Int:
    var value: Int
    fun initialize() -> Int:
        value = 7
        return value
    return ::initialize
scene start():
    val callback = create()
    val result = callback()
"#,
    )
    .unwrap();
}

#[test]
fn read_before_write_paths_and_incomplete_writes_keep_entry_requirements() {
    for body in [
        "val previous = value\n        value = 7\n        return value",
        "while value < 0:\n            value = 7\n        value = 8\n        return value",
        "if flag:\n            value = 7\n        return value",
        "if flag:\n            return 0\n        value = 7\n        return value",
    ] {
        let error = compile(&format!(
            "scene start():\n    var value: Int\n    fun initialize(flag: Boolean) -> Int:\n        {body}\n    val result = initialize(true)\n"
        )).unwrap_err();
        assert!(error.contains("captured binding 'value' is not initialized"), "{error}");
    }
}

#[test]
fn calls_and_early_publication_cannot_hide_reads_before_the_write() {
    for prefix in ["val ignored = reader()", "saved = () => value"] {
        let error = compile(&format!(
            r#"shared var saved: () -> Int = () => 0
scene start():
    var value: Int
    fun reader() -> Int = value
    fun initialize() -> Int:
        {prefix}
        value = 7
        return value
    val result = initialize()
"#
        ))
        .unwrap_err();
        assert!(error.contains("value") && error.contains("initialized"), "{error}");
    }
}

#[test]
fn defaults_and_earlier_snapshots_keep_their_own_requirements() {
    let default = compile(
        r#"scene start():
    var value: Int
    fun initialize(previous: Int = value) -> Int:
        value = 7
        return value
    val result = initialize()
"#,
    )
    .unwrap_err();
    assert!(
        default.contains("captured binding 'value' is not initialized"),
        "{default}"
    );
    let snapshot = compile(
        r#"scene start():
    var future: Int
    var callback: () -> Int = () => future
    val previous = callback
    fun initialize() -> Int:
        callback = () => 7
        return callback()
    val result = initialize()
    val old = previous()
"#,
    )
    .unwrap_err();
    assert!(
        snapshot.contains("future") && snapshot.contains("initialized"),
        "{snapshot}"
    );
}
