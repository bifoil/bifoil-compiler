//! Generic calls combine lower value constraints with contravariant inputs.

use super::compile;
use crate::model::Type;

#[test]
fn callback_input_constraints_intersect_instead_of_joining() {
    let module = compile(
        r#"fun <T> combine(first: (T) -> Int, second: (T) -> Int, value: T) -> T:
    first(value)
    second(value)
    return value
fun narrow(value: Int) -> Int = value
fun broad(value: Int?) -> Int = value ?: 0
fun result() = combine(::narrow, ::broad, 1)
scene start():
    Unit
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.result"].signature.result, Type::Int);
}

#[test]
fn stored_callbacks_and_argument_order_preserve_the_same_input_constraint() {
    compile(
        r#"fun <T> combine(value: T, first: (T) -> Int, second: (T) -> Int) -> T = value
fun narrow(value: Int) -> Int = value
fun broad(value: Int?) -> Int = value ?: 0
scene start():
    val one: (Int) -> Int = ::narrow
    val two: (Int?) -> Int = ::broad
    val result: Int = combine(1, one, two)
"#,
    )
    .unwrap();
}

#[test]
fn callback_inputs_do_not_create_numeric_or_container_adapters() {
    for (parameter, callback, value) in [
        ("Int", "Float", "1"),
        ("Float", "Int", "1.0"),
        ("List<Int>", "MutableList<Int>", "[1]"),
    ] {
        let source = format!(
            r#"fun <T> combine(first: (T) -> Int, second: (T) -> Int, value: T) -> Int = 0
fun narrow(value: {parameter}) -> Int = 1
fun broad(value: {callback}) -> Int = 2
scene start():
    val result = combine(::narrow, ::broad, {value})
"#
        );
        assert!(compile(&source).is_err(), "signature adapter accepted:\n{source}");
    }
}

#[test]
fn explicit_type_arguments_are_not_replaced_by_directional_inference() {
    let source = r#"fun <T> combine(first: (T) -> Int, second: (T) -> Int, value: T) -> Int = 0
fun narrow(value: Int) -> Int = value
fun broad(value: Int?) -> Int = value ?: 0
scene start():
    val result = combine<Int?>(::narrow, ::broad, 1)
"#;
    assert!(compile(source).is_err());
}

#[test]
fn directional_inference_can_widen_values_but_not_callbacks() {
    let module = compile(
        r#"fun <T> combine(first: (T) -> Int, second: (T) -> Int, value: T) -> T = value
fun narrow(value: Float) -> Int = 1
fun broad(value: Float?) -> Int = 2
fun result() = combine(::narrow, ::broad, 1)
scene start():
    Unit
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.result"].signature.result, Type::Float);
}

#[test]
fn dependent_bounds_do_not_invent_unsupplied_type_arguments() {
    assert!(
        compile(
            r#"fun <T: Any> unused() -> Int = 1
scene start():
    val result = unused()
"#
        )
        .is_err()
    );
}

#[test]
fn jointly_constrained_overloaded_callbacks_have_one_complete_solution() {
    let module = compile(
        r#"fun <T> together(first: (T) -> T, second: (T) -> T) -> (T) -> T = first
fun chooseFirst(value: Int) -> Int = value
fun chooseFirst(value: String) -> String = value
fun chooseSecond(value: Int) -> Int = value
fun chooseSecond(value: Boolean) -> Boolean = value
fun result() = together(::chooseFirst, ::chooseSecond)
scene start():
    Unit
"#,
    )
    .unwrap();
    let Type::Function { params, result, .. } = &module.functions["test.result"].signature.result else {
        panic!("function result");
    };
    assert_eq!(params.as_ref(), &[Type::Int]);
    assert_eq!(result.as_ref(), &Type::Int);
}

#[test]
fn jointly_constrained_inputs_and_nested_literal_references_are_resolved_together() {
    compile(
        r#"fun <T> together(first: (T) -> T, second: (T) -> T) -> Unit = Unit
fun <T> nested(first: List<List<(T) -> T>>, second: Map<String, (T) -> T>) -> Unit = Unit
fun chooseFirst(value: Int) -> Int = value
fun chooseFirst(value: String) -> String = value
fun chooseSecond(value: Int) -> Int = value
fun chooseSecond(value: Boolean) -> Boolean = value
scene start():
    together(::chooseFirst, ::chooseSecond)
    nested([[::chooseFirst]], {"call": ::chooseSecond})
"#,
    )
    .unwrap();
}

#[test]
fn multiple_complete_generic_solutions_remain_ambiguous() {
    let error = compile(
        r#"fun <T> together(first: (T) -> T, second: (T) -> T) -> (T) -> T = first
fun chooseFirst(value: Int) -> Int = value
fun chooseFirst(value: String) -> String = value
fun chooseSecond(value: Int) -> Int = value
fun chooseSecond(value: String) -> String = value
scene start():
    val result = together(::chooseFirst, ::chooseSecond)
"#,
    )
    .expect_err("Int and String both satisfy the complete call");
    assert!(error.contains("ambiguous"), "{error}");
}

#[test]
fn result_context_widens_a_completed_directional_result() {
    compile(
        r#"fun <T> together(first: (T) -> Int, second: (T) -> Int, value: T) -> T = value
fun narrow(value: Int) -> Int = value
fun broad(value: Int?) -> Int = value ?: 0
scene start():
    val result: Float = together(::narrow, ::broad, 1)
"#,
    )
    .unwrap();
}

#[test]
fn directional_constraints_preserve_enclosing_parameter_identity() {
    compile(
        r#"fun <U> together(first: (U) -> Int, second: (U) -> Int, value: U) -> U = value
fun <T> outer(value: T) -> T:
    fun narrow(input: T) -> Int = 1
    fun broad(input: Any?) -> Int = 2
    return together(::narrow, ::broad, value)
scene start():
    val result = outer(3)
"#,
    )
    .unwrap();
}

#[test]
fn generic_container_inference_keeps_value_copying_distinct_from_literal_construction() {
    compile(
        r#"fun <T> first(value: MutableList<T>) -> T = value[0]
fun <T> fixed(value: MutableList<T>, item: T) -> T = value[0]
scene start():
    val constructed = first([1, 2])
    val original = [3, 4]
    val selected = fixed(original, 1)
"#,
    )
    .unwrap();
}

#[test]
fn generic_whole_value_joins_cannot_reconstruct_mutable_literals() {
    let error = compile(
        r#"fun <T> joined(left: T, right: T) -> T = left
scene start():
    val result = joined(mutable [1], mutable [2.0])
"#,
    )
    .expect_err("mutable collection arguments remain invariant during inference");
    assert!(error.contains("incompatible constraints"), "{error}");
}

#[test]
fn whole_collection_shapes_can_retain_joint_reference_alternatives() {
    compile(
        r#"fun <T> pair(left: T, right: T) -> T = left
fun chooseFirst(value: Int) -> Int = value
fun chooseFirst(value: String) -> String = value
fun chooseSecond(value: Int) -> Int = value
fun chooseSecond(value: Boolean) -> Boolean = value
scene start():
    val sequence = pair([::chooseFirst], [::chooseSecond])
    val mapping = pair({"f": ::chooseFirst}, {"g": ::chooseSecond})
    val nested = pair([{"f": ::chooseFirst}], [{"g": ::chooseSecond}])
"#,
    )
    .unwrap();
}

#[test]
fn whole_mutable_literals_can_choose_the_same_exact_callable_type() {
    compile(
        r#"fun <T> pair(left: T, right: T) -> T = left
fun chooseFirst(value: Int) -> Int = value
fun chooseFirst(value: String) -> String = value
fun chooseSecond(value: Int) -> Int = value
fun chooseSecond(value: Boolean) -> Boolean = value
scene start():
    val sequence = pair(mutable [::chooseFirst], mutable [::chooseSecond])
"#,
    )
    .unwrap();
}

#[test]
fn whole_literal_alternatives_do_not_hide_multiple_complete_solutions() {
    let error = compile(
        r#"fun <T> pair(left: T, right: T) -> T = left
fun chooseFirst(value: Int) -> Int = value
fun chooseFirst(value: String) -> String = value
fun chooseSecond(value: Int) -> Int = value
fun chooseSecond(value: String) -> String = value
scene start():
    val sequence = pair([::chooseFirst], [::chooseSecond])
"#,
    )
    .expect_err("both List<(Int) -> Int> and List<(String) -> String> fit");
    assert!(error.contains("ambiguous"), "{error}");
}

#[test]
fn whole_literal_receiver_effects_invalidate_prior_narrowing() {
    let error = compile(
        r#"fun <T> pair(left: T, right: T) -> T = left
data class Source:
    fun choose(value: Int) -> Int = value
    fun choose(value: String) -> String = value
fun chooseSecond(value: Int) -> Int = value
fun chooseSecond(value: Boolean) -> Boolean = value
scene start():
    var name: String? = "ready"
    fun reset() -> Source:
        name = null
        return Source()
    if name != null:
        val sequence = pair([reset()::choose], [::chooseSecond])
        prose:
            ${name.length}
"#,
    )
    .expect_err("creating the bound method invalidates the earlier proof");
    assert!(error.contains("nullable member receiver"), "{error}");
}
