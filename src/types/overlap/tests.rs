//! Exhausted optional proofs stay conservative; retained memo identities stay bounded.
use super::*;
use std::sync::Arc;

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
