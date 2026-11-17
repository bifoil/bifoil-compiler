//! Common inference types must remain sound across a finite independent type model.
use super::*;

fn callable(input: Type, result: Type, suspend: bool) -> Type {
    Type::Function {
        params: vec![input].into(),
        result: Arc::new(result),
        suspend,
        vararg: false,
    }
}

fn candidates() -> Vec<Type> {
    vec![
        Type::Int,
        Type::Float,
        Type::Boolean,
        Type::String,
        Type::Char,
        Type::Unit,
        Type::Any,
        Type::Nothing,
        Type::Null,
        Type::Int.nullable(),
        Type::Float.nullable(),
        Type::String.nullable(),
        Type::named("List", vec![Type::Int]),
        Type::named("List", vec![Type::Float]),
        Type::named("List", vec![Type::String]),
        Type::named("Set", vec![Type::Int]),
        Type::named("MutableList", vec![Type::Int]),
        Type::named("MutableList", vec![Type::Float]),
        Type::named("MutableList", vec![Type::Star]),
        Type::named("Map", vec![Type::String, Type::Int]),
        Type::named("Map", vec![Type::String, Type::Float]),
        Type::named("MutableMap", vec![Type::String, Type::Int]),
        Type::named("List", vec![Type::named("List", vec![Type::Int])]),
        callable(Type::Int, Type::String, false),
        callable(Type::Float, Type::String, false),
        callable(Type::Any, Type::Int, false),
        callable(Type::Int.nullable(), Type::String.nullable(), false),
        callable(Type::Int, Type::String, true),
        Type::Scene { params: vec![].into() },
        Type::Scene {
            params: vec![Type::Int].into(),
        },
        Type::Scene {
            params: vec![Type::Float].into(),
        },
    ]
}

#[test]
fn coverage_common_type_results_are_sound_for_all_finite_pairs() {
    let values = candidates();
    let mut query = Common::default();
    for left in &values {
        for right in &values {
            if let Some(joined) = query.join(left, right).unwrap() {
                assert!(
                    joined.conversion_from(left).is_some(),
                    "{left} cannot convert to inferred {joined}"
                );
                assert!(
                    joined.conversion_from(right).is_some(),
                    "{right} cannot convert to inferred {joined}"
                );
            }
            if let Some(meet) = query.meet(left, right).unwrap() {
                assert!(meet.subtype_of(left), "inferred {meet} is not a subtype of {left}");
                assert!(meet.subtype_of(right), "inferred {meet} is not a subtype of {right}");
            }
        }
    }
}

#[test]
fn coverage_common_types_do_not_invent_universal_types_or_mutable_copies() {
    let mut query = Common::default();
    assert_eq!(query.join(&Type::Int, &Type::Float).unwrap(), Some(Type::Float));
    assert_eq!(query.join(&Type::Int, &Type::String).unwrap(), None);
    assert_eq!(query.meet(&Type::Int, &Type::String).unwrap(), Some(Type::Nothing));
    let immutable = Type::named("List", vec![Type::Int]);
    let mutable = Type::named("MutableList", vec![Type::Int]);
    assert_eq!(query.join(&immutable, &mutable).unwrap(), None);
    let projection = Type::named("MutableList", vec![Type::Star]);
    assert_eq!(query.meet(&projection, &mutable).unwrap(), Some(mutable.clone()));
    assert_eq!(query.meet(&mutable, &projection).unwrap(), Some(mutable));
}

#[test]
fn coverage_common_types_report_excessive_depth_without_stack_growth() {
    let mut left = Type::Int;
    let mut right = Type::Float;
    for _ in 0..35 {
        left = Type::named("List", vec![left]);
        right = Type::named("List", vec![right]);
    }
    assert!(Common::default().join(&left, &right).is_err());
}

#[test]
fn coverage_nominal_variance_keeps_common_views_sound_without_inventing_any() {
    use crate::model::{GenericParam, TypeDef};
    let _session = crate::declarations::AnalysisSession::enter();
    let definition = |variance| {
        Arc::new(TypeDef {
            parameters: vec![GenericParam {
                name: "T".into(),
                variance,
                bounds: vec![],
            }],
            field_order: vec![],
            kind: "data".into(),
            fields: BTreeMap::new(),
            methods: BTreeMap::new(),
            private_methods: vec![],
            parent: None,
            variants: vec![],
        })
    };
    crate::declarations::install(BTreeMap::from([
        ("original.Consumer".into(), definition(-1)),
        ("original.Producer".into(), definition(1)),
        ("original.Holder".into(), definition(0)),
    ]));
    let mut query = Common::default();
    let consumer = |argument| Type::named("original.Consumer", vec![argument]);
    let producer = |argument| Type::named("original.Producer", vec![argument]);
    assert_eq!(
        query.join(&consumer(Type::Int), &consumer(Type::String)).unwrap(),
        Some(consumer(Type::Nothing))
    );
    assert_eq!(
        query.meet(&producer(Type::Int), &producer(Type::String)).unwrap(),
        Some(producer(Type::Nothing))
    );
    for family in ["original.Consumer", "original.Producer", "original.Holder"] {
        for left in [Type::Int, Type::String, Type::Any, Type::Star] {
            for right in [Type::Boolean, Type::Int.nullable(), Type::Nothing, Type::Star] {
                let a = Type::named(family, vec![left.clone()]);
                let b = Type::named(family, vec![right]);
                if let Some(joined) = query.join(&a, &b).unwrap() {
                    assert!(a.subtype_of(&joined) && b.subtype_of(&joined));
                }
                if let Some(meet) = query.meet(&a, &b).unwrap() {
                    assert!(meet.subtype_of(&a) && meet.subtype_of(&b));
                }
            }
        }
    }
}

#[test]
fn coverage_common_queries_keep_results_after_the_cache_is_full() {
    let mut query = Common::default();
    for index in 0..5000 {
        let value = Type::named(
            "List",
            vec![Type::named(&format!("original.Item{index}"), vec![Type::Int])],
        );
        assert_eq!(query.join(&value, &value).unwrap(), Some(value.clone()));
        assert_eq!(query.meet(&value, &value).unwrap(), Some(value));
    }
    assert_eq!(query.join(&Type::Int, &Type::String).unwrap(), None);
    assert_eq!(query.meet(&Type::Int, &Type::String).unwrap(), Some(Type::Nothing));
    let mut left = Type::Int;
    let mut right = Type::String;
    for _ in 0..35 {
        left = Type::named("List", vec![left]);
        right = Type::named("List", vec![right]);
    }
    assert!(matches!(query.join(&left, &right), Err(Failure::Budget)));
}
