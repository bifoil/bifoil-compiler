//! Structured expression sites preserve branch flow, preparation effects, and loop boundaries.

use super::compile;

#[test]
fn prose_interpolation_initializes_an_enclosing_immutable_binding() {
    compile(
        r#"scene start():
    val result: Int
    fun initialize() -> String:
        result = 3
        return "ready"
    prose:
        ${initialize()}
    prose:
        ${result}
"#,
    )
    .unwrap();
}

#[test]
fn conditional_headers_and_expression_bodies_keep_definite_writes() {
    for site in [
        "if initialize() == 3:\n        Unit",
        "val answer = if true:\n        initialize() + result\n    else:\n        0",
        "when initialize():\n        case 3:\n            Unit\n        else:\n            Unit",
        "val answer = when true:\n        case true:\n            initialize() + result\n        else:\n            return",
    ] {
        let source = format!(
            r#"scene start():
    val result: Int
    fun initialize() -> Int:
        result = 3
        return 3
    {site}
    prose:
        ${{result}}
"#
        );
        compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn selected_bodies_can_consume_the_single_invocation() {
    for site in [
        "if true:\n        initialize()\n        val answer = result",
        "when 1:\n        case 1:\n            initialize()\n            val answer = result",
        "choice:\n        option \"Initialize\":\n            initialize()\n            val answer = result",
        "choice:\n        option \"Hidden\" if false:\n            Unit\n        else:\n            initialize()\n            val answer = result",
    ] {
        let source = format!(
            r#"scene start():
    val result: Int
    fun initialize():
        result = 3
    {site}
"#
        );
        compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn choice_preparation_can_initialize_before_its_wait() {
    for header in [
        "option \"Ready ${initialize()}\":",
        "option \"Ready\" if initialize() == 3:",
        "option \"Ready\" enabledIf initialize() == 3:",
    ] {
        let source = format!(
            r#"scene start():
    val result: Int
    fun initialize() -> Int:
        result = 3
        return 3
    choice:
        {header}
            prose:
                ${{result}}
"#
        );
        compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn repeated_loop_regions_still_cannot_claim_one_invocation() {
    for site in [
        "while initialize() == 3:\n        break",
        "for i in 1..2:\n        initialize()",
        "prose:\n        @while true:\n            ${initialize()}\n            @continue",
    ] {
        let source = format!(
            r#"scene start():
    val result: Int
    fun initialize() -> Int:
        result = 3
        return 3
    {site}
"#
        );
        let error = compile(&source).expect_err("loop regions may execute repeatedly");
        assert!(
            error.contains("captured val") || error.contains("could be initialized more than once"),
            "{source}\n{error}"
        );
    }
}

#[test]
fn skipped_branch_and_once_option_writes_are_not_definite() {
    for site in [
        "if false:\n        initialize()",
        "choice:\n        option \"Ready ${initialize()}\" once:\n            Unit\n        else:\n            Unit",
        "choice:\n        option \"Hidden ${initialize()}\" if false:\n            Unit\n        else:\n            Unit",
    ] {
        let source = format!(
            r#"scene start():
    val result: Int
    fun initialize() -> Int:
        result = 3
        return 3
    {site}
    prose:
        ${{result}}
"#
        );
        let error = compile(&source).expect_err("a skipped site cannot establish a definite write");
        assert!(error.contains("not definitely initialized"), "{source}\n{error}");
    }
}

#[test]
fn iterable_initialization_occurs_before_repeated_traversal() {
    compile(
        r#"scene start():
    val result: Int
    fun initialize() -> List<Int>:
        result = 3
        return [1, 2]
    for i in initialize():
        val answer = result + i
    prose:
        ${result}
"#,
    )
    .unwrap();
}

#[test]
fn when_case_tests_and_guards_initialize_only_their_reached_paths() {
    for header in ["case initialize():", "case 3 if initialize() == 3:"] {
        let source = format!(
            r#"scene start():
    val result: Int
    fun initialize() -> Int:
        result = 3
        return 3
    when 3:
        {header}
            prose:
                ${{result}}
        else:
            Unit
"#
        );
        compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn prose_and_choice_preparation_retain_the_non_suspending_boundary() {
    for site in [
        "prose:\n        ${initialize()}",
        "choice:\n        option \"${initialize()}\":\n            Unit",
    ] {
        let source = format!(
            r#"scene start():
    val result: Int
    suspend fun initialize() -> String:
        choice:
            option "Wait":
                Unit
        result = 3
        return "ready"
    {site}
"#
        );
        let error = compile(&source).expect_err("preparation and prose interpolation cannot suspend");
        assert!(error.contains("suspend"), "{source}\n{error}");
    }
}
