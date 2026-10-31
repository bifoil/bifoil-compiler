//! Captured-cell stores publish callee locals even before a reader is exported.

use super::compile;

const WRITER: &str = r#"shared var reader: () -> Int = () => 0
shared var writer: () -> Unit = () => Unit
fun install():
    var current: () -> Int = () => 1
    fun observe() -> Int = current()
    fun replace():
        var future: Int
        current = () => future
        val value = reader()
        future = 9
    reader = ::observe
    writer = ::replace
scene start():
    install()
    writer()
"#;

#[test]
fn a_writer_declared_before_publication_cannot_expose_its_uninitialized_local() {
    let error = compile(WRITER).unwrap_err();
    assert!(
        error.contains("captured binding 'future' is not initialized"),
        "{error}"
    );
    compile(&WRITER.replace(
        "current = () => future\n        val value = reader()\n        future = 9",
        "future = 9\n        current = () => future\n        val value = reader()",
    ))
    .unwrap();
}

#[test]
fn same_frame_deferred_values_and_write_only_captures_remain_available() {
    compile(
        r#"scene start():
    var saved: () -> Int
    if true:
        var local: Int
        saved = () => local
        local = 9
    val value = saved()
    var uninitialized: Int
    fun writer():
        uninitialized = 7
    writer()
    val initialized = uninitialized
"#,
    )
    .unwrap();
}
