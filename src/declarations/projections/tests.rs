//! Projected read views preserve bounds, invariance, and compact shared graphs.

use super::read_type;
use crate::model::{GenericParam, Type, TypeDef};
use std::collections::BTreeMap;
use std::sync::Arc;

fn definition(bound: Option<Type>) -> TypeDef {
    TypeDef {
        parameters: vec![GenericParam {
            name: "T".into(),
            variance: 0,
            bounds: bound.into_iter().collect(),
        }],
        field_order: vec![],
        kind: "data".into(),
        fields: BTreeMap::new(),
        methods: BTreeMap::new(),
        private_methods: vec![],
        parent: None,
        variants: vec![],
    }
}
fn repeated(levels: usize, leaf: Type) -> Type {
    let mut ty = leaf;
    for _ in 0..levels {
        ty = Type::named("Entry", vec![ty.clone(), ty]);
    }
    ty
}

#[test]
fn projected_shared_reads_keep_their_bounds_and_query_contexts() {
    let _session = crate::declarations::AnalysisSession::enter();
    let source = repeated(28, Type::parameter("T"));
    let receiver = Type::named("Box", vec![Type::Star]);
    assert_eq!(
        read_type(&source, &receiver, &definition(Some(Type::String))),
        repeated(28, Type::String)
    );
    assert_eq!(
        read_type(&source, &receiver, &definition(Some(Type::Float))),
        repeated(28, Type::Float)
    );
    assert_eq!(
        read_type(&source, &receiver, &definition(None)),
        repeated(28, Type::Any.nullable())
    );
}

#[test]
fn projected_reads_keep_invariant_slots_hidden_and_known_slots_available() {
    let _session = crate::declarations::AnalysisSession::enter();
    let receiver = Type::named("Box", vec![Type::Star]);
    let source = Type::named("MutableMap", vec![Type::parameter("T"), Type::String]);
    assert_eq!(
        read_type(&source, &receiver, &definition(Some(Type::Int))),
        Type::named("MutableMap", vec![Type::Star, Type::String])
    );
    let set = Type::named("Set", vec![Type::parameter("T")]);
    assert_eq!(
        read_type(&set, &receiver, &definition(None)),
        Type::named("Set", vec![Type::Star])
    );
    assert_eq!(
        read_type(&set, &receiver, &definition(Some(Type::String))),
        Type::named("Set", vec![Type::String])
    );
}

#[test]
fn projected_callable_results_preserve_effects_and_safe_input_views() {
    let _session = crate::declarations::AnalysisSession::enter();
    let receiver = Type::named("Box", vec![Type::Star]);
    let source = Type::Function {
        params: vec![Type::Int].into(),
        result: Arc::new(Type::parameter("T")),
        suspend: true,
        vararg: true,
    };
    let expected = Type::Function {
        params: vec![Type::Int].into(),
        result: Arc::new(Type::String),
        suspend: true,
        vararg: true,
    };
    assert_eq!(read_type(&source, &receiver, &definition(Some(Type::String))), expected);
    let unknown_input = Type::Function {
        params: vec![Type::parameter("T")].into(),
        result: Arc::new(Type::Int),
        suspend: false,
        vararg: false,
    };
    assert_eq!(
        read_type(&unknown_input, &receiver, &definition(Some(Type::String))),
        Type::Function {
            params: vec![Type::Nothing].into(),
            result: Arc::new(Type::Int),
            suspend: false,
            vararg: false,
        }
    );
}

#[test]
fn nullable_projected_callable_inputs_keep_effects_variadics_and_result_bounds() {
    let _session = crate::declarations::AnalysisSession::enter();
    let receiver = Type::named("Box", vec![Type::Star]);
    let source = Type::Function {
        params: vec![Type::parameter("T").nullable()].into(),
        result: Arc::new(Type::parameter("T")),
        suspend: true,
        vararg: true,
    };
    assert_eq!(
        read_type(&source, &receiver, &definition(Some(Type::String))),
        Type::Function {
            params: vec![Type::Nothing.nullable()].into(),
            result: Arc::new(Type::String),
            suspend: true,
            vararg: true,
        }
    );
}

#[test]
fn shared_nodes_keep_distinct_input_and_read_proofs_through_callable_variance() {
    let _session = crate::declarations::AnalysisSession::enter();
    let receiver = Type::named("Box", vec![Type::Star]);
    let shared = repeated(28, Type::parameter("T"));
    let source = Type::Function {
        params: vec![shared.clone()].into(),
        result: Arc::new(shared),
        suspend: true,
        vararg: false,
    };
    assert_eq!(
        read_type(&source, &receiver, &definition(Some(Type::String))),
        Type::Function {
            params: vec![repeated(28, Type::Nothing)].into(),
            result: Arc::new(repeated(28, Type::String)),
            suspend: true,
            vararg: false,
        }
    );
}

#[test]
fn projected_nonnull_results_preserve_their_present_value_refinement() {
    let _session = crate::declarations::AnalysisSession::enter();
    let receiver = Type::named("Box", vec![Type::Star]);
    let present = Type::parameter("T").non_null();
    assert_eq!(read_type(&present, &receiver, &definition(None)), Type::Any);
    assert_eq!(
        read_type(&present, &receiver, &definition(Some(Type::String.nullable()))),
        Type::String
    );
}

#[test]
fn coverage_projected_bounds_resolve_known_arguments_and_conservative_cycles() {
    let _session = crate::declarations::AnalysisSession::enter();
    let mut bounded = definition(Some(Type::parameter("U")));
    bounded.parameters.push(GenericParam {
        name: "U".into(),
        variance: 0,
        bounds: vec![Type::String],
    });
    let unknown = Type::named("Box", vec![Type::Star, Type::Star]);
    assert_eq!(read_type(&Type::parameter("T"), &unknown, &bounded), Type::String);
    let known = Type::named("Box", vec![Type::Star, Type::Int]);
    assert_eq!(read_type(&Type::parameter("T"), &known, &bounded), Type::Int);
    bounded.parameters[1].bounds = vec![Type::parameter("T")];
    assert_eq!(
        read_type(&Type::parameter("T"), &unknown, &bounded),
        Type::Any.nullable()
    );
}

#[test]
fn coverage_projected_capabilities_do_not_replace_value_bounds() {
    let _session = crate::declarations::AnalysisSession::enter();
    let receiver = Type::named("Box", vec![Type::Star]);
    let mut bounded = definition(None);
    bounded.parameters[0].bounds = vec![
        Type::named("Equatable", vec![]),
        Type::named("StableKey", vec![]),
        Type::Any,
        Type::String,
        Type::Any,
    ];
    assert_eq!(read_type(&Type::parameter("T"), &receiver, &bounded), Type::String);
    for (bound, expected) in [
        (Type::named("List", vec![Type::parameter("T")]), Type::Any),
        (
            Type::named("List", vec![Type::parameter("T")]).nullable(),
            Type::Any.nullable(),
        ),
    ] {
        bounded.parameters[0].bounds = vec![bound];
        assert_eq!(read_type(&Type::parameter("T"), &receiver, &bounded), expected);
    }
    assert!(super::hidden_parameters(&Type::Int, &bounded).is_empty());
}

#[test]
fn coverage_projected_input_views_retain_invariance_and_unavailable_scene_arguments() {
    let _session = crate::declarations::AnalysisSession::enter();
    let receiver = Type::named("Box", vec![Type::Star]);
    let bounded = definition(Some(Type::String));
    let invariant = Type::named("MutableList", vec![Type::parameter("T")]);
    assert_eq!(super::input_type(&invariant, &receiver, &bounded), Type::Nothing);
    let covariant = Type::named("List", vec![Type::parameter("T")]);
    assert_eq!(
        super::input_type(&covariant, &receiver, &bounded),
        Type::named("List", vec![Type::Nothing])
    );
    let scene = Type::Scene {
        params: vec![Type::parameter("T")].into(),
    };
    assert_eq!(read_type(&scene, &receiver, &bounded), Type::Any.nullable());
    assert_eq!(super::input_type(&scene, &receiver, &bounded), Type::Nothing);
    let unresolved = Type::named("Unresolved", vec![Type::parameter("T")]);
    assert_eq!(read_type(&unresolved, &receiver, &bounded), Type::Any);
}

#[test]
fn coverage_projected_methods_apply_lower_input_and_generic_bounds_without_mutating_headers() {
    let _session = crate::declarations::AnalysisSession::enter();
    let receiver = Type::named("Box", vec![Type::Star]);
    let bounded = definition(Some(Type::String));
    let signature = Arc::new(crate::model::SignatureData {
        id: "original.Box.read".into(),
        generics: vec![GenericParam {
            name: "U".into(),
            variance: 0,
            bounds: vec![Type::parameter("T"), Type::Any],
        }],
        params: vec![crate::model::Param {
            name: "value".into(),
            ty: Type::parameter("T"),
            mutable: false,
            default: true,
            vararg: false,
        }]
        .into(),
        result: Type::parameter("T"),
        suspend: true,
        scene: false,
    });
    let projected = super::method(&signature, &receiver, &bounded);
    assert_eq!(projected.generics[0].bounds, [Type::Nothing, Type::Any]);
    assert_eq!(projected.params[0].ty, Type::Nothing);
    assert_eq!(projected.result, Type::String);
    assert!(projected.suspend && projected.params[0].default);
    assert_eq!(signature.params[0].ty, Type::parameter("T"));
}
