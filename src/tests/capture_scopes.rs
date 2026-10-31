//! Capture checks use declaration-site bindings, including before forward calls.

use super::compile;

fn future_read(call: &str) -> String {
    format!(
        "scene start():\n    if true:\n        val x = 1\n        {call}\n    val x = 2\n    fun read() -> Int = x\n"
    )
}

#[test]
fn an_inner_binding_cannot_prove_a_later_capture_initialized() {
    for call in [
        "val result = read()",
        "val callback: () -> Int = ::read\n        val result = callback()",
        "val callbacks = [::read]\n        val result = callbacks[0]()",
    ] {
        let error = compile(&future_read(call)).unwrap_err();
        assert!(
            error.contains("cannot prove captured binding 'x' initialized"),
            "{error}"
        );
    }
}

#[test]
fn a_present_declaration_site_binding_remains_available_to_forward_calls() {
    compile(
        r#"scene start():
    val x = 2
    if true:
        val other = 1
        val callback: () -> Int = ::read
        val result = callback() + read()
    fun read() -> Int = x
"#,
    )
    .unwrap();
}

#[test]
fn omitted_defaults_keep_their_declaration_environment() {
    let source = r#"scene start():
    if true:
        val x = 1
        val result = read()
    val x = 2
    fun read(value: Int = x) -> Int = value
"#;
    assert!(
        compile(source)
            .unwrap_err()
            .contains("cannot prove captured binding 'x' initialized")
    );
    compile(&source.replace("val result = read()", "val result = read(7)")).unwrap();
}

#[test]
fn overload_selection_keeps_only_the_selected_declaration_scope() {
    let source = r#"scene start():
    if true:
        val x = 1
        val result = read(7)
    val x = 2
    fun read(value: Int = x) -> Int = value
    fun read(value: Float) -> Int = x
"#;
    compile(source).unwrap();
    let error = compile(&source.replace("val result = read(7)", "val result = read(7.0)")).unwrap_err();
    assert!(
        error.contains("cannot prove captured binding 'x' initialized"),
        "{error}"
    );
}
