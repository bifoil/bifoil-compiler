//! Checked casts preserve runtime types; conversions remain explicit operations.

use super::compile;

#[test]
fn casts_allow_nullable_generic_and_covariant_overlap() {
    compile(
        r#"fun nullable(value: Int?) -> String? = value as? String?
fun <T> read(value: T) -> String? = value as? String
fun <T> target(value: String) -> T? = value as? T
fun never() = never() as Nothing
fun empty() -> List<String>:
    val bottom: List<Nothing> = []
    val values: List<Int> = bottom
    return values as List<String>
scene start():
    Unit
"#,
    )
    .unwrap();
}

#[test]
fn casts_allow_a_common_function_subtype_without_adapters() {
    compile(
        r#"fun never(value: Any?) -> Nothing:
    while true:
        Unit
scene start():
    val initial: (Int) -> Int = ::never
    val checked = initial as (String) -> String
    val valid: Boolean = checked is (String) -> String
"#,
    )
    .unwrap();
}

#[test]
fn casts_reject_disjoint_types_and_do_not_convert_or_copy() {
    for declaration in [
        "fun invalid(value: Int) -> Float = value as Float",
        "fun invalid(values: List<Int>) -> MutableList<Int> = values as MutableList<Int>",
        "fun invalid(values: Map<Int, Int>) -> Map<String, Int> = values as Map<String, Int>",
        "fun <T: String> invalid(value: T) -> Int = value as Int",
        "fun <T: String?> invalid(value: T) -> Int = value as Int",
        "fun invalid() -> Int = null as Int",
    ] {
        let source = format!("{declaration}\nscene start():\n    Unit\n");
        let error = compile(&source).expect_err(declaration);
        assert!(
            error.contains("cast between provably incompatible concrete types"),
            "{error}"
        );
    }
}
