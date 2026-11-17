//! Partial callback shapes constrain calls without creating signature adapters.

use super::compile;
use crate::model::{GenericParam, Type};
use std::collections::BTreeMap;

#[test]
fn known_callback_inputs_infer_results_before_contextual_widening() {
    let module = compile(
        r#"fun <T> invoke(factory: (Int) -> T) -> T = factory(1)
fun number(value: Int) -> Int = value
fun number(value: Float) -> Float = value
fun inferred() = invoke(::number)
scene start():
    val widened: Float = invoke(::number)
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.inferred"].signature.result, Type::Int);
}

#[test]
fn generic_reference_candidates_infer_their_own_parameters_from_partial_shapes() {
    compile(
        r#"fun <T> invoke(factory: (Int) -> T) -> T = factory(1)
fun <U: Equatable> number(value: U) -> U = value
fun number(value: Float) -> Float = value
scene start():
    val widened: Float = invoke(::number)
"#,
    )
    .unwrap();
}

#[test]
fn later_supplied_arguments_constrain_earlier_callback_references() {
    compile(
        r#"fun <T> apply(callback: (T) -> T, value: T) -> T = callback(value)
fun choose(value: Int) -> Int = value
fun choose(value: String) -> String = value
scene start():
    val first = apply(::choose, 3)
    val second = apply(::choose, "text")
"#,
    )
    .unwrap();
}

#[test]
fn deferred_callbacks_propagate_constraints_in_reverse_parameter_order() {
    let module = compile(
        r#"fun <A, B> chain(second: (A) -> B, first: (Int) -> A) -> B = second(first(1))
fun begin(value: Int) -> Int = value
fun begin(value: Float) -> Float = value
fun finish(value: Int) -> String = "done"
fun finish(value: String) -> Int = value.length
fun result() = chain(::finish, ::begin)
scene start():
    Unit
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.result"].signature.result, Type::String);
}

#[test]
fn callback_constraints_preserve_input_variance_and_suspension_widening() {
    compile(
        r#"fun <T> invoke(factory: (Int) -> T) -> T = factory(1)
suspend fun <T> delayed(factory: suspend (Int) -> T) -> T = factory(1)
fun choose(value: Int?) -> String = "present"
fun choose(value: Float) -> Float = value
scene start():
    val ordinary = invoke(::choose)
    val effect = delayed(::choose)
"#,
    )
    .unwrap();
}

#[test]
fn variadic_callback_shape_is_preserved_during_partial_inference() {
    compile(
        r#"fun <T> invoke(factory: (vararg Int) -> T) -> T = factory(1, 2)
fun choose(vararg values: Int) -> Int = values.size
fun choose(first: Float, second: Float) -> Float = first + second
scene start():
    val widened: Float = invoke(::choose)
"#,
    )
    .unwrap();
}

#[test]
fn named_spread_and_nested_literal_callbacks_use_the_same_constraints() {
    compile(
        r#"fun <T> invoke(vararg factories: (Int) -> T) -> T = factories[0](1)
fun <T> nested(factories: List<List<(Int) -> T>>) -> T = factories[0][0](1)
fun <T> mapped(factories: Map<String, (Int) -> T>) -> T = (factories["call"]!!)(1)
fun number(value: Int) -> Int = value
fun number(value: Float) -> Float = value
scene start():
    val spread: Float = invoke(*[::number])
    val named: Float = invoke(factories = [::number])
    val deep: Float = nested([[::number]])
    val mapping: Float = mapped({"call": ::number})
"#,
    )
    .unwrap();
}

#[test]
fn explicit_callback_type_arguments_remain_fixed_while_arity_is_resolved() {
    compile(
        r#"fun <T> invoke(factory: (Int) -> T) -> T = factory(1)
fun <U> choose(value: U) -> U = value
fun <U> choose(value: U, extra: U) -> U = value
scene start():
    val widened: Float = invoke(::choose<Int>)
"#,
    )
    .unwrap();
}

#[test]
fn inaccessible_methods_do_not_participate_in_partial_callback_constraints() {
    compile(
        r#"data class Source:
    fun choose(value: Int) -> Int = value
    private fun <U> choose(value: U) -> U = value
fun <T> invoke(factory: (Int) -> T) -> T = factory(1)
scene start():
    val source = Source()
    val inferred = invoke(source::choose)
"#,
    )
    .unwrap();
}

#[test]
fn deferred_method_receivers_keep_source_order_flow_invalidation() {
    let source = r#"data class Source:
    fun choose(value: Int) -> Int = value
    fun choose(value: Float) -> Float = value
fun <T> invoke(factory: (Int) -> T) -> T = factory(1)
scene start():
    var text: String? = "safe"
    val source = Source()
    fun clear() -> Source:
        text = null
        return source
    if text != null:
        val value = invoke(clear()::choose)
        prose:
            ${text.length}
"#;
    let error = compile(source).expect_err("receiver evaluation invalidates the earlier null proof");
    assert!(error.contains("nullable member receiver"), "{error}");
}

#[test]
fn common_callback_constraints_do_not_hide_reference_ambiguity() {
    let error = compile(
        r#"fun <T> invoke(factory: (Int) -> T) -> T = factory(1)
fun choose(value: Int) -> Int = value
fun choose(value: Int?) -> Int = value ?: 0
scene start():
    val result = invoke(::choose)
"#,
    )
    .expect_err("both references satisfy the same callback type");
    assert!(error.contains("ambiguous"), "{error}");
}

#[test]
fn callback_constraints_do_not_create_adapters_or_override_explicit_call_arguments() {
    for call in ["invoke<Float>(::choose)", "invoke(::choose)"] {
        let source = format!(
            r#"fun <T: Float> invoke(factory: (Int) -> T) -> T = factory(1)
fun choose(value: Int) -> Int = value
fun choose(value: Float) -> Float = value
scene start():
    val result: Float = {call}
"#,
        );
        assert!(compile(&source).is_err(), "a signature adapter was invented:\n{source}");
    }
}

#[test]
fn recursive_callback_constraints_report_the_existing_type_depth_budget() {
    let error = compile(
        r#"fun <U> wrap(value: U) -> List<U> = [value]
fun <U> wrap(value: U, extra: Int) -> U = value
fun <A, B> cycle(first: (A) -> B, second: (B) -> A, initial: A) -> A = initial
scene start():
    val initial: List<Nothing> = []
    val result = cycle(::wrap, ::wrap, initial)
"#,
    )
    .expect_err("infinite inferred nesting has no finite source type");
    assert!(error.contains("type graph budget"), "{error}");
}

#[test]
fn callback_constraints_preserve_enclosing_generic_parameter_identity() {
    compile(
        r#"fun <U> invoke(factory: (Int) -> U) -> U = factory(1)
fun <T> outer(value: T) -> T:
    fun choose(input: Int) -> T = value
    fun choose(input: Float) -> Float = input
    val selected = invoke(::choose)
    return selected
scene start():
    val result = outer("retained")
"#,
    )
    .unwrap();
}

#[test]
fn partial_constraints_do_not_bind_unknown_nullable_refinements() {
    let parameter = GenericParam {
        name: "callee#U".into(),
        variance: 0,
        bounds: vec![],
    };
    let unknown = Type::Parameter {
        name: "pending#T".into(),
        nonnull: true,
    };
    let mut arguments = BTreeMap::new();
    crate::generics::infer_partial(
        &Type::parameter(&parameter.name),
        &unknown,
        &[parameter],
        &mut arguments,
        &["pending#T".into()],
    )
    .unwrap();
    assert!(arguments.is_empty());
}

#[test]
fn partial_constraints_do_not_leak_unknown_parameters_inside_composites() {
    let parameter = GenericParam {
        name: "callee#U".into(),
        variance: 0,
        bounds: vec![],
    };
    let actual = Type::named("List", vec![Type::parameter("pending#T")]);
    let mut arguments = BTreeMap::new();
    crate::generics::infer_partial(
        &Type::parameter(&parameter.name),
        &actual,
        std::slice::from_ref(&parameter),
        &mut arguments,
        &["pending#T".into()],
    )
    .unwrap();
    assert!(arguments.is_empty());
    crate::generics::infer(&Type::parameter(&parameter.name), &actual, &[parameter], &mut arguments).unwrap();
    assert_eq!(arguments["callee#U"], actual);
}
