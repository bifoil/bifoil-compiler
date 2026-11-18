//! Constant proofs must agree with checked runtime arithmetic and scalar text.
use super::*;
use crate::model::{Param, Signature, SignatureData};
use std::sync::Arc;

fn binary(operator: &str, left: Expr, right: Expr) -> Expr {
    Expr::Binary(operator.into(), Box::new(left), Box::new(right), 1)
}

#[test]
fn coverage_constants_preserve_boolean_short_circuit_proofs() {
    for left in [false, true] {
        for right in [false, true] {
            assert_eq!(
                constant_boolean(&binary("&&", Expr::Boolean(left), Expr::Boolean(right))),
                Some(left && right)
            );
            assert_eq!(
                constant_boolean(&binary("||", Expr::Boolean(left), Expr::Boolean(right))),
                Some(left || right)
            );
        }
    }
    assert_eq!(
        constant_boolean(&Expr::Group(Box::new(Expr::Boolean(true)))),
        Some(true)
    );
    assert_eq!(
        constant_boolean(&Expr::Unary("!".into(), Box::new(Expr::Boolean(false)), 1)),
        Some(true)
    );
    for (operator, left, expected) in [
        ("&&", false, Some(false)),
        ("&&", true, None),
        ("||", true, Some(true)),
        ("||", false, None),
    ] {
        assert_eq!(
            constant_boolean(&binary(operator, Expr::Boolean(left), Expr::Name("unknown".into()))),
            expected
        );
    }
    assert_eq!(constant_boolean(&Expr::Name("unknown".into())), None);
}

#[test]
fn coverage_constants_keep_exact_scalar_identity_and_unicode_order() {
    for (left, right, ordinary, exact) in [
        (Expr::Int(3), Expr::Float(3.0), true, false),
        (Expr::String("a".into()), Expr::Char("a".into()), true, false),
        (Expr::Boolean(true), Expr::Boolean(true), true, true),
        (Expr::Boolean(false), Expr::Boolean(true), false, false),
        (Expr::Null, Expr::Null, true, true),
        (Expr::Name("Unit".into()), Expr::Name("Unit".into()), true, true),
        (Expr::Null, Expr::Int(0), false, false),
    ] {
        for (operator, expected) in [("==", ordinary), ("!=", !ordinary), ("===", exact), ("!==", !exact)] {
            assert_eq!(constant_comparison(operator, &left, &right), Some(expected));
        }
    }
    let high = Expr::String("😀".into());
    let low = Expr::Group(Box::new(Expr::String("\u{e000}".into())));
    for (operator, expected) in [("<", false), ("<=", false), (">", true), (">=", true)] {
        assert_eq!(constant_comparison(operator, &high, &low), Some(expected));
    }
    assert_eq!(constant_comparison("unknown", &high, &low), None);
    assert_eq!(constant_comparison("==", &Expr::Name("unknown".into()), &low), None);
    assert_eq!(constant_comparison("==", &high, &Expr::Name("unknown".into())), None);
}

#[test]
fn coverage_constants_retain_numeric_checks_and_pinned_fairmath_endpoints() {
    for (operator, a, b, expected) in [
        ("+", 3, 2, 5.0),
        ("-", 3, 2, 1.0),
        ("*", 3, 2, 6.0),
        ("/", -3, 2, -1.0),
        ("%", -5, 3, -2.0),
    ] {
        assert_eq!(
            constant_number(&binary(operator, Expr::Int(a), Expr::Int(b))).unwrap(),
            Some((Type::Int, expected))
        );
    }
    assert_eq!(
        constant_number(&binary("/", Expr::Float(-3.0), Expr::Int(2))).unwrap(),
        Some((Type::Float, -1.5))
    );
    assert_eq!(
        constant_number(&Expr::Unary("-".into(), Box::new(Expr::Int(2147483648)), 1)).unwrap(),
        Some((Type::Int, -2147483648.0))
    );
    for (base, operator, change, expected) in [
        (0.0, "%+", 0.0, 1.0),
        (100.0, "%+", 0.0, 100.0),
        (100.0, "%+", 1.0, 99.0),
        (0.0, "%-", 1.0, 1.0),
        (50.0, "%+", -20.0, 40.0),
        (50.0, "%-", -20.0, 60.0),
        (50.0, "%+", 500.0, 99.0),
        (50.0, "%-", 500.0, 1.0),
    ] {
        assert_eq!(
            constant_number(&binary(operator, Expr::Float(base), Expr::Float(change))).unwrap(),
            Some((Type::Int, expected))
        );
    }
    for expression in [
        binary("/", Expr::Int(1), Expr::Int(0)),
        binary("%", Expr::Int(1), Expr::Int(0)),
        binary("+", Expr::Int(2147483647), Expr::Int(1)),
        binary("-", Expr::Int(-2147483648), Expr::Int(1)),
        binary("*", Expr::Float(f64::MAX), Expr::Float(2.0)),
        binary("%+", Expr::Int(-1), Expr::Name("unknown".into())),
        Expr::Float(f64::NAN),
    ] {
        assert!(constant_number(&expression).is_err());
    }
    assert_eq!(
        constant_number(&binary("+", Expr::Int(1), Expr::Name("unknown".into()))).unwrap(),
        None
    );
    assert_eq!(
        constant_number(&binary("==", Expr::Int(1), Expr::Int(1))).unwrap(),
        None
    );
}

fn signature(id: &str, names: &[&str]) -> Signature {
    Arc::new(SignatureData {
        generics: vec![],
        id: id.into(),
        params: names
            .iter()
            .map(|name| Param {
                name: (*name).into(),
                ty: Type::Int,
                mutable: false,
                default: false,
                vararg: false,
            })
            .collect::<Vec<_>>()
            .into(),
        result: Type::String,
        suspend: false,
        scene: false,
    })
}
fn member(receiver: Expr) -> Expr {
    Expr::Member {
        receiver: Box::new(receiver),
        name: "method".into(),
        safe: false,
        line: 1,
    }
}

#[test]
fn coverage_constants_check_scalar_indexes_and_named_builtin_bounds() {
    let string = Expr::Group(Box::new(Expr::String("😀a".into())));
    assert!(check_index(&string, &Expr::Int(1), &Type::String).is_ok());
    for index in [-1, 2] {
        assert!(check_index(&string, &Expr::Int(index), &Type::String).is_err());
    }
    assert!(check_index(&Expr::Name("unknown".into()), &Expr::Int(99), &Type::String).is_ok());
    assert!(check_index(&string, &Expr::Name("unknown".into()), &Type::String).is_ok());
    let map = Type::named("Map", vec![Type::Int, Type::String]);
    assert!(check_index(&Expr::Name("mapping".into()), &Expr::Int(-1), &map).is_ok());
    let substring = signature("$builtin.substring", &["start", "endExclusive"]);
    let callee = member(string.clone());
    let named = vec![
        (Some("endExclusive".into()), Expr::Int(2)),
        (Some("start".into()), Expr::Int(1)),
    ];
    assert!(check_call_bounds(&substring, &callee, &named).is_ok());
    for (a, b) in [(-1, 1), (0, 3), (2, 1)] {
        assert!(check_call_bounds(&substring, &callee, &[(None, Expr::Int(a)), (None, Expr::Int(b))]).is_err());
    }
    assert!(check_call_bounds(&signature("$builtin.substring", &[]), &callee, &[]).is_err());
    assert!(check_call_bounds(&signature("ordinary", &[]), &callee, &[]).is_ok());
    assert!(check_call_bounds(&substring, &Expr::Name("saved".into()), &[]).is_ok());
    let list = Expr::Collection {
        family: Some("List".into()),
        mutable: true,
        entries: vec![(None, Expr::Int(1)), (None, Expr::Int(2))],
    };
    let list_type = Type::named("MutableList", vec![Type::Int]);
    assert!(check_index(&list, &Expr::Int(2), &list_type).is_err());
    let insert = signature("$builtin.insert", &["index", "element"]);
    let remove = signature("$builtin.removeAt", &["index"]);
    let bound = Expr::BoundReference(Box::new(list), "insert".into(), 1);
    assert!(check_call_bounds(&insert, &bound, &[(None, Expr::Int(2)), (None, Expr::Int(7))]).is_ok());
    assert!(check_call_bounds(&remove, &bound, &[(None, Expr::Int(2))]).is_err());
    assert!(check_call_bounds(&insert, &bound, &[(None, Expr::Int(3)), (None, Expr::Int(7))]).is_err());
    assert!(check_call_bounds(&remove, &bound, &[(None, Expr::Name("unknown".into()))]).is_ok());
}
