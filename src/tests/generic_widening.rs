//! Declared bounds may select existing lossless value widenings.

use super::compile;

#[test]
fn bounds_select_lossless_scalar_and_dependent_arguments() {
    compile(
        r#"fun <T: Float> number(value: T) -> T = value
fun <T: String> text(value: T) -> T = value
fun <T: Float?> optional(value: T) -> T = value
fun <T: U, U: Float> dependent(value: T, upper: U) -> T = value
fun <T: Float> group(vararg values: T) -> List<T> = values
data class Measurement<T: Float>:
    val value: T
scene start():
    val integer = 3
    val character = 'a'
    val nullable: Int? = integer
    val numeric = number(integer)
    val measured = Measurement(integer)
    val string = text(character)
    val present = optional(integer)
    val missing = optional(nullable)
    val related = dependent(integer, 1.5)
    val propagated = dependent(integer, 1)
    val values = group(1, 2)
    val kept: Float? = number(integer)
"#,
    )
    .unwrap();
}

#[test]
fn bounds_select_widenings_through_immutable_collection_layers() {
    compile(
        r#"fun <T: List<Float>> numbers(value: T) -> T = value
fun <T: List<List<Float>>> nested(value: T) -> T = value
fun <T: Set<String>> letters(value: T) -> T = value
fun <T: Map<Float, String>> pairs(value: T) -> T = value
scene start():
    val integers = [1, 2]
    val nestedIntegers = [[1], [2]]
    val characters = {'a', 'b'}
    val entries = {1: 'a', 2: 'b'}
    val a = numbers(integers)
    val b = nested(nestedIntegers)
    val c = letters(characters)
    val d = pairs(entries)
"#,
    )
    .unwrap();
}

#[test]
fn bounds_do_not_infer_family_copies_unwrap_null_or_create_adapters() {
    for (declaration, body) in [
        (
            "fun <T: List<Float>> keep(value: T) -> T = value",
            "val value = mutable [1]\nval result = keep(value)",
        ),
        (
            "fun <T: MutableList<Float>> keep(value: T) -> T = value",
            "val value = [1]\nval result = keep(value)",
        ),
        (
            "fun <T: MutableList<Float>> keep(value: T) -> T = value",
            "val value = mutable [1]\nval result = keep(value)",
        ),
        (
            "fun <T: Float> keep(value: T) -> T = value",
            "val value: Int? = null\nval result = keep(value)",
        ),
        (
            "fun <T: List<MutableList<Float>>> keep(value: T) -> T = value",
            "val value = [mutable [1]]\nval result = keep(value)",
        ),
        (
            "fun <T: Float> keep(value: T) -> T = value",
            "val result = keep<Int>(1)",
        ),
        (
            "fun <T: Float> keep(value: T) -> T = value",
            "val result: (Int) -> Float = ::keep",
        ),
    ] {
        let source = format!(
            "{declaration}\nscene start():\n{}",
            body.lines().map(|line| format!("    {line}\n")).collect::<String>()
        );
        assert!(
            compile(&source).is_err(),
            "invalid bound-directed conversion accepted:\n{source}"
        );
    }
}
