//! Dependent bounds combine observations without adapting existing values.

use super::compile;
use crate::model::Type;

fn result_type(source: &str) -> Type {
    compile(source).unwrap().functions["test.result"]
        .signature
        .result
        .clone()
}

#[test]
fn numeric_observations_widen_the_target_of_a_dependent_bound() {
    let result = result_type(
        r#"fun <A, B: A> choose(first: A, second: B) -> A = first
fun result() = choose(1, 2.5)
scene start():
    Unit
"#,
    );
    assert_eq!(result, Type::Float);
}

#[test]
fn dependent_chains_propagate_before_reporting_a_conflict() {
    let result = result_type(
        r#"fun <A, B: A, C: B> choose(first: A, second: B, third: C) -> A = first
fun result() = choose(1, 2, 3.5)
scene start():
    Unit
"#,
    );
    assert_eq!(result, Type::Float);
}

#[test]
fn char_observations_use_the_existing_lossless_widening() {
    assert_eq!(
        result_type(
            r#"fun <A, B: A> choose(first: A, second: B) -> A = first
fun result() = choose('a', "b")
scene start():
    Unit
"#
        ),
        Type::String
    );
}

#[test]
fn a_structural_bound_supplies_a_missing_element_argument() {
    assert_eq!(
        result_type(
            r#"fun <A, B: List<A>> choose(values: B) -> Int = 1
fun result() = choose([2.5])
scene start():
    Unit
"#
        ),
        Type::Int
    );
}

#[test]
fn immutable_structural_bounds_allow_scalar_leaf_widening() {
    assert_eq!(
        result_type(
            r#"fun <A, B: List<List<A>>> choose(first: A, values: B) -> A = first
fun result() = choose(1, [[2.5]])
scene start():
    Unit
"#
        ),
        Type::Float
    );
}

#[test]
fn nominal_bound_arguments_can_widen_an_independent_scalar_input() {
    assert_eq!(
        result_type(
            r#"data class Box<T>:
    val value: T
fun <A, B: Box<A>> choose(first: A, value: B) -> A = first
fun result() = choose(1, Box(2.5))
scene start():
    Unit
"#
        ),
        Type::Float
    );
}

#[test]
fn function_bound_inputs_keep_their_contravariant_direction() {
    assert_eq!(
        result_type(
            r#"fun <A, B: (A) -> Int> choose(first: A, target: B) -> A = first
fun callback(value: Float) -> Int = 1
fun result() = choose(1, ::callback)
scene start():
    Unit
"#
        ),
        Type::Float
    );
}

#[test]
fn function_bound_results_supply_covariant_information() {
    assert_eq!(
        result_type(
            r#"fun <A, B: () -> A> choose(first: A, target: B) -> A = first
fun callback() -> Float = 2.5
fun result() = choose(1, ::callback)
scene start():
    Unit
"#
        ),
        Type::Float
    );
}

#[test]
fn dependent_bounds_do_not_convert_invariant_nominal_or_mutable_arguments() {
    for (bound, value) in [("Box<A>", "Box(1)"), ("MutableList<A>", "mutable [1]")] {
        let source = format!(
            "data class Box<T>:\n    val value: T\nfun <A, B: {bound}> choose(first: A, value: B) -> A = first\nscene start():\n    val result = choose(2.5, {value})\n"
        );
        assert!(compile(&source).is_err(), "invariant conversion accepted: {source}");
    }
}

#[test]
fn dependent_bounds_do_not_create_callback_return_adapters() {
    assert!(
        compile(
            r#"fun <A, B: () -> A> choose(first: A, target: B) -> A = first
fun callback() -> Int = 1
scene start():
    val result = choose(2.5, ::callback)
"#
        )
        .is_err()
    );
}

#[test]
fn explicit_type_arguments_still_control_dependent_bounds() {
    assert!(
        compile(
            r#"fun <A, B: A> choose(first: A, second: B) -> A = first
scene start():
    val result = choose<Int, Float>(1, 2.5)
"#
        )
        .is_err()
    );
}

#[test]
fn unobserved_bounds_do_not_invent_arguments() {
    assert!(
        compile(
            r#"fun <A: Any, B: A> choose() -> Int = 1
scene start():
    val result = choose()
"#
        )
        .is_err()
    );
}

#[test]
fn recursive_calls_keep_the_caller_parameter_identity() {
    compile(
        r#"fun <A, B: A> recurse(first: A, second: B, count: Int) -> A:
    if count == 0:
        return first
    return recurse(first, second, count - 1)
scene start():
    val result = recurse(1, 2.5, 2)
"#,
    )
    .unwrap();
}

#[test]
fn a_previously_valid_dependent_widening_keeps_its_result() {
    assert_eq!(
        result_type(
            r#"fun <A, B: A> choose(first: A, second: B) -> A = first
fun result() = choose(1.5, 2)
scene start():
    Unit
"#,
        ),
        Type::Float
    );
}

#[test]
fn direct_bound_cycles_remain_errors_without_inference_growth() {
    let error = compile(
        r#"fun <A: B, B: A> choose(first: A, second: B) -> A = first
scene start():
    Unit
"#,
    )
    .unwrap_err();
    assert!(error.contains("cyclic generic upper bounds"), "{error}");
}

#[test]
fn provisional_cyclic_interfaces_resolve_dependent_parameter_names_first() {
    use crate::{compiler, parser};
    use std::collections::BTreeMap;
    use std::time::Instant;

    let source = "export fun <A, B: A> choose(first: A, second: B) -> A = first\n";
    let ast = parser::parse(source, "src/test.bfl").unwrap();
    let seed = compiler::seed(&ast, "test").unwrap();
    let seed_scope = BTreeMap::from([("test".into(), &seed)]);
    let resolved = compiler::declaration_interface(ast.clone(), "test", &seed_scope).unwrap();
    let resolved_scope = BTreeMap::from([("test".into(), &resolved)]);
    compiler::interface(ast, "test", "src/test.bfl", &resolved_scope, Instant::now()).unwrap();
}

#[test]
fn a_provisional_interface_cannot_publish_an_invalid_unused_bound() {
    use crate::{compiler, parser};
    use std::collections::BTreeMap;
    use std::time::Instant;

    let source = "data class Box<T>:\n    val value: T\nexport fun <U: Box<Int, Float>> unused(item: U) -> U = item\n";
    let ast = parser::parse(source, "src/test.bfl").unwrap();
    let provisional = compiler::declaration_interface(ast.clone(), "test", &BTreeMap::new()).unwrap();
    let scope = BTreeMap::from([("test".into(), &provisional)]);
    let error = compiler::interface(ast, "test", "src/test.bfl", &scope, Instant::now()).unwrap_err();
    assert!(error.contains("requires 1 arguments"), "{error}");
}
