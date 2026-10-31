//! Selection proves preparation ran, without reviving stale value/type facts.

use super::compile;

#[test]
fn selected_once_and_conditionally_visible_labels_initialize_their_cells() {
    for modifier in ["once", "if visible"] {
        let source = format!(
            r#"suspend fun run(visible: Boolean) -> Int:
    val result: Int
    fun initialize() -> String:
        result = 3
        return "Ready"
    choice:
        option "${{initialize()}}" {modifier}:
            return result
        else:
            return 0
"#
        );
        compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn another_option_does_not_imply_a_skipped_label_was_evaluated() {
    let error = compile(
        r#"suspend fun run(visible: Boolean) -> Int:
    val result: Int
    fun initialize() -> String:
        result = 3
        return "Ready"
    choice:
        option "${initialize()}" if visible:
            return result
        option "Other":
            return result
"#,
    )
    .expect_err("choosing Other does not prove the first option was prepared");
    assert!(error.contains("not definitely initialized"), "{error}");
}

#[test]
fn selected_preparation_does_not_restore_narrowing_invalidated_by_later_options() {
    let error = compile(
        r#"suspend fun run(visible: Boolean) -> Int:
    val result: Int
    var name: String? = null
    fun initialize() -> String:
        name = "ready"
        result = 3
        return "First"
    fun clear() -> String:
        name = null
        return "Other"
    choice:
        option "${initialize()}" if visible && name == null:
            return result + name.length
        option "${clear()}":
            return 0
"#,
    )
    .expect_err("later preparation may overwrite the mutable value");
    assert!(error.contains("nullable"), "{error}");
}

#[test]
fn a_sole_once_option_initializes_every_normal_exit() {
    compile(
        r#"scene start():
    val result: Int
    fun initialize() -> String:
        result = 3
        return "Only"
    choice:
        option "${initialize()}" once:
            Unit
    prose:
        ${result}
"#,
    )
    .unwrap();
}
