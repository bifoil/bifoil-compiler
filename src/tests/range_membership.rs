//! Integer range queries use their element type, including checked generic bounds.

use super::compile;

#[test]
fn integer_membership_uses_proven_generic_bounds_and_preserves_explicit_conversions() {
    compile(
        r#"fun <T: Int> inProgression(value: T, items: IntProgression) -> Boolean = value in items
fun <T: IntRange> inRange(value: Int, items: T) -> Boolean = value in items
scene start():
    val first = inProgression(3, 1..5 step 2)
    val second = inRange(3, 1..5)
    var numeric = 3.0
    val converted = Int(numeric) in 1..5
    val negated = 4 !in (1..5 step 2)
"#,
    )
    .unwrap();
}

#[test]
fn integer_membership_rejects_floats_nullable_values_and_unproved_generic_queries() {
    for source in [
        "fun query(value: Float) = value in 1..5\n",
        "fun query(value: Float) = value !in (1..5 step 2)\n",
        "fun query(value: Int?) = value in 1..5\n",
        "fun <T: Equatable> query(value: T) = value in 1..5\n",
        "fun <T: Float> query(value: T) = value in 1..5\n",
        "fun <T: Int?> query(value: T) = value in 1..5\n",
        "fun query(value: Char) = value in 1..5\n",
    ] {
        let error = compile(source).unwrap_err();
        assert!(
            error.contains("integer range membership requires Int"),
            "{error}\n{source}"
        );
    }
}

#[test]
fn range_query_checking_does_not_change_compatible_collection_membership_or_equality() {
    compile(
        r#"scene start():
    val list = 1.0 in [1, 2]
    val set = 1.0 in {1, 2}
    val mapping = 1.0 in {1: "one"}
    val ordinary = 1 == 1.0
    val exact = 1 !== 1.0
"#,
    )
    .unwrap();
}

#[test]
fn progression_step_is_a_contextual_member_in_code_templates_and_continuations() {
    compile(
        r#"scene start():
    val progression = 1..9 step 2
    val magnitude: Int = progression.step
    val aligned: Int = progression.
        step
    val nullable: IntProgression? = progression
    val safe = nullable?.step ?: 0
    choice:
        option "Magnitude ${progression.step}":
            Unit
    prose:
        ${magnitude}|${aligned}|${safe}|${progression.step}
"#,
    )
    .unwrap();
    for source in ["fun step() = 1\n", "scene start():\n    val step = 1\n"] {
        let error = compile(source).unwrap_err();
        assert!(error.contains("reserved keyword 'step'"), "{error}");
    }
}

#[test]
fn compiled_modules_encode_progression_views_without_retyping_the_original_value() {
    let module = compile(
        r#"fun search(value: Int, items: IntProgression) -> Boolean = value in items
fun direct(value: Int, items: IntRange) -> Boolean = value in items
scene start():
    val original = -2147483648..2147483647
    val view: IntProgression = original
    val retained = view is IntRange
"#,
    )
    .unwrap();
    assert_eq!(module.format, "bifoil.module.v6");
    assert!(
        module.functions["test.search"]
            .code
            .iter()
            .any(|instruction| matches!(instruction.op, crate::model::Op::RangeStep))
    );
    assert!(
        !module.functions["test.direct"]
            .code
            .iter()
            .any(|instruction| matches!(instruction.op, crate::model::Op::RangeStep))
    );
}
