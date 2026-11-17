//! Invalid authored syntax must fail with the original file and a useful diagnostic.
use crate::parser::parse;

#[test]
fn coverage_declaration_errors_report_the_original_source() {
    let cases = [
        ("fun f(vararg values: Int, other: Int):\n    Unit\n", "vararg"),
        ("fun f(vararg values: Int = 1):\n    Unit\n", "vararg"),
        ("legacy scene start() = \"legacy\"\n", "legacy scene"),
        ("scene start() = Unit\n", "block body"),
        ("scene start() -> Int:\n    Unit\n", "scene result"),
        ("data Wrong:\n    val item: Int\n", "class"),
        ("sealed Wrong:\n    case Empty\n", "class"),
        ("data class Record:\n        val item: Int\n", "indentation"),
        ("data class Record:\n    unknown item: Int\n", "field or method"),
        (
            "data class Record:\n    val item: Int\n    val item: Int\n",
            "duplicate field",
        ),
        ("enum Mood:\n    case Happy(value: Int)\n", "payload"),
        ("enum Mood:\n    case Happy\n    case Happy\n", "duplicate case"),
        ("sealed class Empty:\n", "requires cases"),
        ("scene start():\n    val item\n", "requires a type"),
        ("scene start():\n    var item: Int extra\n", "unexpected"),
    ];
    for (source, expected) in cases {
        let error = parse(source, "original-invalid.bfl").unwrap_err();
        assert!(error.contains(expected), "expected {expected}: {error}\n{source}");
        assert!(error.contains("original-invalid.bfl"), "{error}");
    }
}

#[test]
fn coverage_statement_errors_reject_detached_branches_and_malformed_choice_headers() {
    for (body, expected) in [
        ("    outer@ if true:\n        Unit\n", "loop labels"),
        ("    for item 1..2:\n        Unit\n", "requires in"),
        ("    shared val count: Int\n", "require initializers"),
        ("    do:\n        Unit\n", "aligned trailing while"),
        ("    do:\n        Unit\n    Unit\n", "requires a trailing while"),
        ("    else:\n        Unit\n", "preceding if"),
        ("    elif true:\n        Unit\n", "preceding if"),
        (
            "    choice:\n            option \"Wrong\":\n                Unit\n",
            "option indentation",
        ),
        (
            "    choice:\n        unknown \"Wrong\":\n            Unit\n",
            "option headers",
        ),
        ("    choice:\n        option 3:\n            Unit\n", "quoted String"),
        (
            "    choice:\n        else:\n            Unit\n        else:\n            Unit\n",
            "duplicate choice fallback",
        ),
        (
            "    choice:\n        else:\n            Unit\n        option \"Late\":\n            Unit\n",
            "fallback must be last",
        ),
    ] {
        let source = format!("scene start():\n{body}");
        let error = parse(&source, "original-statement.bfl").unwrap_err();
        assert!(error.contains(expected), "expected {expected}: {error}\n{source}");
        assert!(error.contains("original-statement.bfl"), "{error}");
    }
}

#[test]
fn coverage_expression_errors_reject_incomplete_literals_calls_and_variadic_lambdas() {
    for expression in [
        "(vararg values: Int, other: Int) => other",
        "(vararg values: Int = 1) => values.size",
        "[1, 2",
        "{1: 2",
        "invoke(1 2)",
        "invoke(name = 1 positional)",
        "mutable 1",
        "value.",
        "value[1, 2]",
        "value is",
        "value as?",
        "(value: Int) value",
        "{1: 2, 3}",
        "{1, 2: 3}",
    ] {
        let source = format!("fun original() = {expression}\n");
        let error = parse(&source, "original-expression.bfl").unwrap_err();
        assert!(error.contains("original-expression.bfl"), "{error}");
    }
}

#[test]
fn coverage_when_payload_patterns_reject_invalid_binding_contracts() {
    for branch in [
        "case !is Outcome.Ready(first, second):",
        "case is Outcome.Ready(first):",
        "case is Outcome.Ready(same, same):",
        "case is Int(value):",
        "case is Character(value):",
        "case is Outcome.Ready(first, second), is Outcome.Empty:",
    ] {
        let source = format!(
            "sealed class Outcome:\n    case Ready(number: Int, text: String)\n    case Empty\n\
             data class Character:\n    val value: Int\n\
             fun inspect(input: Outcome) -> Int:\n    return when input:\n        {branch}\n            1\n        else:\n            0\n\
             scene start():\n    Unit\n"
        );
        assert!(
            super::compile(&source).is_err(),
            "invalid payload bindings were accepted:\n{source}"
        );
    }
}

#[test]
fn coverage_parentheses_preserve_boolean_and_nullable_enum_exhaustiveness() {
    for source in [
        "fun inspect(input: Boolean) -> Int:\n    return when input:\n        case (true):\n            1\n        case (false):\n            0\nscene start():\n    Unit\n",
        "enum Mood:\n    Happy\n    Quiet\nfun inspect(input: Mood?) -> Int:\n    return when input:\n        case (Mood.Happy):\n            1\n        case (Mood.Quiet):\n            2\n        case (null):\n            0\nscene start():\n    Unit\n",
        "fun inspect(input: String?) -> Int:\n    return when input:\n        case (null):\n            0\n        else:\n            input.length\nscene start():\n    Unit\n",
    ] {
        super::compile(source)
            .unwrap_or_else(|error| panic!("parentheses changed exhaustive matching: {error}\n{source}"));
    }
}

#[test]
fn coverage_prose_directive_comments_and_terminal_blank_runs_are_not_story_content() {
    use crate::syntax::StatementKind;
    for ending in ["\n\n", "\n        \n\n"] {
        let source = format!(
            "scene start():\n    prose:\n        Original prose.\n        @# Skipped hash comment\n        @// Skipped slash comment\n        Continued prose.{ending}"
        );
        let ast = parse(&source, "original-prose.bfl").unwrap();
        let StatementKind::Narrative(body) = &ast.functions[0].body[0].kind else {
            panic!("missing prose body");
        };
        assert_eq!(body.len(), 1, "comments and terminal separators are not paragraphs");
        assert!(matches!(body[0].kind, StatementKind::Text(_)));
        assert!(!format!("{body:?}").contains("Skipped"));
    }
}
