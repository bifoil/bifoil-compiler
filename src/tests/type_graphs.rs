//! Shared type graphs retain semantic checks without repeated tree expansion.

use super::compile;
use crate::model::Type;
use std::sync::Arc;

fn aliases(levels: usize, generic: bool) -> String {
    let parameter = if generic { "<T>" } else { "" };
    let mut source = format!("typealias A0{parameter} = Int\n");
    for level in 1..=levels {
        source += &format!(
            "typealias A{level}{parameter} = Map<A{}{parameter}, A{}{parameter}>\n",
            level - 1,
            level - 1,
        );
    }
    source
}

#[test]
fn reused_aliases_and_unused_generic_arguments_preserve_compact_graphs() {
    for generic in [false, true] {
        let source = aliases(28, generic) + "scene start():\n    Unit\n";
        let module = compile(&source).unwrap();
        assert_eq!(module.functions.len(), 1);
        assert!(module.types.is_empty());
    }
    let source = aliases(34, false) + "scene start():\n    Unit\n";
    assert!(compile(&source).unwrap_err().contains("type graph budget exceeded"));
}

#[test]
fn shared_depth_queries_revisit_nodes_on_longer_paths() {
    let shared = Type::named("List", vec![Type::named("List", vec![Type::Int])]);
    let mut deep = shared.clone();
    for _ in 0..30 {
        deep = Type::named("List", vec![deep]);
    }
    // The LIFO traversal visits the short right-hand path first.
    let root = Type::named("Map", vec![deep, shared]);
    assert!(!root.within_depth(32));
    assert!(root.within_depth(33));
}

#[test]
fn shared_arguments_do_not_confuse_list_and_set_validation() {
    let _session = crate::declarations::AnalysisSession::enter();
    let arguments: Arc<[Type]> = vec![Type::Function {
        params: vec![].into(),
        result: std::sync::Arc::new(Type::Int),
        suspend: false,
        vararg: false,
    }]
    .into();
    let signature = Type::Function {
        params: vec![
            Type::Named {
                name: "List".into(),
                args: Arc::clone(&arguments),
            },
            Type::Named {
                name: "Set".into(),
                args: arguments,
            },
        ]
        .into(),
        result: std::sync::Arc::new(Type::Unit),
        suspend: false,
        vararg: false,
    };
    assert!(
        crate::declarations::validate_type(&signature)
            .unwrap_err()
            .contains("StableKey")
    );
}

#[test]
fn large_plain_alias_scopes_share_their_visible_names() {
    let mut source = String::from("typealias A0 = Int\n");
    for level in 1..=2048 {
        source += &format!("typealias A{level} = A{}\n", level - 1);
    }
    source += "fun value(input: A2048) -> A2048 = input\n";
    let module = compile(&source).unwrap();
    assert_eq!(module.functions["test.value"].signature.result, Type::Int);
}

#[test]
fn nullable_alias_application_flattens_before_export_and_signature_use() {
    let module = compile(
        r#"typealias Optional = String?
export typealias Again = Optional?
export fun echo(value: Again) -> Again = value
"#,
    )
    .unwrap();
    let signature = &module.functions["test.echo"].signature;
    assert_eq!(signature.params[0].ty, Type::String.nullable());
    assert_eq!(signature.result, Type::String.nullable());
    let crate::model::Export::Alias { ty, .. } = &module.exports["Again"] else {
        panic!("missing alias")
    };
    assert_eq!(*ty, Type::String.nullable());
}

fn repeated_graph(levels: usize, leaf: Type) -> Type {
    let mut ty = leaf;
    for _ in 0..levels {
        ty = Type::named("Map", vec![ty.clone(), ty]);
    }
    ty
}

#[test]
fn independent_shared_graphs_compare_by_structure() {
    let first = repeated_graph(28, Type::Int);
    let second = repeated_graph(28, Type::Int);
    let different = repeated_graph(28, Type::Float);
    assert!(first == second);
    assert!(first != different);
    assert!(first == first.clone());
}

#[test]
fn structural_type_equality_preserves_serialized_identity_and_callable_shape() {
    let mut types = vec![
        Type::Int,
        Type::Float,
        Type::Boolean,
        Type::String,
        Type::Char,
        Type::Unit,
        Type::Nothing,
        Type::Any,
        Type::Null,
        Type::Star,
        Type::parameter("outer#T"),
        Type::parameter("other#T"),
        Type::Parameter {
            name: "outer#T".into(),
            nonnull: true,
        },
        Type::Int.nullable(),
        Type::Int.nullable().nullable(),
        Type::named("List", vec![Type::Int]),
        Type::named("Set", vec![Type::Int]),
        Type::named("List", vec![Type::Float]),
        Type::named("Empty", vec![]),
        Type::Scene { params: vec![].into() },
        Type::Scene {
            params: vec![Type::Int].into(),
        },
    ];
    for suspend in [false, true] {
        for vararg in [false, true] {
            for params in [vec![], vec![Type::Int], vec![Type::Float]] {
                for result in [Type::Int, Type::Unit] {
                    types.push(Type::Function {
                        params: params.clone().into(),
                        result: std::sync::Arc::new(result),
                        suspend,
                        vararg,
                    });
                }
            }
        }
    }
    let encoded = types
        .iter()
        .map(|ty| serde_json::to_value(ty).unwrap())
        .collect::<Vec<_>>();
    for (left, left_type) in types.iter().enumerate() {
        for (right, right_type) in types.iter().enumerate() {
            assert_eq!(
                left_type == right_type,
                encoded[left] == encoded[right],
                "type pair {left}/{right}"
            );
        }
        assert!(*left_type != Type::Pending);
    }
    assert!(Type::Pending == Type::Pending);
}

fn bounded_alias_application(levels: usize, leaf: &str) -> String {
    let mut source = aliases(levels, false);
    source += &format!("typealias B0 = {leaf}\n");
    for level in 1..=levels {
        source += &format!("typealias B{level} = Map<B{}, B{}>\n", level - 1, level - 1);
    }
    source
        + &format!(
            "typealias Check<T: A{levels}> = Unit\ntypealias Checked = Check<B{levels}>\nscene start():\n    Unit\n"
        )
}

#[test]
fn large_equivalent_alias_bounds_accept_without_tree_expansion() {
    let module = compile(&bounded_alias_application(28, "Int")).unwrap();
    assert_eq!(module.functions.len(), 1);
}

#[test]
fn large_invalid_alias_bounds_have_bounded_source_diagnostics() {
    let message = compile(&bounded_alias_application(24, "Float")).unwrap_err();
    assert!(message.contains("does not satisfy"));
    assert!(message.contains('…'));
    assert!(message.len() < 9000);
}

#[test]
fn bounded_type_names_keep_unicode_boundaries_and_limit_nested_wrappers() {
    let ty = Type::named(&format!("{}😀tail", "a".repeat(4095)), vec![]);
    let name = ty.to_string();
    assert_eq!(name, format!("{}…", "a".repeat(4095)));
    let mut nested = Type::Int;
    for _ in 0..100 {
        nested = Type::Nullable {
            inner: Box::new(nested),
        };
    }
    assert_eq!(nested.to_string(), "…");
    let graph = repeated_graph(28, Type::Int).to_string();
    assert!(graph.ends_with('…'));
    assert!(graph.len() <= 4099);
}

#[test]
fn covariant_shared_graphs_reuse_subtype_checks_without_changing_direction() {
    fn graph(leaf: Type) -> Type {
        let mut ty = leaf;
        for _ in 0..28 {
            ty = Type::named("Entry", vec![ty.clone(), ty]);
        }
        ty
    }
    let narrow = graph(Type::Int);
    let broad = graph(Type::Any);
    assert!(narrow.subtype_of(&broad));
    assert!(!broad.subtype_of(&narrow));
    assert!(narrow.nullable().subtype_of(&broad.nullable()));
    assert!(!narrow.nullable().subtype_of(&broad));
}

#[test]
fn used_generic_alias_parameters_share_only_matching_instantiations() {
    let mut source = String::from("typealias A0<T> = T\n");
    for level in 1..=28 {
        source += &format!("typealias A{level}<T> = Entry<A{}<T>, A{}<T>>\n", level - 1, level - 1);
    }
    source += "typealias Final = A28<Int>\nscene start():\n    Unit\n";
    assert_eq!(compile(&source).unwrap().functions.len(), 1);
    let invalid = source.replace("Entry<", "Map<");
    assert!(compile(&invalid).unwrap_err().contains("StableKey"));
    let module = compile(
        "typealias Pair<T> = Map<String, T>\n\
         fun integer(value: Pair<Int>) -> Int? = value[\"x\"]\n\
         fun floating(value: Pair<Float>) -> Float? = value[\"x\"]\n",
    )
    .unwrap();
    assert_eq!(module.functions["test.integer"].signature.result, Type::Int.nullable());
    assert_eq!(
        module.functions["test.floating"].signature.result,
        Type::Float.nullable()
    );
}

#[test]
fn pure_shape_queries_preserve_hidden_parameters_and_projections() {
    let repeated = repeated_graph(28, Type::Int);
    assert!(!repeated.any_node(|ty| matches!(ty, Type::Star | Type::Parameter { .. })));
    for hidden in [Type::Star, Type::parameter("hidden#T")] {
        let graph = Type::named("Entry", vec![hidden, repeated.clone()]);
        assert!(graph.any_node(|ty| matches!(ty, Type::Star | Type::Parameter { .. })));
    }
}

#[test]
fn alias_bounds_resolve_later_plain_and_generic_aliases_in_the_same_scope() {
    for declarations in [
        "typealias Checked<T: Name> = T\ntypealias Name = String\n",
        "typealias Name = String\ntypealias Checked<T: Name> = T\n",
    ] {
        let module =
            compile(&(declarations.to_owned() + "fun echo(value: Checked<String>) -> String = value\n")).unwrap();
        assert_eq!(module.functions["test.echo"].signature.params[0].ty, Type::String);
        let error =
            compile(&(declarations.to_owned() + "fun wrong(value: Checked<Int>) -> Int = value\n")).unwrap_err();
        assert!(error.contains("does not satisfy String"), "{error}");
        assert!(!error.contains("$alias:"), "{error}");
    }
    compile(
        "typealias Checked<T: Later<String>> = T\n\
         typealias Later<U> = List<U>\n\
         fun echo(value: Checked<List<String>>) -> List<String> = value\n",
    )
    .unwrap();
    compile(
        r#"scene start():
    typealias Checked<T: Name> = T
    typealias Name = String
    val text: Checked<String> = "Ada"
    prose:
        ${text}
"#,
    )
    .unwrap();
}

#[test]
fn alias_bound_expansion_cycles_and_invalid_forward_arity_remain_errors() {
    for source in [
        "typealias A<T: B<T>> = T\ntypealias B<T: A<T>> = T\n",
        "typealias A<T: B> = T\ntypealias B = A<String>\n",
        "typealias A = B\ntypealias B = A\n",
    ] {
        assert!(compile(source).unwrap_err().contains("type alias expansion cycle"));
    }
    let error = compile("typealias A<T: B<Int>> = T\ntypealias B = String\n").unwrap_err();
    assert!(error.contains("type alias argument count mismatch"), "{error}");
}

#[test]
fn repeated_inference_constraints_preserve_cross_argument_joins_and_failures() {
    use crate::model::GenericParam;
    use std::collections::BTreeMap;
    let parameter = GenericParam {
        name: "call#T".into(),
        variance: 0,
        bounds: vec![],
    };
    let pattern = repeated_graph(28, Type::parameter(&parameter.name));
    let mut arguments = BTreeMap::new();
    crate::generics::infer(
        &pattern,
        &repeated_graph(28, Type::Int),
        std::slice::from_ref(&parameter),
        &mut arguments,
    )
    .unwrap();
    assert_eq!(arguments[&parameter.name], Type::Int);
    crate::generics::infer(
        &pattern,
        &repeated_graph(28, Type::Float),
        std::slice::from_ref(&parameter),
        &mut arguments,
    )
    .unwrap();
    assert_eq!(arguments[&parameter.name], Type::Float);
    let error = crate::generics::infer(
        &pattern,
        &repeated_graph(28, Type::String),
        &[parameter],
        &mut arguments,
    )
    .unwrap_err();
    assert!(error.contains("incompatible constraints"));
}

#[test]
fn inference_pair_keys_retain_distinct_parameter_bindings() {
    use crate::model::GenericParam;
    use std::collections::BTreeMap;
    let parameters = ["first#T", "second#U"].map(|name| GenericParam {
        name: name.into(),
        variance: 0,
        bounds: vec![],
    });
    let pattern = Type::named(
        "Entry",
        parameters
            .iter()
            .map(|p| repeated_graph(26, Type::parameter(&p.name)))
            .collect(),
    );
    let actual = Type::named(
        "Entry",
        vec![repeated_graph(26, Type::Int), repeated_graph(26, Type::Float)],
    );
    let mut arguments = BTreeMap::new();
    crate::generics::infer(&pattern, &actual, &parameters, &mut arguments).unwrap();
    assert_eq!(arguments[&parameters[0].name], Type::Int);
    assert_eq!(arguments[&parameters[1].name], Type::Float);
}

#[test]
fn large_generic_alias_scopes_share_all_nonparameter_names() {
    let mut source = String::from("typealias A0<T> = T\n");
    for level in 1..=2048 {
        source += &format!("typealias A{level}<T> = A{}<T>\n", level - 1);
    }
    source += "fun echo(value: A2048<String>) -> String = value\n";
    let module = compile(&source).unwrap();
    assert_eq!(module.functions["test.echo"].signature.params[0].ty, Type::String);
}

#[test]
fn alias_parameter_overlays_preserve_outer_types_and_shadowing_rules() {
    compile(
        r#"fun <T> outer(value: T) -> T:
    typealias Local<U> = U
    val held: Local<T> = value
    return held
"#,
    )
    .unwrap();
    assert!(
        compile("typealias Local<T, T> = T\n")
            .unwrap_err()
            .contains("duplicate type parameter")
    );
    for source in [
        "typealias Visible = Int\ntypealias Local<Visible> = Visible\n",
        "fun <T> outer(value: T) -> T:\n    typealias Local<T> = T\n    return value\n",
    ] {
        let error = compile(source).unwrap_err();
        assert!(
            error.contains("generic parameter shadows a visible type name"),
            "{source}: {error}"
        );
    }
}

#[test]
fn narrowed_results_keep_closed_shared_graphs_compact() {
    let source = |levels| {
        let mut source = String::from(
            "data class Pair<out L, out R>:\n    val left: L\n    val right: R\n\
             typealias A0 = Any\ntypealias B0 = Int\n",
        );
        for level in 1..=levels {
            source += &format!(
                "typealias A{level} = Pair<A{}, A{}>\ntypealias B{level} = Pair<B{}, B{}>\n",
                level - 1,
                level - 1,
                level - 1,
                level - 1,
            );
        }
        source + &format!("fun view(input: A{levels}) = if input is B{levels}:\n    input\nelse:\n    input\n")
    };
    let module = compile(&source(5)).unwrap();
    assert!(module.functions["test.view"].signature.result.within_depth(6));
    // The expanded interface still exceeds the existing publication budget;
    // analysis must reach that diagnostic without unfolding every query first.
    assert!(
        compile(&source(28))
            .unwrap_err()
            .contains("canonical data size budget exceeded")
    );
}

#[test]
fn repeated_invariant_cast_arguments_retain_possible_parameter_identity() {
    let graph = |mut leaf: Type| {
        for _ in 0..28 {
            leaf = Type::named("Entry", vec![leaf.clone(), leaf]);
        }
        Type::named("MutableList", vec![leaf])
    };
    assert!(graph(Type::parameter("unknown#T")).may_overlap(&graph(Type::parameter("unknown#U"))));
    assert!(!graph(Type::Int).may_overlap(&graph(Type::Float)));
    assert!(Type::named("List", vec![Type::Int]).may_overlap(&Type::named("List", vec![Type::Float])));
    assert!(
        !Type::named("MutableList", vec![Type::named("List", vec![Type::Int])]).may_overlap(&Type::named(
            "MutableList",
            vec![Type::named("List", vec![Type::Float])]
        ))
    );
}

#[test]
fn deep_cast_fallbacks_do_not_replace_a_shallower_disjoint_proof() {
    let graph = |leaf| {
        let shared = Type::named("Entry", vec![leaf, Type::Int]);
        let mut deep = shared.clone();
        for _ in 0..61 {
            deep = Type::named("List", vec![deep]);
        }
        Type::named("MutableList", vec![Type::named("Entry", vec![deep, shared])])
    };
    assert!(!graph(Type::Int).may_overlap(&graph(Type::Float)));
}

#[test]
fn large_generic_cast_graphs_reach_the_publication_budget() {
    let mut source = String::from(
        "data class Pair<out L, out R>:\n    val left: L\n    val right: R\n\
         data class Box<T>:\n    val value: T\ntypealias A0<T> = T\n",
    );
    for level in 1..=28 {
        source += &format!("typealias A{level}<T> = Pair<A{}<T>, A{}<T>>\n", level - 1, level - 1);
    }
    source += "fun <T, U> check(input: Box<A28<T>>) -> Box<A28<U>>? = input as? Box<A28<U>>\n";
    assert!(
        compile(&source)
            .unwrap_err()
            .contains("canonical data size budget exceeded")
    );
}

#[test]
fn projected_shared_nominal_reads_reach_publication_checks_without_expansion() {
    for levels in [5, 20] {
        let mut source = String::from(
            "data class Pair<out Left, out Right>:\n    val left: Left\n    val right: Right\ntypealias A0<T> = T\n",
        );
        for level in 1..=levels {
            let previous = level - 1;
            source += &format!("typealias A{level}<T> = Pair<A{previous}<T>, A{previous}<T>>\n");
        }
        source += &format!(
            "data class Box<T>:\n    val value: A{levels}<T>\nfun read(input: Box<*>) = input.value\nscene start():\n    Unit\n"
        );
        if levels == 5 {
            compile(&source).unwrap();
        } else {
            assert!(
                compile(&source)
                    .unwrap_err()
                    .contains("canonical data size budget exceeded")
            );
        }
    }
}

#[test]
fn forward_alias_bounds_and_function_bodies_keep_nominal_recursion() {
    let module = compile(
        r#"
export typealias Mapper<T: Limit> = (T) -> Product<T>
typealias Product<T: Limit> = Box<T>
typealias Limit = Any
typealias Link = Node?
data class Node:
    var next: Link
data class Box<T>:
    val value: T
fun <T: Any> mapper() -> Mapper<T>:
    return (value: T) => Box(value)
scene start():
    val wrap = mapper<Int>()
    val node = Node(null)
    node.next = node
    prose:
        ${wrap(7).value}|${node.next == node}
"#,
    )
    .unwrap();
    assert_eq!(
        module.types["test.Node"].fields["next"].ty,
        Type::named("test.Node", vec![]).nullable()
    );
    let crate::model::Export::Alias {
        ty: Type::Function { result, .. },
        ..
    } = &module.exports["Mapper"]
    else {
        panic!("missing expanded generic function alias");
    };
    assert!(matches!(result.as_ref(), Type::Named { name, args } if name == "test.Box" && args.len() == 1));
}
