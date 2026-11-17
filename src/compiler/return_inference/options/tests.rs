//! Alternative return inference retains compatible values and rejects unbounded result sets.
use super::*;

#[test]
fn coverage_result_choices_join_values_without_universal_fallbacks_or_container_copies() {
    let mut choices = Choices::default();
    choices
        .add(vec![Type::Int.into(), Type::String.into(), Type::Int.into()])
        .unwrap();
    choices.add(vec![Type::Float.into()]).unwrap();
    assert!(
        choices.finish(Type::Unit) == vec![Type::Float.into()],
        "only Float should survive the join"
    );

    let mut missing = Choices::default();
    missing.add(vec![]).unwrap();
    assert!(missing.finish(Type::Unit).is_empty());

    let mut incompatible = Choices::default();
    incompatible
        .add(vec![Type::named("List", vec![Type::Int]).into()])
        .unwrap();
    incompatible
        .add(vec![Type::named("MutableList", vec![Type::Int]).into()])
        .unwrap();
    assert!(incompatible.finish(Type::Unit).is_empty());

    let mut transferred = Choices::default();
    transferred.add(vec![Type::Nothing.into()]).unwrap();
    transferred.add(vec![Type::String.into()]).unwrap();
    assert!(
        transferred.finish(Type::Unit) == vec![Type::String.into()],
        "Nothing must not alter the returned type"
    );
}

#[test]
fn coverage_result_choice_limits_produce_diagnostics_instead_of_unbounded_graphs() {
    let mut alternatives = Choices::default();
    let values = (0..257)
        .map(|index| Type::named(&format!("original.Result{index}"), vec![]).into())
        .collect();
    assert!(
        alternatives
            .add(values)
            .expect_err("too many result alternatives must fail")
            .contains("budget exceeded")
    );
    let mut deep = Type::Int;
    for _ in 0..35 {
        deep = Type::named("List", vec![deep]);
    }
    assert!(
        Choices::default()
            .add(vec![deep.into()])
            .unwrap_err()
            .contains("budget exceeded")
    );
}
