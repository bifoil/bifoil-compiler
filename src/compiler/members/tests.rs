//! Built-in method admission exposes mutators only on the appropriate family.
use super::*;

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
