//! Member lookup preserves nominal ownership, constructor identity and collection views.
use super::*;
use crate::model::{FieldDef, TypeDef};
use std::collections::BTreeMap;

fn signature(id: &str, result: Type) -> Signature {
    Arc::new(SignatureData {
        id: id.into(),
        result,
        generics: vec![],
        params: vec![].into(),
        suspend: false,
        scene: false,
    })
}

fn binding(runtime: &str, callables: Vec<Signature>) -> Binding {
    Binding {
        identity: super::super::bindings::Identity::new(),
        index_key: false,
        global_aliases: None,
        volatile: false,
        implicit_receiver: false,
        escaped: false,
        runtime: runtime.into(),
        ty: callables.first().map_or(Type::Int, |s| s.ty()),
        mutable: false,
        initialized: true,
        may_be_initialized: true,
        owner: "members-test".into(),
        narrow: None,
        field_narrows: Default::default(),
        callables,
        requires: Default::default(),
        callable_requirements: BTreeMap::new(),
    }
}

fn definition(kind: &str) -> TypeDef {
    TypeDef {
        parameters: vec![],
        field_order: vec![],
        kind: kind.into(),
        fields: BTreeMap::new(),
        methods: BTreeMap::new(),
        private_methods: vec![],
        parent: None,
        variants: vec![],
    }
}

fn member(receiver: Expr, name: &str) -> Expr {
    Expr::Member {
        receiver: Box::new(receiver),
        name: name.into(),
        safe: false,
        line: 1,
    }
}

#[test]
fn function_fields_and_methods_retain_distinct_targets_and_static_admission() {
    let _session = crate::declarations::AnalysisSession::enter();
    let receiver = Type::named("Box", vec![]);
    let method = signature("Box.run", Type::Int);
    let callback = signature("callback", Type::Unit).ty();
    let mut data = definition("data");
    data.fields.insert(
        "callback".into(),
        FieldDef {
            ty: callback.clone(),
            mutable: false,
            private: false,
        },
    );
    data.methods.insert("run".into(), vec![method.clone()]);
    crate::declarations::install(BTreeMap::from([("Box".into(), Arc::new(data))]));
    assert_eq!(member_type(&receiver, "callback").unwrap(), callback);
    assert_eq!(member_type(&receiver, "run").unwrap(), method.ty());
    assert_eq!(resolved_member_target(&receiver, "callback"), None);
    assert_eq!(resolved_member_target(&receiver, "run").as_deref(), Some("Box.run"));

    let mut env = vec![Arc::new(rpds::RedBlackTreeMapSync::new_sync())];
    super::super::insert(&mut env, "Box.run", binding("Box.run", vec![method])).unwrap();
    let method_path = member(Expr::Name("Box".into()), "run");
    assert!(static_symbol(&method_path, &env).is_none());
    assert!(
        check_static_receiver(&method_path, &env)
            .unwrap_err()
            .contains("requires a receiver")
    );
    let bound = Expr::BoundReference(Box::new(Expr::Name("Box".into())), "run".into(), 1);
    assert!(check_static_receiver(&bound, &env).is_err());
    assert!(static_symbol(&member(Expr::Int(1), "run"), &env).is_none());
}

#[test]
fn constructor_member_checks_require_the_declared_constructor_identity() {
    let _session = crate::declarations::AnalysisSession::enter();
    let mut data = definition("data");
    data.fields.insert(
        "value".into(),
        FieldDef {
            ty: Type::Int,
            mutable: false,
            private: false,
        },
    );
    crate::declarations::install(BTreeMap::from([
        ("Box".into(), Arc::new(data)),
        ("Tag".into(), Arc::new(definition("variant"))),
    ]));
    let mut env = vec![Arc::new(rpds::RedBlackTreeMapSync::new_sync())];
    for (name, signature) in [
        ("Box", signature("Box", Type::named("Box", vec![]))),
        ("factory", signature("factory", Type::named("Box", vec![]))),
        ("scalar", signature("scalar", Type::Int)),
        ("Missing", signature("Missing", Type::named("Missing", vec![]))),
        ("Tag", signature("Tag", Type::named("Tag", vec![]))),
    ] {
        // The absent declaration deliberately exercises lookup before type validation.
        Arc::make_mut(env.last_mut().unwrap()).insert_mut(name.into(), binding(name, vec![signature]));
    }
    assert!(constructor_has_member(&Expr::Name("Box".into()), "value", &env));
    for receiver in [
        Expr::Reference("Box".into()),
        Expr::Name("factory".into()),
        Expr::Name("scalar".into()),
        Expr::Name("Missing".into()),
        Expr::Name("Tag".into()),
        Expr::Name("unbound".into()),
    ] {
        assert!(!constructor_has_member(&receiver, "value", &env));
    }
    assert!(check_static_receiver(&member(Expr::Name("Box".into()), "value"), &env).is_err());
    assert!(check_static_receiver(&member(Expr::Name("factory".into()), "value"), &env).is_ok());
}

#[test]
fn enum_shorthand_requires_a_real_import_and_preserves_qualified_case_identity() {
    let _session = crate::declarations::AnalysisSession::enter();
    let mut palette = definition("enum");
    palette.variants = vec!["Palette.Red".into()];
    let mut payload = definition("variant");
    payload.fields.insert(
        "value".into(),
        FieldDef {
            ty: Type::Int,
            mutable: false,
            private: false,
        },
    );
    crate::declarations::install(BTreeMap::from([
        ("Palette".into(), Arc::new(palette)),
        ("Empty".into(), Arc::new(definition("variant"))),
        ("Payload".into(), Arc::new(payload)),
    ]));
    let expected = Type::named("Palette", vec![]);
    let red = binding("Palette.Red", vec![signature("Palette.Red", expected.clone())]);
    let mut env = vec![Arc::new(rpds::RedBlackTreeMapSync::new_sync())];
    assert!(
        enum_shorthand("Red", &env, Some(&expected))
            .err()
            .unwrap()
            .contains("not imported")
    );
    super::super::insert(&mut env, "Alias", red.clone()).unwrap();
    assert_eq!(
        enum_shorthand("Red", &env, Some(&expected)).unwrap().unwrap().runtime,
        "Palette.Red"
    );
    assert!(constant_case(&red));
    assert!(constant_case(&binding(
        "Empty",
        vec![signature("Empty", Type::named("Empty", vec![]))]
    )));
    assert!(!constant_case(&binding(
        "other",
        vec![signature("other", Type::named("Empty", vec![]))]
    )));
    assert!(!constant_case(&binding(
        "Red",
        vec![signature("Red", expected.clone())]
    )));
    assert!(!constant_case(&binding("value", vec![signature("value", Type::Int)])));
    let payload_type = Type::named("Payload", vec![]);
    assert!(!constant_case(&binding(
        "factory",
        vec![signature("factory", payload_type.clone())]
    )));
    let mut constructor = signature("Payload", payload_type);
    Arc::make_mut(&mut constructor).params = vec![Param {
        name: "value".into(),
        ty: Type::Int,
        mutable: false,
        default: false,
        vararg: false,
    }]
    .into();
    assert!(!constant_case(&binding("Payload", vec![constructor])));
    assert!(constant_case_type(&binding("value", vec![]), None).is_err());
    for ty in [Type::named("Missing", vec![]), Type::Int] {
        assert!(enum_shorthand("Red", &env, Some(&ty)).unwrap().is_none());
    }
    super::super::insert(&mut env, "Red", binding("local", vec![])).unwrap();
    assert!(
        enum_shorthand("Red", &env, Some(&expected))
            .err()
            .unwrap()
            .contains("conflicts")
    );
}

#[test]
fn coverage_builtin_collection_methods_preserve_family_mutability_and_result_shapes() {
    for (family, args) in [
        ("List", vec![Type::Int]),
        ("Set", vec![Type::String]),
        ("Map", vec![Type::String, Type::Int.nullable()]),
    ] {
        for mutable in [false, true] {
            let name = format!("{}{family}", if mutable { "Mutable" } else { "" });
            let receiver = Type::named(&name, args.clone());
            for method in [
                "toList",
                "toMutableList",
                "toSet",
                "toMutableSet",
                "toMap",
                "toMutableMap",
            ] {
                let signature = builtin_signature(&receiver, method);
                assert_eq!(signature.is_some(), method.ends_with(family), "{name}.{method}");
                if let Some(signature) = signature {
                    assert!(signature.params.is_empty());
                    assert_eq!(
                        signature.result,
                        Type::named(method.strip_prefix("to").unwrap(), args.clone())
                    );
                }
            }
            for method in ["clear", "remove", "add", "insert", "removeAt", "put"] {
                let allowed = mutable
                    && match method {
                        "clear" | "remove" => true,
                        "add" => family != "Map",
                        "insert" | "removeAt" => family == "List",
                        "put" => family == "Map",
                        _ => false,
                    };
                let signature = builtin_signature(&receiver, method);
                assert_eq!(signature.is_some(), allowed, "{name}.{method}");
                if let Some(signature) = signature {
                    assert!(!signature.suspend && !signature.scene);
                    assert!(signature.params.iter().all(|p| !p.mutable && !p.vararg && !p.default));
                }
            }
            let contains = builtin_signature(&receiver, "contains").unwrap();
            assert_eq!(contains.params[0].ty, args[0]);
            assert_eq!(contains.result, Type::Boolean);
            assert_eq!(builtin_signature(&receiver, "containsKey").is_some(), family == "Map");
            assert_eq!(builtin_signature(&receiver, "isEmpty").unwrap().result, Type::Boolean);
            assert!(builtin_signature(&receiver, "missing").is_none());
        }
    }
    let callbacks = Type::named(
        "MutableList",
        vec![Type::Function {
            params: vec![].into(),
            result: Arc::new(Type::Unit),
            suspend: false,
            vararg: false,
        }],
    );
    assert!(builtin_signature(&callbacks, "contains").is_none());
    assert!(builtin_signature(&callbacks, "remove").is_none());
    assert!(builtin_signature(&callbacks, "add").is_some());
}

#[test]
fn coverage_builtin_primitive_methods_keep_numeric_and_text_interfaces_distinct() {
    for receiver in [
        Type::Int,
        Type::Float,
        Type::Char,
        Type::String,
        Type::Boolean,
        Type::Unit,
    ] {
        assert_eq!(builtin_signature(&receiver, "toString").unwrap().result, Type::String);
        assert_eq!(
            builtin_signature(&receiver, "toInt").is_some(),
            matches!(receiver, Type::Int | Type::Float)
        );
        assert_eq!(
            builtin_signature(&receiver, "toFloat").is_some(),
            matches!(receiver, Type::Int | Type::Float)
        );
        assert_eq!(
            builtin_signature(&receiver, "substring").is_some(),
            receiver == Type::String
        );
        assert_eq!(
            builtin_signature(&receiver, "isEmpty").is_some(),
            receiver == Type::String
        );
        assert!(builtin_signature(&receiver, "contains").is_none());
    }
    let substring = builtin_signature(&Type::String, "substring").unwrap();
    assert_eq!(
        substring.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["start", "endExclusive"]
    );
    assert!(substring.params.iter().all(|p| p.ty == Type::Int));
}

#[test]
fn projected_index_reads_preserve_keys_and_expose_unknown_elements_as_nullable_any() {
    let _session = crate::declarations::AnalysisSession::enter();
    for (receiver, expected) in [
        (Type::String, (Type::Int, Type::Char)),
        (Type::named("List", vec![Type::Int]), (Type::Int, Type::Int)),
        (Type::named("List", vec![Type::Star]), (Type::Int, Type::Any.nullable())),
        (
            Type::named("Map", vec![Type::String, Type::Int]),
            (Type::String, Type::Int.nullable()),
        ),
        (
            Type::named("Map", vec![Type::String, Type::Star]),
            (Type::String, Type::Any.nullable()),
        ),
    ] {
        assert_eq!(index_types(&receiver).unwrap(), expected);
    }
    assert!(
        index_types(&Type::named("Set", vec![Type::Int]))
            .unwrap_err()
            .contains("Set cannot be indexed")
    );
    assert!(index_types(&Type::Int).unwrap_err().contains("cannot be indexed"));
}

#[test]
fn scalar_collection_and_progression_properties_do_not_invent_unrelated_members() {
    let _session = crate::declarations::AnalysisSession::enter();
    for (receiver, name) in [
        (Type::String, "length"),
        (Type::Char, "code"),
        (Type::named("List", vec![Type::Int]), "size"),
        (Type::named("IntRange", vec![]), "first"),
        (Type::named("IntProgression", vec![]), "last"),
        (Type::named("IntProgression", vec![]), "step"),
    ] {
        assert_eq!(member_type(&receiver, name).unwrap(), Type::Int);
    }
    for (receiver, name) in [
        (Type::Int, "length"),
        (Type::String, "code"),
        (Type::named("IntRange", vec![]), "unrelated"),
        (Type::named("Missing", vec![]), "first"),
    ] {
        assert!(
            member_type(&receiver, name)
                .unwrap_err()
                .contains("has no accessible member")
        );
    }
}

#[test]
fn expression_paths_require_a_complete_ordinary_member_chain() {
    let named = Expr::Name("Owner".into());
    let path = Expr::Member {
        receiver: Box::new(named.clone()),
        name: "child".into(),
        safe: false,
        line: 1,
    };
    assert_eq!(expression_path(&path).as_deref(), Some("Owner.child"));
    let safe = Expr::Member {
        receiver: Box::new(named),
        name: "child".into(),
        safe: true,
        line: 1,
    };
    assert_eq!(expression_path(&safe), None);
    let dynamic = Expr::Member {
        receiver: Box::new(Expr::Int(1)),
        name: "child".into(),
        safe: false,
        line: 1,
    };
    assert_eq!(expression_path(&dynamic), None);
    assert_eq!(
        expression_path(&Expr::Reference("Owner".into())).as_deref(),
        Some("Owner")
    );
    assert!(static_symbol(&Expr::Name("Owner".into()), &vec![]).is_none());
    let bound = Expr::BoundReference(Box::new(Expr::Int(1)), "method".into(), 1);
    assert!(check_static_receiver(&bound, &vec![]).is_ok());
}
