//! Mutable captured cells retain publication obligations through flow joins.

use super::compile;

const PRELUDE: &str = r#"shared var callback: () -> Int = () => 0
fun invoke() -> Int = callback()
fun publish(action: () -> Int):
    callback = action
scene start():
    var future: Int
    var target: () -> Int = () => 7
    var other: () -> Int = () => 9
    fun through() -> Int = target()
"#;

#[test]
fn escaped_cells_reject_new_uninitialized_reads_at_subsequent_writes() {
    for statements in [
        "    callback = ::through\n    target = () => future\n",
        "    callback = ::through\n    target = () => other()\n    other = () => future\n",
        "    publish(::through)\n    target = () => future\n",
        "    val stored = mutable [() => 0]\n    stored[0] = ::through\n    target = () => future\n",
        "    if callback() == 0:\n        callback = ::through\n    target = () => future\n",
        "    target = if true:\n        callback = ::through\n        () => future\n    else:\n        () => 0\n",
    ] {
        let source = format!("{PRELUDE}{statements}    val result = invoke()\n    future = 3\n");
        let error = compile(&source).expect_err("opaque aliases may invoke the new callback immediately");
        assert!(error.contains("not initialized"), "{error}\n{source}");
    }
}

#[test]
fn escaped_cells_accept_initialized_retargets_and_shared_state_updates() {
    compile(&format!(
        "{PRELUDE}    callback = ::through\n    target = () => other()\n    future = 3\n    other = () => future\n    val result = invoke()\n    future = 9\n    val changed = invoke()\n"
    ))
    .unwrap();
}

#[test]
fn publication_facts_follow_reachable_branches_without_marking_ordinary_local_calls() {
    compile(
        r#"shared var callback: () -> Int = () => 0
fun run(flag: Boolean):
    var future: Int
    var target: () -> Int = () => 7
    fun through() -> Int = target()
    val previous = through()
    if flag:
        callback = ::through
    else:
        target = () => future
        future = 3
        val result = through()
scene start():
    var future: Int
    var target: () -> Int = () => 7
    fun through() -> Int = target()
    val previous = through()
    if false:
        callback = ::through
    target = () => future
    future = 3
    val result = through()
    run(true)
    run(false)
"#,
    )
    .unwrap();
}
