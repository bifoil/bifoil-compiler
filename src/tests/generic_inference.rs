//! Generic inference follows scene shapes, sealed parents, and checked bounds.

use super::compile;
use crate::model::Type;

#[test]
fn scene_reference_shapes_infer_call_and_contextual_reference_arguments() {
    let module = compile(
        r#"fun <T> keep(value: Scene<T>) -> Scene<T> = value
fun forward(value: Scene<Int>) = keep(value)
fun reference() -> (Scene<Int>) -> Scene<Int> = ::keep
scene start():
    Unit
"#,
    )
    .unwrap();
    assert_eq!(
        module.functions["test.forward"].signature.result,
        Type::Scene {
            params: vec![Type::Int].into()
        }
    );
}

#[test]
fn sealed_parent_arguments_constrain_inputs_and_expected_variant_results() {
    compile(
        r#"sealed class Result<T>:
    case Success(value: T)
    case Empty
fun <T> known(value: Result<T>) -> Boolean = true
fun <T> empty() -> Result.Empty<T> = Result.Empty<T>()
scene start():
    val answer = known(Result.Success(3))
    val missing: Result<Int> = empty()
"#,
    )
    .unwrap();
}

#[test]
fn structural_patterns_infer_arguments_from_generic_upper_views() {
    let module = compile(
        r#"fun <T> first(value: List<T>) -> T = value[0]
fun <U: List<String>> forward(value: U) = first(value)
fun <T> call(value: () -> T) -> T = value()
fun <U: () -> Int> indirect(value: U) = call(value)
fun <T> keep(value: Scene<T>) -> Scene<T> = value
fun <U: Scene<Int>> sceneReference(value: U) = keep(value)
fun <T> identity(value: T) -> T = value
fun <U: List<String>> unchanged(value: U) -> U = identity(value)
scene start():
    Unit
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.forward"].signature.result, Type::String);
    assert_eq!(module.functions["test.indirect"].signature.result, Type::Int);
}

#[test]
fn structural_inference_does_not_authorize_unwrapping_copying_or_signature_adapters() {
    for source in [
        "fun <T> first(value: List<T>) -> T = value[0]\nfun <U: List<String>?> invalid(value: U) -> String = first(value)\n",
        "fun <T> first(value: MutableList<T>) -> T = value[0]\nfun invalid(value: List<Int>) = first(value)\n",
        "fun <T> invoke(value: (T) -> Float, item: T) -> Float = value(item)\nfun invalid(value: (Float) -> Float) -> Float = invoke<Int>(value, 3)\n",
    ] {
        let source = format!("{source}scene start():\n    Unit\n");
        assert!(compile(&source).is_err(), "invalid conversion accepted:\n{source}");
    }
}

#[test]
fn nested_generic_calls_infer_only_the_callees_own_parameters() {
    compile(
        r#"fun <T> outer(value: T) -> String:
    fun <U> inner(original: T, extra: U) -> String = String(original)
    return inner(value, 1)
scene start():
    val result = outer(3)
"#,
    )
    .unwrap();
    let error = compile(
        r#"fun <T> outer(value: T) -> String:
    fun <U> inner(original: T, extra: U) -> String = String(original)
    return inner("changed", 1)
scene start():
    val result = outer(3)
"#,
    )
    .expect_err("a nested generic call cannot choose a different enclosing T");
    assert!(error.contains("T") && error.contains("String"), "{error}");
}

#[test]
fn expected_results_preserve_original_generic_arguments_through_inferred_calls() {
    compile(
        r#"data class Box<T>:
    val value: T
fun <U> identity(item: U) -> U = item
fun <U> first(vararg items: U) -> U = items[0]
fun <T> preserve(value: T) -> T:
    if value is String:
        return identity(value)
    return value
fun <T> field(holder: Box<T>) -> T:
    if holder.value is String:
        return identity(holder.value!!)
    return holder.value
fun <T> spread(value: T) -> T:
    if value is String:
        return first(*[value])
    return value
scene start():
    Unit
"#,
    )
    .unwrap();
}

#[test]
fn provisional_argument_context_does_not_prevent_compatible_literal_joins() {
    let module = compile(
        r#"fun <T> second(first: T, values: List<T>) -> T = values[0]
fun joined() = second(1, [2.5])
scene start():
    Unit
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.joined"].signature.result, Type::Float);
}

#[test]
fn expected_generic_results_do_not_give_computed_values_an_original_identity() {
    let error = compile(
        r#"fun <U> identity(item: U) -> U = item
fun <T> invalid(value: T) -> T:
    if value is String:
        return identity(value + "changed")
    return value
scene start():
    Unit
"#,
    )
    .expect_err("newly computed strings cannot satisfy a caller-selected T");
    assert!(error.contains("argument requires T, found String"), "{error}");
}

#[test]
fn nullable_input_arguments_are_not_stripped_by_a_nonnull_result_context() {
    compile(
        r#"data class Box<T>:
    val value: T
fun <T> read(holder: Box<T>) -> T = holder.value
fun <T> required(holder: Box<T>) = holder.value!!
fun checked(holder: Box<Int?>) -> Int = required(holder)
fun optional(holder: Box<String?>) -> String? = read(holder)
scene start():
    val present = read(Box<String?>("Ada"))
    val missing = read(Box<String?>(null))
    val number: Int = required(Box<Int?>(7))
"#,
    )
    .unwrap();
}
