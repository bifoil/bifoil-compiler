//! The built-in Unit value participates in ordinary lexical name rules.

use super::compile;

#[test]
fn the_builtin_unit_value_cannot_be_shadowed_by_a_source_binding() {
    for source in [
        "scene start():\n    val Unit = 7\n",
        "scene start():\n    var Unit: Int\n",
        "fun Unit() -> Int = 7\n",
        "fun receive(Unit: Int) -> Int = Unit\n",
    ] {
        let error = compile(source).expect_err("Unit is already a visible value binding");
        assert!(error.contains("shadow"), "{error}\n{source}");
    }
}

#[test]
fn unit_enum_cases_require_qualification_when_the_builtin_value_is_visible() {
    let source = r#"enum Status:
    case Unit
fun status() -> Status = Unit
"#;
    let error = compile(source).unwrap_err();
    assert!(error.contains("conflicts with a visible binding"), "{error}");
    compile(&source.replace("= Unit", "= Status.Unit")).unwrap();
    compile("fun empty() -> Unit = Unit\nscene start():\n    val values: List<Unit> = [Unit]\n").unwrap();
}
