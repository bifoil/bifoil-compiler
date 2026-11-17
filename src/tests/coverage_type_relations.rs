//! Independent finite models for subtype, conversion and cast-overlap contracts.
use crate::model::Type;
use std::sync::Arc;

fn function(params: Vec<Type>, result: Type, suspend: bool, vararg: bool) -> Type {
    Type::Function {
        params: params.into(),
        result: Arc::new(result),
        suspend,
        vararg,
    }
}

#[test]
fn coverage_type_relations_separate_scalar_subtyping_from_value_widening() {
    let scalars = [
        Type::Int,
        Type::Float,
        Type::Boolean,
        Type::String,
        Type::Char,
        Type::Unit,
    ];
    for source in &scalars {
        for target in &scalars {
            assert_eq!(source.subtype_of(target), source == target, "{source} <: {target}");
            let widening = matches!((source, target), (Type::Int, Type::Float) | (Type::Char, Type::String));
            let rank = if source == target {
                Some(0)
            } else if widening {
                Some(1)
            } else {
                None
            };
            assert_eq!(target.conversion_from(source), rank, "{source} -> {target}");
            assert!(source.subtype_of(&Type::Any));
            assert!(source.subtype_of(&source.nullable()));
            assert!(!source.nullable().subtype_of(source));
        }
    }
}

#[test]
fn coverage_type_relations_distinguish_empty_covariant_values_from_invariant_arguments() {
    for left in [Type::Int, Type::Float, Type::String, Type::Boolean] {
        for right in [Type::Int, Type::Float, Type::String, Type::Boolean] {
            // Empty immutable containers can be exposed through both covariant
            // views; mutable containers retain one exact construction argument.
            assert!(Type::named("List", vec![left.clone()]).may_overlap(&Type::named("List", vec![right.clone()])));
            assert_eq!(
                Type::named("MutableList", vec![left.clone()])
                    .may_overlap(&Type::named("MutableList", vec![right.clone()])),
                left == right
            );
            let a = Type::named("MutableList", vec![Type::named("List", vec![left.clone()])]);
            let b = Type::named("MutableList", vec![Type::named("List", vec![right.clone()])]);
            assert_eq!(a.may_overlap(&b), left == right);
            let optional_a = Type::named("MutableList", vec![left.nullable()]);
            let optional_b = Type::named("MutableList", vec![right.nullable()]);
            assert_eq!(optional_a.may_overlap(&optional_b), left == right);
        }
    }
    let concrete = Type::named("MutableList", vec![Type::Int]);
    assert!(concrete.may_overlap(&Type::named("MutableList", vec![Type::Star])));
    assert!(Type::named("MutableList", vec![Type::Star]).may_overlap(&concrete));
    assert!(!concrete.may_overlap(&Type::named("MutableMap", vec![Type::Int, Type::Int])));
}

#[test]
fn coverage_type_relations_preserve_callable_shape_and_scene_input_identity() {
    for left in [Type::Int, Type::String, Type::Int.nullable()] {
        for right in [Type::Float, Type::String, Type::String.nullable()] {
            for suspended in [false, true] {
                let a = function(vec![left.clone()], Type::String, false, false);
                let b = function(vec![right.clone()], Type::Int, suspended, false);
                // A broad-input function that never returns may inhabit both.
                assert!(a.may_overlap(&b));
                assert!(b.may_overlap(&a));
                assert!(!a.may_overlap(&function(vec![], Type::Int, suspended, false)));
                assert!(!a.may_overlap(&function(vec![right.clone()], Type::Int, suspended, true)));
                let scene_a = Type::Scene {
                    params: vec![left.clone()].into(),
                };
                let scene_b = Type::Scene {
                    params: vec![right.clone()].into(),
                };
                assert_eq!(scene_a.may_overlap(&scene_b), left == right);
                let slot_a = Type::named("MutableList", vec![a.clone()]);
                let slot_b = Type::named("MutableList", vec![b.clone()]);
                assert!(!slot_a.may_overlap(&slot_b));
                assert!(!Type::named("MutableList", vec![scene_a]).may_overlap(&Type::named(
                    "MutableList",
                    vec![Type::Scene {
                        params: vec![right.clone(), Type::Int].into()
                    }]
                )));
            }
        }
    }
}

#[test]
fn coverage_type_relations_keep_unknown_parameters_conservative_and_null_explicit() {
    let parameter = Type::parameter("Unconstrained");
    for concrete in [
        Type::Int,
        Type::String,
        Type::Unit,
        Type::named("List", vec![Type::Boolean]),
    ] {
        assert!(parameter.may_overlap(&concrete));
        assert!(concrete.may_overlap(&parameter));
        assert!(!Type::Null.may_overlap(&concrete));
        assert!(!concrete.may_overlap(&Type::Null));
        assert!(!concrete.may_overlap(&Type::Nothing));
        // A bottom-valued operand cannot reach the runtime cast operation.
        assert!(Type::Nothing.may_overlap(&concrete));
    }
    assert!(Type::Null.may_overlap(&Type::Int.nullable()));
    assert!(Type::Int.nullable().may_overlap(&Type::String.nullable()));
    assert!(Type::Pending.may_overlap(&Type::String));
    assert!(Type::String.may_overlap(&Type::Pending));
}

#[test]
fn coverage_invariant_callable_arguments_require_matching_effects_results_and_parameters() {
    let container = |ty| Type::named("MutableList", vec![ty]);
    let baseline = container(function(vec![Type::Int], Type::String, false, false));
    for other in [
        function(vec![Type::Float], Type::String, false, false),
        function(vec![Type::Int], Type::String, true, false),
        function(vec![Type::Int], Type::String, false, true),
        function(vec![Type::Int], Type::Int, false, false),
    ] {
        assert!(!baseline.may_overlap(&container(other)));
    }
    let scene = |input| {
        container(Type::Scene {
            params: vec![input].into(),
        })
    };
    assert!(!scene(Type::Int).may_overlap(&scene(Type::Float)));
    assert!(
        !container(Type::named("List", vec![Type::Int])).may_overlap(&container(Type::named("Set", vec![Type::Int])))
    );
}

#[test]
fn coverage_deep_overlap_queries_remain_conservative_instead_of_overflowing_the_stack() {
    let mut left = Type::Int;
    let mut right = Type::String;
    for _ in 0..70 {
        left = Type::named("MutableList", vec![left]);
        right = Type::named("MutableList", vec![right]);
    }
    assert!(left.may_overlap(&right));
}

#[test]
fn coverage_return_joins_agree_across_named_if_when_and_anonymous_bodies() {
    let scalars = [
        ("1", Type::Int),
        ("1.5", Type::Float),
        ("true", Type::Boolean),
        ("\"Original\"", Type::String),
        ("'x'", Type::Char),
        ("Unit", Type::Unit),
    ];
    for (left, left_type) in &scalars {
        for (right, right_type) in &scalars {
            // The source-level common-type contract admits only identity and
            // the two approved scalar widenings; no arbitrary Any fallback.
            let expected = if left_type == right_type {
                Some(left_type.clone())
            } else if matches!(
                (left_type, right_type),
                (Type::Int, Type::Float) | (Type::Float, Type::Int)
            ) {
                Some(Type::Float)
            } else if matches!(
                (left_type, right_type),
                (Type::Char, Type::String) | (Type::String, Type::Char)
            ) {
                Some(Type::String)
            } else {
                None
            };
            for body in [
                format!(
                    "fun choose(flag: Boolean):\n    if flag:\n        return {left}\n    else:\n        return {right}\n"
                ),
                format!("fun choose(flag: Boolean) = if flag:\n    {left}\nelse:\n    {right}\n"),
                format!(
                    "fun choose(flag: Boolean) = when flag:\n    case true:\n        {left}\n    else:\n        {right}\n"
                ),
                format!(
                    "fun choose(flag: Boolean):\n    val callback = (selected: Boolean) =>:\n        if selected:\n            return {left}\n        {right}\n    return callback(flag)\n"
                ),
            ] {
                let source = body + "scene start():\n    Unit\n";
                match (super::compile(&source), &expected) {
                    (Ok(module), Some(expected)) => {
                        assert_eq!(&module.functions["test.choose"].signature.result, expected, "{source}")
                    }
                    (Err(_), None) => {}
                    (Ok(_), None) => panic!("unrelated return types acquired an implicit common type:\n{source}"),
                    (Err(error), Some(_)) => panic!("compatible return paths were rejected: {error}\n{source}"),
                }
            }
        }
    }
}
