//! Calling an explicit reference retains the function-value interface.

use super::compile;

#[test]
fn immediate_references_accept_explicit_arguments_and_retain_variadic_shape() {
    compile(
        r#"
fun read(value: Int = 7) -> Int = value
fun <T> identity(value: T) -> T = value
fun count(vararg values: Int) -> Int = values.size
data class Box:
    val value: Int
    fun read(amount: Int = 7) -> Int = this.value + amount
scene start():
    val box = Box(1)
    val a: Int = (::read)(3)
    val b: Int = (box::read)(4)
    val c: Int = (::identity<Int>)(5)
    val d: Int = (::count)(1, *[2, 3])
    val e: Int = read(value = 6)
    val f: Int = box.read()
"#,
    )
    .unwrap();
}

#[test]
fn immediate_references_do_not_expose_names_defaults_or_argument_based_overload_selection() {
    let prelude = r#"
fun read(value: Int = 7) -> Int = value
fun pick(value: Int) -> Int = value
fun pick(value: Float) -> Float = value
fun <T> identity(value: T) -> T = value
data class Box:
    val value: Int
    fun read(amount: Int = 7) -> Int = amount
"#;
    for (call, expected) in [
        ("(::read)()", "missing required argument"),
        ("(::read)(value = 3)", "do not expose parameter names"),
        ("(Box(1)::read)()", "missing required argument"),
        ("(Box(1)::read)(amount = 3)", "do not expose parameter names"),
        ("(::pick)(3)", "reference"),
        ("(::identity)(3)", "cannot infer type argument"),
    ] {
        let source = format!("{prelude}scene start():\n    val result = {call}\n");
        let error = compile(&source).expect_err("reference construction must finish before a value call");
        assert!(error.contains(expected), "{error}\n{source}");
    }
}
