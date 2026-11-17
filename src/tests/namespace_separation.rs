//! Type/value separation does not erase ordinary value initialization or lookup.
use super::compile;
use crate::model::Op;

const HEADER: &str = "enum Mood:\n    Happy\n    Quiet\ntypealias Flag = Mood\n";

#[test]
fn value_prefix_owns_member_lookup_after_its_declaration() {
    let module = compile(&format!(
        "{HEADER}{}",
        r#"data class Holder:
    val Happy: Int
fun read() -> Int:
    val before: Flag = Flag.Happy
    val Flag = Holder(7)
    return Flag.Happy
"#
    ))
    .unwrap();
    assert!(
        module.functions["test.read"]
            .code
            .iter()
            .any(|instruction| matches!(&instruction.op, Op::Member { name, .. } if name == "Happy"))
    );
}

#[test]
fn value_prefix_cannot_fall_back_to_an_enum_alias() {
    let error = compile(&format!(
        "{HEADER}fun read():\n    val Flag = \"value\"\n    return Flag.Happy\n"
    ))
    .unwrap_err();
    assert!(error.contains("member"), "{error}");
    let error = compile(&format!(
        "{HEADER}data class Holder:\n    val Happy: Any\nfun read():\n    val Flag: Holder\n    return Flag.Happy\n"
    ))
    .unwrap_err();
    assert!(error.contains("not definitely initialized"), "{error}");
}

#[test]
fn builtin_types_cannot_be_replaced_by_an_alias_or_parameter() {
    for source in [
        "typealias Int = String\n",
        "fun <List> identity(value: List) = value\n",
        "data class String:\n    val value: Int\n",
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
    compile("fun count() -> Int:\n    val List: List<Int> = [1]\n    return List.size\n").unwrap();
}

#[test]
fn exported_alias_and_value_require_distinct_names() {
    for value in [
        "export fun Label() -> String = \"value\"\n",
        "export shared val Label = \"value\"\n",
    ] {
        let error = compile(&format!("export typealias Label = String\n{value}")).unwrap_err();
        assert!(error.contains("require distinct exported names"), "{error}");
    }
    compile("export typealias Label = String\nfun Label() -> String = \"value\"\n").unwrap();
    compile("typealias Label = String\nexport fun Label() -> String = \"value\"\n").unwrap();
}

#[test]
fn generated_field_parameter_reads_do_not_use_enum_shorthand() {
    compile(&format!(
        "{HEADER}{}",
        r#"data class Holder:
    val Happy: Mood
fun construct() -> Holder:
    val value = Holder(Mood.Quiet)
    return value.copy(Happy = Mood.Happy)
"#
    ))
    .unwrap();
    let error = compile(&format!("{HEADER}fun ambiguous(Happy: Mood) -> Mood = Happy\n")).unwrap_err();
    assert!(error.contains("conflicts with a visible binding"), "{error}");
}
