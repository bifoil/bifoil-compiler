//! Publishing a callback checks its current reads at the global alias boundary.

use super::compile;

#[test]
fn global_callback_publication_rejects_current_uninitialized_reads() {
    for source in [
        r#"shared var callback: () -> Int = () => 0
fun invoke() -> Int = callback()
scene start():
    var future: Int
    callback = () => future
    val result = invoke()
    future = 3
"#,
        r#"shared var callbacks: List<() -> Int> = [() => 0]
fun invoke() -> Int = callbacks[0]()
scene start():
    var future: Int
    callbacks = [() => future]
    val result = invoke()
    future = 3
"#,
    ] {
        let error = compile(source).expect_err("global consumers cannot trust a callback's uninitialized capture");
        assert!(error.contains("not initialized"), "{error}\n{source}");
    }
}

#[test]
fn local_deferred_captures_and_provably_unused_defaults_remain_valid() {
    compile(
        r#"shared var callback: () -> Int = () => 0
fun invoke() -> Int = callback()
scene start():
    var future: Int
    val local = () => future
    future = 3
    callback = local
    val first = invoke()
"#,
    )
    .unwrap();
    compile(
        r#"shared var callback: () -> Int = () => 0
fun invoke() -> Int = callback()
scene start():
    var unused: Int
    fun read(value: Int = unused) -> Int = value
    fun through() -> Int = read(7)
    callback = ::through
    choice:
        option "Resume skipped default":
            Unit
    val first = invoke()
"#,
    )
    .unwrap();
}
