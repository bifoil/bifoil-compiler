//! Nullable projected inputs permit null without guessing a hidden type argument.

use super::compile;

#[test]
fn projected_nullable_method_inputs_accept_only_null() {
    compile(
        r#"data class Box<T: Any>:
    var value: T?
    fun replace(next: T? = null) -> T?:
        val previous = value
        value = next
        return previous
    fun count(vararg values: T?) -> Int = values.size
scene start():
    val original = Box<Int>(3)
    val hidden: Box<*> = original
    val previous: Any? = hidden.replace(null)
    hidden.replace()
    val reset: (Nothing?) -> Any? = hidden::replace
    reset(null)
    val count = hidden.count(null, null)
    val copied = hidden.copy(value = null)
    prose:
        ${previous != null}|${count}|${copied.value == null}
"#,
    )
    .unwrap();
}

#[test]
fn nullable_capable_hidden_parameters_do_not_guarantee_nullable_inputs() {
    for declaration in ["T", "T?"] {
        let argument = if declaration == "T" { "null" } else { "7" };
        let source = format!(
            "data class Box<T>:\n    val value: T\n    fun accept(next: {declaration}):\n        Unit\nscene start():\n    val hidden: Box<*> = Box(3)\n    hidden.accept({argument})\n"
        );
        assert!(compile(&source).is_err(), "hidden type was assumed: {source}");
    }
}

#[test]
fn projected_callback_fields_and_default_only_calls_keep_safe_function_types() {
    compile(
        r#"data class Callbacks<T: Any>:
    val callback: (T?) -> T?
    fun get() -> (T?) -> T? = callback
data class Box<T>:
    val value: T
    fun use(next: T = value) -> T = next
    fun count(vararg values: T) -> Int = values.size
scene start():
    val callbacks = Callbacks<Int>((value: Int?) => value)
    val hidden: Callbacks<*> = callbacks
    val direct: (Nothing?) -> Any? = hidden.callback
    val returned: (Nothing?) -> Any? = hidden.get()
    direct(null)
    returned(null)
    val box: Box<*> = Box(7)
    val previous: Any? = box.use()
    val copied = box.copy()
    val count: (vararg Nothing) -> Int = box::count
    prose:
        ${previous != null}|${copied is Box<Int>}|${box.count()}|${count()}
"#,
    )
    .unwrap();
}

#[test]
fn projected_inputs_follow_callable_and_immutable_container_variance() {
    compile(
        r#"data class Box<T>:
    val value: T
    fun <U> map(callback: (T) -> U) -> U = callback(value)
    fun list(values: List<T>) -> Int = values.size
    fun present() = value!!
scene start():
    val hidden: Box<*> = Box(7)
    val text = hidden.map((value: Any?) => String(value))
    val mapped: ((Any?) -> String) -> String = hidden::map
    mapped((value: Any?) => String(value))
    val count = hidden.list([])
    val present: Any = hidden.present()
"#,
    )
    .unwrap();
    let definitions = r#"data class Box<T>:
    val value: T
    fun map(callback: (T) -> String) -> String = callback(value)
    fun produce(callback: () -> T) -> T = callback()
    fun takeMutable(values: MutableList<T>) -> Int = values.size
scene start():
    val hidden: Box<*> = Box(7)
"#;
    compile(&format!("{definitions}    Unit\n")).unwrap();
    for operation in [
        "hidden.map((value: Int) => String(value))",
        "hidden.produce(() => 7)",
        "hidden.takeMutable(mutable [1])",
    ] {
        let error = compile(&format!("{definitions}    {operation}\n"))
            .expect_err("a projected call must not choose a hidden argument");
        assert!(
            error.contains("argument") || error.contains("Nothing"),
            "{operation}: {error}"
        );
    }
}

#[test]
fn projected_write_restrictions_do_not_make_mutable_fields_permanent_facts() {
    let error = compile(
        r#"data class Box<T>:
    var value: T
fun clear(box: Box<String?>):
    box.value = null
scene start():
    val original = Box<String?>("Ada")
    val hidden: Box<*> = original
    if hidden.value is String:
        clear(original)
        prose:
            ${hidden.value.length}
"#,
    )
    .expect_err("a concrete alias can invalidate a projected mutable-field guard");
    assert!(error.contains("nullable") || error.contains("length"), "{error}");
}

#[test]
fn immutable_projected_fields_keep_their_guards_through_calls_and_suspension() {
    compile(
        r#"data class Box<T>:
    val value: T
fun ping():
    Unit
scene start():
    val hidden: Box<*> = Box("Ada")
    if hidden.value is String:
        ping()
        choice:
            option "Continue":
                Unit
        prose:
            ${hidden.value.length}
"#,
    )
    .unwrap();
}
