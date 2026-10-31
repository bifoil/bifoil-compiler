//! Enum bridges use declared case names without permitting mutable composites.

use super::compile;

#[test]
fn enum_boundaries_allow_state_parameters_results_nullable_values_and_varargs() {
    compile(
        r#"enum Mood:
    case Happy
    case Sad
legacy var mood: Mood? = "mood"
legacy fun echo(value: Mood = Mood.Happy) -> Mood? = "helpers" label "echo" result "answer"
legacy fun many(vararg values: Mood) -> Unit = "helpers" label "many"
scene start():
    mood = Mood.Sad
    val result = echo(Mood.Happy)
    many(Mood.Happy, Mood.Sad)
"#,
    )
    .unwrap();
}

#[test]
fn non_enum_nominal_boundaries_remain_rejected() {
    for definition in [
        "data class Record:\n    val number: Int\n",
        "sealed class Record:\n    case Pending\n",
    ] {
        let source = format!(
            r#"{definition}legacy fun echo(value: Record) -> Record = "helpers" label "echo" result "answer"
scene start():
    Unit
"#
        );
        let error = compile(&source).expect_err("data and sum types need another representation");
        assert!(error.contains("legacy interfaces"), "{error}\n{source}");
    }
}
