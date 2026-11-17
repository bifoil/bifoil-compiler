//! Equality eligibility and contextual nullable-result conversions.

use super::compile;
use crate::model::Type;

#[test]
fn explicit_fallback_contexts_allow_copies_and_universal_results() {
    let module = compile(
        r#"fun frozen(value: MutableList<Int>?, fallback: List<Float>) -> List<Float> = value ?: fallback
fun editable(value: List<Int>?, fallback: MutableList<Float>) -> MutableList<Float> = value ?: fallback
fun broad(value: Int?, fallback: String) -> Any = value ?: fallback
fun nullable(value: MutableList<Int>?, fallback: List<Float>?) -> List<Float>? = value ?: fallback
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.broad"].signature.result, Type::Any);
    assert_eq!(
        module.functions["test.frozen"].signature.result,
        Type::named("List", vec![Type::Float])
    );
}

#[test]
fn fallback_inference_keeps_scalar_joins_without_inventing_copies_or_any() {
    let module = compile(
        r#"fun number(value: Int?, fallback: Float) = value ?: fallback
fun text(value: Char?, fallback: String) = value ?: fallback
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.number"].signature.result, Type::Float);
    assert_eq!(module.functions["test.text"].signature.result, Type::String);
    for source in [
        "fun bad(value: Int?, fallback: String) = value ?: fallback",
        "fun bad(value: MutableList<Int>?, fallback: List<Int>) = value ?: fallback",
        "fun bad(value: Int?, fallback: String) -> Boolean = value ?: fallback",
    ] {
        assert!(compile(source).unwrap_err().contains("fallback types"), "{source}");
    }
}

#[test]
fn ordinary_equality_supports_compatible_families_and_exactness_retains_types() {
    compile(
        r#"data class Cell:
    var value: Int
sealed class Result:
    case Value(value: Cell)
    case Pending
fun primitives() = (3 == 3.0) && ('a' == "a") && (3 !== 3.0)
fun lists(a: List<Int>, b: MutableList<Float>) = a == b
fun variants(a: Result, b: Result) = a != b
fun nullable(a: (() -> Int)?) = a == null
"#,
    )
    .unwrap();
    for source in [
        "fun bad() = 3 == \"3\"",
        "fun bad(values: List<Any>) = values == values",
        "fun bad(value: () -> Int) = value === value",
        "data class Box:\n    val callback: () -> Unit\nfun bad(value: Box) = value == value",
    ] {
        assert!(compile(source).unwrap_err().contains("cannot combine"), "{source}");
    }
}

#[test]
fn skipped_nullable_operations_still_require_valid_types_and_effects() {
    for (source, expected) in [
        (
            "fun bad(callback: ((Int) -> Int)?) = callback?.(\"wrong\")",
            "expected Int",
        ),
        (
            "suspend fun ask() -> Int = 1\nfun bad(callback: ((Int) -> Int)?) = callback?.(ask())",
            "suspend context",
        ),
    ] {
        assert!(compile(source).unwrap_err().contains(expected), "{source}");
    }
}
