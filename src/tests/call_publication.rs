//! Invoked helpers retain publication obligations on mutable captured cells.

use super::compile;

fn publication(call: &str, later: &str) -> String {
    format!(
        "shared var exposed: () -> Int = () => 0\nscene start():\n    var current: () -> Int = () => 1\n    fun publish():\n        exposed = () => current()\n    {call}\n    var future: Int\n    {later}\n    prose:\n        ${{exposed()}}\n"
    )
}

#[test]
fn called_publishers_keep_later_callback_assignments_checked() {
    for call in ["publish()", "val action = ::publish\n    action()"] {
        let error = compile(&publication(call, "current = () => future")).unwrap_err();
        assert!(
            error.contains("captured binding 'future' is not initialized"),
            "{error}"
        );
        compile(&publication(call, "future = 9\n    current = () => future")).unwrap();
    }
}

#[test]
fn publication_checks_apply_at_calls_instead_of_declarations() {
    compile(&publication(
        "Unit",
        "current = () => future\n    future = 9\n    publish()",
    ))
    .unwrap();
    compile(&publication(
        "if false:\n        publish()",
        "current = () => future\n    future = 9\n    publish()",
    ))
    .unwrap();
}

#[test]
fn returning_a_reader_retains_the_same_mutable_cell_obligation() {
    let source = r#"shared var exposed: () -> Int = () => 0
scene start():
    var current: () -> Int = () => 1
    fun reader() -> () -> Int = () => current()
    exposed = reader()
    var future: Int
    current = () => future
    prose:
        ${exposed()}
"#;
    let error = compile(source).unwrap_err();
    assert!(
        error.contains("captured binding 'future' is not initialized"),
        "{error}"
    );
    compile(&source.replace("current = () => future", "future = 9\n    current = () => future")).unwrap();
}

#[test]
fn transitive_publishers_preserve_defaults_and_overload_profiles() {
    let common = r#"shared var exposed: () -> Int = () => 0
scene start():
    var current: () -> Int = () => 1
    fun install() -> Int:
        exposed = () => current()
        return 0
    fun withDefault(value: Int = install()):
        Unit
    fun choose(value: Int):
        Unit
    fun choose(value: String):
        install()
    fun forward():
        wrapper()
    fun wrapper():
        install()
"#;
    for call in ["forward()", "withDefault()", "choose(\"publish\")"] {
        let error = compile(&format!(
            "{common}    {call}\n    var future: Int\n    current = () => future\n    future = 9\n"
        ))
        .unwrap_err();
        assert!(
            error.contains("captured binding 'future' is not initialized"),
            "{error}"
        );
    }
    for call in ["withDefault(0)", "choose(0)"] {
        compile(&format!(
            "{common}    {call}\n    var future: Int\n    current = () => future\n    future = 9\n    install()\n"
        ))
        .unwrap();
    }
}

#[test]
fn read_only_wrapper_graphs_do_not_create_publication_obligations() {
    let mut source = String::from("scene start():\n");
    for index in 0..500 {
        source.push_str(&format!("    fun call{index}() -> Int = call{}()\n", index + 1));
    }
    source.push_str("    fun call500() -> Int = 7\n");
    for index in 0..500 {
        source.push_str(&format!("    val result{index} = call{index}()\n"));
    }
    compile(&source).unwrap();
}
