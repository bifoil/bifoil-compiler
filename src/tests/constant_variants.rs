//! Payload-free generic constants use the same expected-type inference as constructors.

use super::compile;
use crate::model::Type;

#[test]
fn constant_variants_infer_nullable_parent_and_contextual_collection_arguments() {
    let module = compile(
        r#"sealed class Outcome<out T>:
    case Ready(value: T)
    case Empty
fun text() -> Outcome<String> = Outcome.Empty
fun optional() -> Outcome<Int>? = Outcome.Empty
fun <T> generic() -> Outcome<T> = Outcome.Empty
scene start():
    val maybe: Outcome<String>? = Outcome.Empty
    val items: List<Outcome<Int>> = [Outcome.Empty]
    val produce: () -> Outcome<String> = () => Outcome.Empty
    val retained = generic<String>()
"#,
    )
    .unwrap();
    assert_eq!(
        module.functions["test.text"].signature.result,
        Type::named("test.Outcome", vec![Type::String]),
    );
    assert_eq!(
        module.functions["test.optional"].signature.result,
        Type::named("test.Outcome", vec![Type::Int]).nullable(),
    );
}

#[test]
fn constant_variants_infer_through_aliases_and_check_capability_bounds() {
    compile(
        r#"sealed class Outcome<T: Equatable>:
    case Empty
typealias TextResult = Outcome<String>
fun consume(input: TextResult) -> TextResult = input
fun select() -> TextResult = consume(Outcome.Empty)
scene start():
    val value: TextResult = Outcome.Empty
"#,
    )
    .unwrap();
    let invalid = r#"sealed class Outcome<T: Equatable>:
    case Empty
scene start():
    val value: Outcome<() -> Int> = Outcome.Empty
"#;
    assert!(compile(invalid).is_err());
}

#[test]
fn constant_variants_require_a_concrete_inference_source() {
    for body in [
        "val result = Outcome.Empty",
        "val result: Any = Outcome.Empty",
        "val result: Outcome<*> = Outcome.Empty",
    ] {
        let source = format!("sealed class Outcome<T>:\n    case Empty\nscene start():\n    {body}\n",);
        assert!(compile(&source).is_err(), "{source}");
    }
}

#[test]
fn generic_constant_cases_complete_checked_nullable_sum_coverage() {
    compile(
        r#"sealed class Outcome<T: Equatable>:
    case Ready(value: T)
    case Empty
fun render(input: Outcome<String>?) -> String:
    return when input:
        case null:
            "null"
        case is Outcome.Ready<String>(payload):
            payload
        case Outcome.Empty:
            "empty"
scene start():
    val result = render(Outcome.Empty)
"#,
    )
    .unwrap();
}

#[test]
fn generic_constant_cases_do_not_cover_guarded_or_unrelated_variants() {
    for cases in [
        "case Outcome.Empty if flag:\n            0",
        "case Other.Empty:\n            0",
    ] {
        let source = r#"sealed class Outcome<T: Equatable>:
    case Ready(value: T)
    case Empty
sealed class Other:
    case Empty
fun render(input: Outcome<String>, flag: Boolean) -> Int:
    return when input:
        case is Outcome.Ready<String>(payload):
            1
        CASES
scene start():
    Unit
"#
        .replace("CASES", cases);
        crate::parser::parse(&source, "negative.bfl").unwrap();
        assert!(compile(&source).is_err(), "{source}");
    }
}
