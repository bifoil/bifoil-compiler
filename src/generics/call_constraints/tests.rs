//! Inference retains observed types and separates value conversion from subtype bounds.
use super::*;

fn parameter() -> GenericParam {
    GenericParam {
        name: "T".into(),
        variance: 0,
        bounds: vec![],
    }
}

#[test]
fn coverage_constraints_join_observed_scalars_without_inventing_any_or_copying() {
    for (left, right, expected) in [
        (Type::Int, Type::Float, Some(Type::Float)),
        (Type::Char, Type::String, Some(Type::String)),
        (Type::Int, Type::Int.nullable(), Some(Type::Int.nullable())),
        (Type::Int, Type::String, None),
        (
            Type::named("MutableList", vec![Type::Int]),
            Type::named("MutableList", vec![Type::Float]),
            None,
        ),
        (
            Type::named("List", vec![Type::Int]),
            Type::named("MutableList", vec![Type::Int]),
            None,
        ),
    ] {
        let parameters = [parameter()];
        let mut constraints = Constraints::new(&parameters);
        constraints.argument(&Type::parameter("T"), &left, false).unwrap();
        constraints.argument(&Type::parameter("T"), &right, false).unwrap();
        match expected {
            Some(expected) => assert_eq!(constraints.solve().unwrap()["T"], expected),
            None => assert!(matches!(constraints.solve(), Err(Failure::Conflict))),
        }
    }
}

#[test]
fn coverage_constraints_preserve_arguments_under_a_compatible_expected_result() {
    let parameters = [parameter()];
    let mut constraints = Constraints::new(&parameters);
    constraints.argument(&Type::parameter("T"), &Type::Int, false).unwrap();
    constraints.context(&Type::parameter("T"), &Type::Any).unwrap();
    assert_eq!(constraints.solve().unwrap()["T"], Type::Int);
    let mut incompatible = Constraints::new(&parameters);
    incompatible
        .argument(&Type::parameter("T"), &Type::String, false)
        .unwrap();
    incompatible.context(&Type::parameter("T"), &Type::Int).unwrap();
    assert!(matches!(incompatible.solve(), Err(Failure::Conflict)));
    let unobserved = Constraints::new(&parameters);
    assert!(unobserved.solve().unwrap().is_empty());
}

#[test]
fn coverage_constraint_relations_separate_exact_subtype_and_copy_requirements() {
    let edge = |actual, direction, mode| Edge {
        actual,
        direction,
        mode,
        origin: Origin::Argument,
    };
    for direction in [Direction::Lower, Direction::Upper] {
        let exact = edge(Type::Int, direction, Mode::Subtype);
        assert!(satisfies(&Type::Int, &exact, false));
        assert!(!satisfies(&Type::Float, &exact, false));
    }
    let exact = edge(Type::Int, Direction::Exact, Mode::Value);
    assert!(satisfies(&Type::Int, &exact, false));
    assert!(!satisfies(&Type::Float, &exact, false));
    let numeric = edge(Type::Int, Direction::Lower, Mode::Key);
    assert!(satisfies(&Type::Float, &numeric, false));
    assert!(!satisfies(&Type::String, &numeric, false));
    let immutable = Type::named("List", vec![Type::Int]);
    let mutable = Type::named("MutableList", vec![Type::Int]);
    let copy = edge(immutable, Direction::Lower, Mode::Value);
    assert!(!satisfies(&mutable, &copy, false));
    assert!(satisfies(&mutable, &copy, true));
    let literal = edge(
        Type::named("MutableList", vec![Type::Int]),
        Direction::Lower,
        Mode::Construct,
    );
    assert!(satisfies(
        &Type::named("MutableList", vec![Type::Float]),
        &literal,
        false
    ));
    assert!(!satisfies(
        &Type::named("MutableList", vec![Type::String]),
        &literal,
        false
    ));
}

#[test]
fn coverage_contextual_construction_keeps_nullable_and_nested_family_contracts() {
    for (target, source, expected) in [
        (Type::Int.nullable(), Type::Null, true),
        (Type::Int, Type::Int.nullable(), false),
        (
            Type::named("MutableList", vec![Type::Float]),
            Type::named("MutableList", vec![Type::Int]),
            true,
        ),
        (
            Type::named("MutableList", vec![Type::String]),
            Type::named("MutableList", vec![Type::Int]),
            false,
        ),
        (
            Type::named("List", vec![Type::Float]),
            Type::named("List", vec![Type::Int]),
            true,
        ),
        (
            Type::named("MutableList", vec![Type::Float]).nullable(),
            Type::named("MutableList", vec![Type::Int]),
            true,
        ),
        (
            Type::named("MutableList", vec![Type::Float]).nullable(),
            Type::named("MutableList", vec![Type::Int]).nullable(),
            true,
        ),
    ] {
        assert_eq!(construction(&target, &source), expected, "{source} -> {target}");
    }
}

#[test]
fn coverage_callable_input_constraints_infer_safe_intersections_without_numeric_adapters() {
    let callable = |input: Type, result: Type| Type::Function {
        params: vec![input].into(),
        result: std::sync::Arc::new(result),
        suspend: false,
        vararg: false,
    };
    for (left, right, expected) in [
        (Type::Int.nullable(), Type::Int, Type::Int),
        (Type::Int, Type::String, Type::Nothing),
    ] {
        let parameters = [parameter()];
        let mut constraints = Constraints::new(&parameters);
        let pattern = callable(Type::parameter("T"), Type::Unit);
        constraints
            .argument(&pattern, &callable(left, Type::Unit), false)
            .unwrap();
        constraints
            .argument(&pattern, &callable(right, Type::Unit), false)
            .unwrap();
        assert_eq!(constraints.solve().unwrap()["T"], expected);
    }
    let parameters = [parameter()];
    let mut constraints = Constraints::new(&parameters);
    let pattern = callable(Type::Unit, Type::parameter("T"));
    constraints
        .argument(&pattern, &callable(Type::Unit, Type::Int), false)
        .unwrap();
    constraints
        .context(&pattern, &callable(Type::Unit, Type::Float))
        .unwrap();
    assert!(matches!(constraints.solve(), Err(Failure::Conflict)));
}

#[test]
fn coverage_mutable_contexts_allow_literal_construction_without_retyping_existing_containers() {
    for (literal, succeeds) in [(true, true), (false, false)] {
        let parameters = [parameter()];
        let mut constraints = Constraints::new(&parameters);
        let pattern = Type::named("MutableList", vec![Type::parameter("T")]);
        constraints
            .argument(&pattern, &Type::named("MutableList", vec![Type::Int]), literal)
            .unwrap();
        constraints
            .context(&pattern, &Type::named("MutableList", vec![Type::Float]))
            .unwrap();
        if succeeds {
            assert_eq!(constraints.solve().unwrap()["T"], Type::Float);
        } else {
            assert!(matches!(constraints.solve(), Err(Failure::Conflict)));
        }
    }
    let parameters = [parameter()];
    let mut constraints = Constraints::new(&parameters);
    let scene = |input| Type::Scene {
        params: vec![input].into(),
    };
    constraints
        .argument(&scene(Type::parameter("T")), &scene(Type::Int), false)
        .unwrap();
    constraints
        .context(&scene(Type::parameter("T")), &scene(Type::Float))
        .unwrap();
    assert!(matches!(constraints.solve(), Err(Failure::Conflict)));
}

#[test]
fn coverage_nominal_constraints_follow_variant_parents_in_both_directions() {
    use crate::model::TypeDef;
    use std::sync::Arc;
    let _session = crate::declarations::AnalysisSession::enter();
    let definition = |name: &str, kind: &str, parent: Option<Type>| {
        Arc::new(TypeDef {
            parameters: vec![GenericParam {
                name: name.into(),
                variance: 1,
                bounds: vec![],
            }],
            field_order: vec![],
            kind: kind.into(),
            fields: BTreeMap::new(),
            methods: BTreeMap::new(),
            private_methods: vec![],
            parent,
            variants: vec![],
        })
    };
    crate::declarations::install(BTreeMap::from([
        ("original.Outcome".into(), definition("P", "sum", None)),
        (
            "original.Outcome.Ready".into(),
            definition(
                "V",
                "variant",
                Some(Type::named("original.Outcome", vec![Type::parameter("V")])),
            ),
        ),
    ]));
    let parameters = [parameter()];
    let mut constraints = Constraints::new(&parameters);
    constraints
        .argument(
            &Type::named("original.Outcome", vec![Type::parameter("T")]),
            &Type::named("original.Outcome.Ready", vec![Type::Int]),
            false,
        )
        .unwrap();
    constraints
        .context(
            &Type::named("original.Outcome.Ready", vec![Type::parameter("T")]),
            &Type::named("original.Outcome", vec![Type::Any]),
        )
        .unwrap();
    assert_eq!(constraints.solve().unwrap()["T"], Type::Int);
}

#[test]
fn coverage_three_observations_do_not_recover_after_an_incompatible_join() {
    let parameters = [parameter()];
    let mut constraints = Constraints::new(&parameters);
    for actual in [Type::Int, Type::String, Type::Boolean] {
        constraints.argument(&Type::parameter("T"), &actual, false).unwrap();
    }
    assert!(matches!(constraints.solve(), Err(Failure::Conflict)));
    let mut invariant = Constraints::new(&parameters);
    for actual in [Type::Int, Type::String] {
        invariant
            .argument(
                &Type::named("MutableList", vec![Type::parameter("T")]),
                &Type::named("MutableList", vec![actual]),
                false,
            )
            .unwrap();
    }
    assert!(matches!(invariant.solve(), Err(Failure::Conflict)));
}

#[test]
fn coverage_unrepresentable_callback_meets_do_not_invent_signature_adapters() {
    let parameters = [parameter()];
    let callable = |input| Type::Function {
        params: vec![input].into(),
        result: std::sync::Arc::new(Type::Unit),
        suspend: false,
        vararg: false,
    };
    let mut constraints = Constraints::new(&parameters);
    for argument in [Type::Int, Type::String, Type::Boolean] {
        constraints
            .argument(&callable(Type::parameter("T")), &callable(callable(argument)), false)
            .unwrap();
    }
    assert!(matches!(constraints.solve(), Err(Failure::Conflict)));
}

#[test]
fn coverage_bounds_restrict_observations_without_seeding_unobserved_parameters() {
    let parameters = [GenericParam {
        name: "T".into(),
        variance: 0,
        bounds: vec![Type::Float, Type::named("Equatable", vec![])],
    }];
    assert!(Constraints::new(&parameters).solve().unwrap().is_empty());
    let mut constraints = Constraints::new(&parameters);
    constraints.argument(&Type::parameter("T"), &Type::Int, false).unwrap();
    assert_eq!(constraints.solve().unwrap()["T"], Type::Float);
    let mut invalid = Constraints::new(&parameters);
    invalid.argument(&Type::parameter("T"), &Type::String, false).unwrap();
    assert!(matches!(invalid.solve(), Err(Failure::Conflict)));
}
