//! Exhausted optional proofs stay conservative; retained memo identities stay bounded.
use super::*;
use std::sync::Arc;

#[test]
fn unfinished_operands_and_unknown_invariant_arguments_keep_runtime_checks_available() {
    for operand in [Type::Nothing, Type::Pending] {
        assert!(operand.may_overlap(&Type::String));
    }
    assert!(Type::String.may_overlap(&Type::Pending));
    let mut query = OverlapQuery::default();
    assert!(query.check(Proof::ArgumentIdentity, &Type::Star, &Type::Int, 0));
    assert!(query.check(Proof::ArgumentIdentity, &Type::Int, &Type::Star, 0));
    assert!(!query.check(Proof::ArgumentIdentity, &Type::Int, &Type::String, 0));
    assert!(!query.check(Proof::ValueOverlap, &Type::Int, &Type::String, MAX_PROOF_DEPTH - 1));
    assert!(query.check(Proof::ValueOverlap, &Type::Int, &Type::String, MAX_PROOF_DEPTH));
}

#[test]
fn parameter_casts_use_resolved_views_for_missing_nullable_and_cyclic_bounds() {
    let _session = crate::declarations::AnalysisSession::enter();
    for (name, bounds) in [
        ("Numeric", vec![Type::Int]),
        ("OptionalNumeric", vec![Type::Int.nullable()]),
        ("CycleA", vec![Type::parameter("CycleB")]),
        ("CycleB", vec![Type::parameter("CycleA")]),
    ] {
        crate::generics::install(&crate::model::GenericParam {
            name: name.into(),
            variance: 0,
            bounds,
        });
    }
    for (name, view, string_overlap) in [
        ("Unregistered", Type::Any, true),
        ("Numeric", Type::Int, false),
        ("OptionalNumeric", Type::Int, false),
    ] {
        let parameter = Type::parameter(name);
        assert_eq!(crate::generics::member_view(&parameter), view);
        assert_eq!(parameter.may_overlap(&Type::String), string_overlap, "{name}");
        assert_eq!(Type::String.may_overlap(&parameter), string_overlap, "{name}");
    }
    // Bound validation rejects cycles before overlap is called. Its member-view
    // resolver also remains conservative while that catalog is being prepared.
    assert_eq!(crate::generics::member_view(&Type::parameter("CycleA")), Type::Any);
}

#[test]
fn variant_parent_overlap_checks_complete_reified_arguments_in_both_directions() {
    let _session = crate::declarations::AnalysisSession::enter();
    let declaration = |kind: &str, parent| crate::model::TypeDef {
        parameters: vec![],
        field_order: vec![],
        kind: kind.into(),
        fields: BTreeMap::new(),
        methods: BTreeMap::new(),
        private_methods: vec![],
        parent,
        variants: vec![],
    };
    let mut sum = declaration("sum", None);
    sum.parameters.push(crate::model::GenericParam {
        name: "T".into(),
        variance: 0,
        bounds: vec![],
    });
    crate::declarations::install(BTreeMap::from([
        ("Sum".into(), Arc::new(sum)),
        (
            "IntCase".into(),
            Arc::new(declaration("variant", Some(Type::named("Sum", vec![Type::Int])))),
        ),
        (
            "Sibling".into(),
            Arc::new(declaration("variant", Some(Type::named("Sum", vec![Type::Int])))),
        ),
        // Incomplete metadata cannot manufacture a nominal relationship.
        (
            "UnresolvedParent".into(),
            Arc::new(declaration("variant", Some(Type::Int))),
        ),
    ]));
    for (left, right, expected) in [
        (
            Type::named("IntCase", vec![]),
            Type::named("Sum", vec![Type::Int]),
            true,
        ),
        (
            Type::named("IntCase", vec![]),
            Type::named("Sum", vec![Type::String]),
            false,
        ),
        (Type::named("IntCase", vec![]), Type::named("Sibling", vec![]), false),
        (
            Type::named("UnresolvedParent", vec![]),
            Type::named("Other", vec![]),
            false,
        ),
    ] {
        assert_eq!(left.may_overlap(&right), expected, "{left} / {right}");
        assert_eq!(right.may_overlap(&left), expected, "{right} / {left}");
    }
}

#[test]
fn deep_invariant_arguments_keep_a_runtime_check_when_the_proof_budget_is_exhausted() {
    for depth in [4, 80] {
        let mut left = Type::Int;
        let mut right = Type::String;
        for _ in 0..depth {
            left = Type::named("MutableList", vec![left]);
            right = Type::named("MutableList", vec![right]);
        }
        assert_eq!(left.may_overlap(&right), depth == 80);
        assert_eq!(right.may_overlap(&left), depth == 80);
    }
}

#[test]
fn retained_query_identities_cannot_reuse_dropped_shapes_or_grow_past_the_cache_budget() {
    let mut query = OverlapQuery::default();
    for index in 0..MAX_QUERY_RELATIONSHIPS + 8 {
        let left = Type::named(&format!("original.Left{index}"), vec![Type::Int]);
        let right = Type::named(&format!("original.Right{index}"), vec![Type::String]);
        assert!(!query.check(Proof::ValueOverlap, &left, &right, 0));
    }
    assert_eq!(query.checked.len(), MAX_QUERY_RELATIONSHIPS);
    let left = Type::named("original.Shared", vec![Type::Int]);
    let right = Type::named("original.Shared", vec![Type::Int]);
    assert!(query.check(Proof::ValueOverlap, &left, &right, 0));
    assert_eq!(query.checked.len(), MAX_QUERY_RELATIONSHIPS);
}

#[test]
fn mismatched_reified_arities_never_compare_only_their_shared_argument_prefix() {
    for left_count in 0..3 {
        for right_count in 0..3 {
            let left = Type::named("original.Reified", vec![Type::Int; left_count]);
            let right = Type::named("original.Reified", vec![Type::Int; right_count]);
            assert_eq!(left.may_overlap(&right), left_count == right_count);
            assert_eq!(
                Type::named("MutableList", vec![left]).may_overlap(&Type::named("MutableList", vec![right])),
                left_count == right_count,
            );
        }
    }
}

#[test]
fn invariant_callable_arguments_retain_their_complete_parameter_shape() {
    for left_count in 0..3 {
        for right_count in 0..3 {
            let callable = |count| Type::Function {
                params: vec![Type::Int; count].into(),
                result: Arc::new(Type::String),
                suspend: false,
                vararg: false,
            };
            assert_eq!(
                Type::named("MutableList", vec![callable(left_count)])
                    .may_overlap(&Type::named("MutableList", vec![callable(right_count)])),
                left_count == right_count,
            );
        }
    }
}
