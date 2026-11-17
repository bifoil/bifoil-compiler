//! Operand contracts independently enumerate supported numeric and nullable cases.
use super::*;

#[test]
fn coverage_numeric_operators_keep_int_division_and_float_widening_distinct() {
    for left in [Type::Int, Type::Float] {
        for right in [Type::Int, Type::Float] {
            let ordinary = if left == Type::Int && right == Type::Int {
                Type::Int
            } else {
                Type::Float
            };
            for operator in ["+", "-", "*", "/"] {
                assert_eq!(binary_type(operator, &left, &right).unwrap(), ordinary);
            }
            for operator in ["%+", "%-"] {
                assert_eq!(binary_type(operator, &left, &right).unwrap(), Type::Int);
            }
            for operator in ["<", "<=", ">", ">=", "==", "!=", "===", "!=="] {
                assert_eq!(binary_type(operator, &left, &right).unwrap(), Type::Boolean);
            }
            assert_eq!(
                binary_type("%", &left, &right).is_ok(),
                left == Type::Int && right == Type::Int
            );
            assert!(binary_type("unsupported", &left, &right).is_err());
        }
    }
    for invalid in [Type::Boolean, Type::Unit, Type::Int.nullable(), Type::Any] {
        for operator in [
            "-", "*", "/", "%", "%+", "%-", "..", "..<", "downTo", "step", "&&", "||",
        ] {
            assert!(
                binary_type(operator, &invalid, &Type::Int).is_err(),
                "{invalid} {operator} Int"
            );
            assert!(
                binary_type(operator, &Type::Int, &invalid).is_err(),
                "Int {operator} {invalid}"
            );
        }
    }
}

#[test]
fn coverage_membership_checks_container_elements_range_queries_and_nullable_receivers() {
    for family in ["IntRange", "IntProgression"] {
        let range = Type::named(family, vec![]);
        for operator in ["in", "!in"] {
            assert_eq!(binary_type(operator, &Type::Int, &range).unwrap(), Type::Boolean);
            for invalid in [Type::Float, Type::Char, Type::Int.nullable()] {
                assert!(binary_type(operator, &invalid, &range).is_err());
            }
            assert!(binary_type(operator, &Type::Int, &range.nullable()).is_err());
        }
        assert_eq!(
            binary_type("step", &range, &Type::Int).unwrap(),
            Type::named("IntProgression", vec![])
        );
        assert!(binary_type("step", &range, &Type::Float).is_err());
    }
    for family in ["List", "MutableList", "Set", "MutableSet"] {
        let items = Type::named(family, vec![Type::Float]);
        assert_eq!(binary_type("in", &Type::Int, &items).unwrap(), Type::Boolean);
        assert!(binary_type("in", &Type::String, &items).is_err());
    }
    let map = Type::named("Map", vec![Type::String, Type::Int]);
    assert_eq!(binary_type("!in", &Type::Char, &map).unwrap(), Type::Boolean);
    assert!(binary_type("in", &Type::Int, &map).is_err());
    assert_eq!(binary_type("in", &Type::Char, &Type::String).unwrap(), Type::Boolean);
}

#[test]
fn coverage_equality_and_fallback_do_not_erase_capabilities_or_nullability() {
    let callback = Type::Function {
        params: vec![].into(),
        result: std::sync::Arc::new(Type::Unit),
        suspend: false,
        vararg: false,
    };
    for operator in ["==", "!=", "===", "!=="] {
        assert!(binary_type(operator, &callback, &callback).is_err());
        assert_eq!(
            binary_type(operator, &callback.nullable(), &Type::Null).unwrap(),
            Type::Boolean
        );
        assert_eq!(
            binary_type(operator, &Type::Null, &callback.nullable()).unwrap(),
            Type::Boolean
        );
    }
    for operator in ["==", "!="] {
        assert!(binary_type(operator, &Type::Int, &Type::String).is_err());
        assert_eq!(
            binary_type(
                operator,
                &Type::named("List", vec![Type::Int]),
                &Type::named("MutableList", vec![Type::Float])
            )
            .unwrap(),
            Type::Boolean
        );
    }
    for operator in ["===", "!=="] {
        assert_eq!(binary_type(operator, &Type::Int, &Type::String).unwrap(), Type::Boolean);
    }
    assert_eq!(binary_type("?:", &Type::Null, &Type::String).unwrap(), Type::String);
    assert_eq!(
        binary_type("?:", &Type::Int.nullable(), &Type::Float).unwrap(),
        Type::Float
    );
    assert!(binary_type("?:", &Type::Int.nullable(), &Type::String).is_err());
    for operator in ["&&", "||"] {
        assert_eq!(
            binary_type(operator, &Type::Boolean, &Type::Nothing).unwrap(),
            Type::Boolean
        );
        assert!(binary_type(operator, &Type::String, &Type::Nothing).is_err());
    }
    assert_eq!(binary_type("+", &Type::Int, &Type::Nothing).unwrap(), Type::Nothing);
    assert_eq!(binary_type("+", &Type::Nothing, &Type::String).unwrap(), Type::Nothing);
    assert_eq!(binary_type("+", &Type::Pending, &Type::Int).unwrap(), Type::Pending);
    assert_eq!(binary_type("+", &Type::Int, &Type::Pending).unwrap(), Type::Pending);
    for primitive in [Type::Int, Type::Float, Type::Char, Type::Boolean] {
        assert_eq!(binary_type("+", &Type::String, &primitive).unwrap(), Type::String);
        assert_eq!(binary_type("+", &primitive, &Type::String).unwrap(), Type::String);
    }
    assert!(binary_type("+", &Type::String, &Type::Unit).is_err());
    assert!(binary_type("+", &Type::Unit, &Type::String).is_err());
}
