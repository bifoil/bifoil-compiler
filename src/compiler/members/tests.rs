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
