//! A completed generic result may convert without adapting supplied values.

use super::compile;

#[test]
fn generic_result_context_does_not_force_callback_adapters() {
    compile(
        r#"fun <T> produce(factory: () -> T) -> T = factory()
fun number() -> Int = 3
fun letter() -> Char = 'a'
fun numbers() -> List<Int> = [1, 2]
scene start():
    val numeric: Float = produce(::number)
    val text: String = produce(::letter)
    val values: List<Float> = produce(::numbers)
    val optional: Float? = produce(::number)
    val copied: MutableList<Float> = produce(::numbers)
"#,
    )
    .unwrap();
}

#[test]
fn generic_result_context_preserves_invariant_inputs_and_bound_method_references() {
    compile(
        r#"data class Box<T>:
    val value: T
fun <T> read(holder: Box<T>) -> T = holder.value
data class Source:
    val value: Int
    fun read() -> Int = value
fun <T> produce(factory: () -> T) -> T = factory()
scene start():
    val holder = Box(3)
    val first: Float = read(holder)
    val source = Source(4)
    val second: Float = produce(source::read)
"#,
    )
    .unwrap();
}

#[test]
fn generic_result_context_retains_supplied_vararg_and_named_group_types() {
    compile(
        r#"fun <T> first(vararg holders: MutableList<T>) -> T = holders[0][0]
scene start():
    val values = mutable [3]
    val direct: Float = first(values)
    val spread: Float = first(*[values])
    val named: Float = first(holders = [values])
"#,
    )
    .unwrap();
}

#[test]
fn successful_contextual_specialization_and_missing_arguments_keep_their_existing_rules() {
    compile(
        r#"fun <T> keep(value: T) -> T = value
fun <T> missing() -> T? = null
fun <T, U> combine(value: T) -> U? = null
scene start():
    val numeric: Float = keep(3)
    val absent: String? = missing()
    val dependent: Float? = combine(3)
"#,
    )
    .unwrap();
}

#[test]
fn result_context_retry_cannot_override_explicit_arguments_or_declared_bounds() {
    for source in [
        "fun <T> produce(factory: () -> T) -> T = factory()\nfun number() -> Int = 3\nscene start():\n    val value: Float = produce<Float>(::number)\n",
        "fun <T: Float> produce(factory: () -> T) -> T = factory()\nfun number() -> Int = 3\nscene start():\n    val value: Float = produce(::number)\n",
        "fun <T> produce(factory: (T) -> T, value: T) -> T = factory(value)\nfun number(value: Int) -> Int = value\nscene start():\n    val value: Float = produce(::number, 3.5)\n",
        "fun <T> produce(factory: () -> T) -> T = factory()\nfun number() -> Int = 3\nscene start():\n    val value: String = produce(::number)\n",
    ] {
        assert!(compile(source).is_err(), "invalid generic call accepted:\n{source}");
    }
}

#[test]
fn supplied_first_retry_still_infers_result_only_parameters() {
    compile(
        r#"fun <T: StableKey, U> empty(factory: () -> T) -> Map<T, U>:
    factory()
    return {}
fun number() -> Int = 3
scene start():
    val result: Map<Float, String> = empty(::number)
"#,
    )
    .unwrap();
}
