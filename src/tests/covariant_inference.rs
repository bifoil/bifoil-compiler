//! Expected result views do not replace already sufficient argument types.
use super::compile;
use crate::model::{Op, Type};

fn argument_types(source: &str) -> Vec<Type> {
    let module = compile(source).unwrap();
    module.functions["test.start"]
        .code
        .iter()
        .filter_map(|instruction| match &instruction.op {
            Op::Specialize { args } => Some(args.values().cloned()),
            _ => None,
        })
        .flatten()
        .collect()
}

#[test]
fn covariance_preserves_argument_inference_and_explicit_arguments_remain_authoritative() {
    let source = r#"data class Box<out T>:
    val value: T
fun <T> box(value: T) = Box(value)
scene start():
    val inferred: Box<Any> = box(3)
    val explicit: Box<Any> = box<Any>(4)
"#;
    assert_eq!(argument_types(source), vec![Type::Int, Type::Any]);
}

#[test]
fn numeric_context_and_invariant_positions_still_constrain_instantiation() {
    let source = r#"data class Box<out T>:
    val value: T
data class Cell<T>:
    var value: T
fun <T> box(value: T) = Box(value)
fun <T> cell(value: T) = Cell(value)
scene start():
    val numeric: Box<Float> = box(3)
    val invariant: Cell<Any> = cell(4)
"#;
    assert_eq!(argument_types(source), vec![Type::Float, Type::Any]);
}

#[test]
fn expected_types_fill_missing_parameters_without_replacing_supplied_covariant_arguments() {
    let source = r#"data class Pair<out T, out U>:
    val first: T
    val second: U?
fun <T, U> pair(value: T) -> Pair<T, U> = Pair(value, null)
scene start():
    val result: Pair<Any, String> = pair(3)
"#;
    assert_eq!(argument_types(source), vec![Type::Int, Type::String]);
}

#[test]
fn nullable_and_callable_results_use_subtyping_without_value_conversion_adapters() {
    let source = r#"data class Box<out T>:
    val value: T
fun <T> optional(value: T) -> Box<T>? = Box(value)
fun <T> supplier(value: T) -> () -> T = () => value
scene start():
    val optionalResult: Box<Any>? = optional(3)
    val callableResult: () -> Any = supplier(4)
"#;
    assert_eq!(argument_types(source), vec![Type::Int, Type::Int]);
}
