//! Compound operators use the left value read before right-side effects.

use super::compile;

#[test]
fn compound_fields_keep_guarded_string_and_fairmath_types_across_rhs_calls() {
    compile(
        r#"data class TextBox:
    var text: String?
data class ScoreBox:
    var score: Int?
fun clearText(holder: TextBox) -> String:
    holder.text = null
    return "!"
fun clearScore(holder: ScoreBox) -> Int:
    holder.score = null
    return 20
fun updateText(holder: TextBox) -> String:
    if holder.text == null:
        return "missing"
    holder.text += clearText(holder)
    return holder.text ?: "missing"
fun updateScore(holder: ScoreBox) -> Int:
    if holder.score == null:
        return 0
    holder.score %+= clearScore(holder)
    return holder.score!!
"#,
    )
    .unwrap();
}

#[test]
fn compound_index_reads_precede_key_changes_and_container_mutation() {
    compile(
        r#"fun update(values: MutableMap<Int, String?>) -> String:
    var selected = 0
    fun reset() -> String:
        selected = 1
        values[0] = null
        return "!"
    if values[selected] == null:
        return "missing"
    values[selected] += reset()
    return values[0]!!
"#,
    )
    .unwrap();
}

#[test]
fn compound_index_reads_keep_the_addressed_container_when_its_root_changes() {
    compile(
        r#"fun update() -> String:
    var values: MutableList<String?> = mutable ["Ada"]
    val previous = values
    fun replace() -> String:
        values = mutable ["Other"]
        return "!"
    if values[0] == null:
        return "missing"
    values[0] += replace()
    return previous[0]!! + "|" + values[0]!!
"#,
    )
    .unwrap();
}

#[test]
fn effects_during_address_evaluation_cannot_borrow_an_earlier_index_guard() {
    let error = compile(
        r#"fun reset(values: MutableList<String?>) -> Int:
    values[0] = null
    return 0
fun invalid(values: MutableList<String?>):
    if values[0] == null:
        return
    values[reset(values)] += "!"
"#,
    )
    .unwrap_err();
    assert!(error.contains("String?"), "{error}");
}

#[test]
fn nonreturning_rhs_does_not_require_an_operator_result() {
    compile(
        r#"data class Box:
    var value: String?
fun stop(holder: Box) -> String:
    holder.value += return "done"
    return "unreachable"
"#,
    )
    .unwrap();
}
