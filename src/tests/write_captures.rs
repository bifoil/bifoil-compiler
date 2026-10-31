//! Plain captured writes need an existing cell, not its previous value.

use super::compile;

#[test]
fn write_only_captures_allow_uninitialized_vars_and_skipped_default_profiles() {
    compile(
        r#"fun run(action: () -> Unit):
    action()
scene start():
    var value: Int
    var score: Int
    fun bump():
        score += 1
    score = 3
    bump()
    fun initialize():
        value = 1
    fun defaults(done: Unit = initialize()) -> Unit = Unit
    initialize()
    defaults()
    run(() =>:
        value = 3
    )
    value = 2
    prose:
        ${value}
"#,
    )
    .unwrap();
}

#[test]
fn escaping_writers_keep_uninitialized_cells_without_exposing_their_old_contents() {
    compile(
        r#"shared var writer: () -> Unit = () => Unit
fun makeWriter() -> suspend (Int) -> Unit:
    var hidden: Int
    return suspend (value: Int) =>:
        choice:
            option "Write ${value}":
                Unit
        hidden = value
scene start():
    var future: Int
    var target: () -> Int
    writer = () =>:
        target = () => 7
    target = () => future
    future = 3
    val first = target()
    writer()
    val second = target()
    val escaped = makeWriter()
    escaped(3)
    val local: () -> Unit = if true:
        var hidden: Int
        fun store():
            hidden = 1
        ::store
    else:
        () => Unit
    local()
"#,
    )
    .unwrap();
}

#[test]
fn read_requirements_are_not_hidden_by_write_only_capture_visits() {
    for body in [
        "    fun update():\n        value += 1\n    update()\n",
        "    fun update():\n        value = value + 1\n    update()\n",
        "    fun read() -> Int = value\n    fun write():\n        value = 1\n    fun both():\n        val previous = read()\n        write()\n    both()\n",
    ] {
        let source = format!("scene start():\n    var value: Int\n{body}    value = 2\n");
        let error = compile(&source).expect_err("a possible read still requires initialization");
        assert!(error.contains("not initialized"), "{error}\n{source}");
    }
}
