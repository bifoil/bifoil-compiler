//! Constant key diagnostics retain Unit/null and exact-comparison distinctions.

use super::compile;

#[test]
fn unit_and_proven_boolean_duplicates_are_compile_errors() {
    for expression in [
        "{Unit: 1, Unit: 2}",
        "{(!false): 1, true: 2}",
        "{(1 < 2): 1, true: 2}",
        "{(3 === 3.0): 1, false: 2}",
    ] {
        let source = format!("scene start():\n    val values = {expression}\n");
        let error = compile(&source).expect_err("known equal keys must fail before runtime");
        assert!(error.contains("duplicate constant map key"), "{error}\n{source}");
    }
}

#[test]
fn unit_and_null_are_different_keys_and_distinct_boolean_constants_remain_valid() {
    compile(
        r#"scene start():
    val values: Map<Unit?, Int> = {Unit: 1, null: 2}
    val flags = {(!false): 3, false: 4}
"#,
    )
    .unwrap();
}

#[test]
fn enum_case_duplicates_are_rejected_after_resolving_expected_type_shorthand() {
    let prefix = "enum Mood:\n    Happy\n    Sad\nscene start():\n";
    for keys in ["Mood.Happy: 1, Happy: 2", "Happy: 1, Mood.Happy: 2"] {
        let source = format!("{prefix}    val values: Map<Mood, Int> = {{{keys}}}\n");
        let error = compile(&source).expect_err("qualified and shorthand cases denote one key");
        assert!(error.contains("duplicate constant map key"), "{error}\n{source}");
    }
    compile(&format!(
        "{prefix}    val values: Map<Mood, Int> = {{Mood.Happy: 1, Sad: 2}}\n"
    ))
    .unwrap();
}
