//! Callable and scene aliases share their children without losing type semantics.

use super::compile;
use crate::model::{GenericParam, Type};
use std::collections::BTreeMap;
use std::sync::Arc;

fn callable(parameter: Type, result: Type) -> Type {
    Type::Function {
        params: vec![parameter].into(),
        result: Arc::new(result),
        suspend: false,
        vararg: false,
    }
}
fn repeated(levels: usize, leaf: Type, scene: bool) -> Type {
    let mut ty = leaf;
    for _ in 0..levels {
        ty = if scene {
            Type::Scene {
                params: vec![ty.clone(), ty].into(),
            }
        } else {
            callable(ty.clone(), ty)
        };
    }
    ty
}

#[test]
fn unused_callable_aliases_stay_compact_and_obey_depth_limits() {
    for scene in [false, true] {
        for levels in [28, 31, 34] {
            let mut source = String::from(if scene {
                "typealias A0 = Scene\n"
            } else {
                "typealias A0 = (Int) -> Int\n"
            });
            for level in 1..=levels {
                let previous = level - 1;
                source += &if scene {
                    format!("typealias A{level} = Scene<A{previous}, A{previous}>\n")
                } else {
                    format!("typealias A{level} = (A{previous}) -> A{previous}\n")
                };
            }
            source += "scene start():\n    Unit\n";
            if levels > 32 {
                assert!(compile(&source).unwrap_err().contains("type graph budget exceeded"));
            } else {
                assert_eq!(compile(&source).unwrap().functions.len(), 1);
            }
        }
    }
}

#[test]
fn callable_graph_queries_keep_child_kind_result_and_flags_distinct() {
    let params: Arc<[Type]> = vec![Type::Int].into();
    let function = Type::Function {
        params: Arc::clone(&params),
        result: Arc::new(Type::Star),
        suspend: false,
        vararg: false,
    };
    // The scene is visited first. Its identical parameter allocation must not
    // hide the function's result from subsequent shape and validity checks.
    let root = Type::named("Entry", vec![function.clone(), Type::Scene { params }]);
    assert!(root.any_node(|ty| matches!(ty, Type::Star)));
    let _session = crate::declarations::AnalysisSession::enter();
    assert!(
        crate::declarations::validate_type(&root)
            .unwrap_err()
            .contains("star projection")
    );
    let ordinary = callable(Type::Int, Type::Int);
    let Type::Function { params, result, .. } = &ordinary else {
        unreachable!()
    };
    let suspended = Type::Function {
        params: Arc::clone(params),
        result: Arc::clone(result),
        suspend: true,
        vararg: false,
    };
    let variadic = Type::Function {
        params: Arc::clone(params),
        result: Arc::clone(result),
        suspend: false,
        vararg: true,
    };
    assert!(ordinary != suspended && ordinary != variadic);
    assert!(ordinary.subtype_of(&suspended));
    assert!(!suspended.subtype_of(&ordinary));
    assert!(!ordinary.subtype_of(&variadic));
}

#[test]
fn callable_graph_equality_and_depth_do_not_expand_repeated_children() {
    for scene in [false, true] {
        let first = repeated(28, Type::Int, scene);
        let second = repeated(28, Type::Int, scene);
        assert_eq!(first, second);
        assert!(first != repeated(28, Type::Float, scene));
        assert!(first.within_depth(28));
        assert!(!first.within_depth(27));
        let shallow = repeated(2, Type::Int, scene);
        let deep = repeated(30, shallow.clone(), scene);
        let root = Type::named("Entry", vec![deep, shallow]);
        assert!(!root.within_depth(32));
        assert!(root.within_depth(33));
    }
}

#[test]
fn callable_substitution_and_inference_reuse_completed_constraints() {
    let parameter = GenericParam {
        name: "T".into(),
        variance: 0,
        bounds: vec![],
    };
    for scene in [false, true] {
        let pattern = repeated(28, Type::parameter("T"), scene);
        let actual = repeated(28, Type::Int, scene);
        let mut arguments = BTreeMap::new();
        crate::generics::infer(&pattern, &actual, std::slice::from_ref(&parameter), &mut arguments).unwrap();
        assert_eq!(arguments["T"], Type::Int);
        assert_eq!(crate::generics::substitute(&pattern, &arguments), actual);
    }
}
