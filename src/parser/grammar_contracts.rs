//! Cross-production regressions for the development profile source grammar.

use super::parse;
use crate::syntax::{Expr, StatementKind};

fn expression(source: &str) -> Expr {
    let mut ast = parse(&format!("fun example() = {source}\n"), "grammar.bfl").unwrap();
    match ast.functions.remove(0).body.remove(0).kind {
        StatementKind::Return(Some(value)) => value,
        other => panic!("unexpected expression-body AST: {other:?}"),
    }
}

#[test]
fn declaration_productions_keep_distinct_ast_contracts() {
    let ast = parse(
        r#"import story.stats.gold as coins
export shared var total: Int = 0
export typealias Transform<T> = (T) -> T where T: Equatable
export data class Box<out T>:
    private val value: T
    public fun read() -> T = value
export sealed class Outcome<T>:
    case Ready(value: T)
    case Empty
export enum Mood:
    case Happy
    case Quiet
legacy var old: Int = "score"
legacy suspend fun ask(value: Int) -> Int = "native" label "ask" result "answer"
suspend fun <T: Equatable> keep(var value: T, vararg rest: T) -> T where T: StableKey:
    return value
scene start():
    val message = "ready"
    prose:
        ${message}
"#,
        "grammar.bfl",
    )
    .unwrap();
    assert_eq!(ast.imports[0].module, "story.stats");
    assert_eq!(ast.imports[0].alias, "coins");
    assert_eq!(ast.globals.len(), 1);
    assert_eq!(ast.aliases.len(), 1);
    assert_eq!(ast.types.len(), 3);
    assert!(ast.types[0].fields[0].private);
    assert_eq!(ast.types[0].methods[0].name, "read");
    assert_eq!(ast.types[1].variants.len(), 2);
    assert_eq!(ast.legacy_globals[0].target, "score");
    assert_eq!(ast.functions.len(), 3);
    let legacy = ast.functions[0].legacy.as_ref().unwrap();
    assert_eq!(legacy.label.as_deref(), Some("ask"));
    assert_eq!(legacy.result.as_deref(), Some("answer"));
    let keep = &ast.functions[1];
    assert!(keep.suspend && keep.params[0].mutable && keep.params[1].vararg);
    assert_eq!(keep.generics[0].bounds.len(), 2);
    assert!(ast.functions[2].scene);
}

#[test]
fn logical_rows_ignore_comments_and_alignment_but_enforce_block_boundaries() {
    let source = concat!(
        "# outside\r\nscene start():\r\n",
        "\t  val value = (1 +\r\n",
        "    // aligned continuation comment\r\n",
        "  2)\r\n",
        "\t  if true:\r\n",
        "\t\tUnit\r\n",
        "\t  else if false:\r\n",
        "\t\tUnit\r\n",
        "\t  elif false:\r\n",
        "\t\tUnit",
    );
    let ast = parse(source, "layout.bfl").unwrap();
    assert_eq!(ast.functions[0].body.len(), 2);
    let StatementKind::If { branches, .. } = &ast.functions[0].body[1].kind else {
        panic!("expected conditional chain");
    };
    assert_eq!(branches.len(), 3);
    for source in [
        "scene start():\n        Unit\n",
        "scene start():\n    if true:\n            Unit\n",
        "scene start():\n    val value = 1 +",
        "scene start():\n    val value = (1\n",
    ] {
        assert!(parse(source, "invalid.bfl").is_err(), "{source}");
    }
}

#[test]
fn arithmetic_elvis_casts_and_boolean_operations_keep_the_documented_grouping() {
    let Expr::Binary(add, _, right, ..) = expression("1 + 2 * 3") else {
        panic!("expected addition");
    };
    assert_eq!(add, "+");
    assert!(matches!(*right, Expr::Binary(ref operator, ..) if operator == "*"));

    let Expr::Binary(fallback, _, right, ..) = expression("first ?: second ?: third") else {
        panic!("expected fallback");
    };
    assert_eq!(fallback, "?:");
    assert!(matches!(*right, Expr::Binary(ref operator, ..) if operator == "?:"));

    let Expr::Binary(disjunction, left, ..) = expression("value in 1..9 step 2 && true || false") else {
        panic!("expected disjunction");
    };
    assert_eq!(disjunction, "||");
    let Expr::Binary(conjunction, membership, ..) = *left else {
        panic!("expected conjunction");
    };
    assert_eq!(conjunction, "&&");
    let Expr::Binary(operator, _, progression, ..) = *membership else {
        panic!("expected membership");
    };
    assert_eq!(operator, "in");
    assert!(matches!(*progression, Expr::Binary(ref operator, ..) if operator == "step"));

    let Expr::Unary(operator, operand, ..) = expression("-value as Float") else {
        panic!("expected prefix negation");
    };
    assert_eq!(operator, "-");
    assert!(matches!(*operand, Expr::TypeOp { ref operator, .. } if operator == "as"));
}

#[test]
fn fairmath_grouping_is_checked_before_static_operand_rules() {
    for value in [
        "score %+ 10 + 1",
        "1 + score %- 10",
        "score %+ 10 * 2",
        "score %+ 10 == 20",
        "score %+ 10 %- 5",
        "score %+ 10 && true",
    ] {
        let source = format!("fun example() = {value}\n");
        let error = parse(&source, "fairmath.bfl").unwrap_err();
        assert!(
            error.contains("fairmath combinations require explicit parentheses"),
            "{error}"
        );
    }
    for value in [
        "score %+ 10",
        "(score %+ 10) + 1",
        "score %+ (10 * 2)",
        "(score %+ 10) %- 5",
    ] {
        expression(value);
    }
}

#[test]
fn labels_require_a_loop_in_ordinary_code_and_prose_directives() {
    let invalid = [
        "label@ if true:\n        Unit\n",
        "label@ when true:\n        case true:\n            Unit\n",
        "label@ choice:\n        option \"Continue\":\n            Unit\n",
        "label@ prose:\n        text\n",
        "label@ return\n",
        "label@ Unit\n",
        "label@ if (\n        true\n    ):\n        Unit\n",
    ];
    for statement in invalid {
        let source = format!("scene start():\n    {statement}");
        let error = super::parse(&source, "labels.bfl").unwrap_err();
        assert!(error.starts_with("labels.bfl:2:"), "{error}");
        assert!(error.contains("loop labels require for, while, or do"), "{error}");
    }
    let source = "scene start():\n    prose:\n        @label@ if true:\n            content\n";
    let error = super::parse(source, "labels.bfl").unwrap_err();
    assert_eq!(error, "labels.bfl:3: loop labels require for, while, or do");
}

#[test]
fn all_selected_labeled_loop_forms_remain_valid() {
    super::parse(
        r#"scene start():
    first@ for i in 1..2:
        continue@first
    second@ while false:
        break@second
    third@ do:
        break@third
    while true
    prose:
        @fourth@ for j in 1..2:
            @continue@fourth
"#,
        "labels.bfl",
    )
    .unwrap();
}

#[test]
fn repeated_comparisons_remain_binary_expressions_with_ordinary_types() {
    let Expr::Binary(operator, left, ..) = expression("1 < 2 < 3") else {
        panic!("expected ordinary comparison");
    };
    assert_eq!(operator, "<");
    assert!(matches!(*left, Expr::Binary(ref operator, ..) if operator == "<"));
}

#[test]
fn source_newlines_normalize_before_triple_template_decoding() {
    let ast = parse(
        "fun text() -> String = \"\"\"\r\n    alpha\r\n      beta\r\n    \"\"\"\r\n",
        "text.bfl",
    )
    .unwrap();
    let StatementKind::Return(Some(Expr::String(text))) = &ast.functions[0].body[0].kind else {
        panic!("expected decoded triple string");
    };
    assert_eq!(text, "\nalpha\n  beta\n    ");
    let escaped = expression(r#""""a\r\nb""""#);
    assert!(matches!(escaped, Expr::String(ref text) if text == "a\r\nb"));
}
