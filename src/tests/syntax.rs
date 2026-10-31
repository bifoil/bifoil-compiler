//! Layout, text, and expression grammar using original Bifoil programs.

use super::compile;

#[test]
fn keywords_are_reserved_identifiers() {
    assert!(
        compile(
            r#"scene start():
    val if = 1
"#
        )
        .unwrap_err()
        .contains("reserved keyword")
    );
}
#[test]
fn choice_interpolation_cannot_suspend() {
    assert!(
        compile(
            r#"suspend fun ask() -> Int = 1
scene start():
    choice:
        option "${ask()}":
            return
"#
        )
        .unwrap_err()
        .contains("suspend context")
    );
    compile(
        r#"scene start():
    choice:
        option "go" if false:
            return
        else:
            return
"#,
    )
    .unwrap();
}
#[test]
fn indentation_and_unicode() {
    compile(
        r#"scene start():
	val name = "\u{1f600}"
	prose:
		Hello ${name}.
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"fun bad() = "\u{d800}"
"#
        )
        .is_err()
    );
}
#[test]
fn nullable_header_colons_and_continued_elvis_are_distinct() {
    compile(
        r#"fun value() -> String?:
    return null
"#,
    )
    .unwrap();
    compile(
        r#"fun check(value: Any?) -> Boolean:
    if value is String?:
        return true
    return false
"#,
    )
    .unwrap();
    compile(
        r#"fun factory() -> (() -> Int)?:
    return null
"#,
    )
    .unwrap();
    compile(
        r#"fun fallback(value: String?) -> String:
    return value ?:
        "missing"
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"fun invalid() -> Int??:
    return null
"#
        )
        .unwrap_err()
        .contains("repeated nullable marker")
    );
}
#[test]
fn interpolation_balances_nested_triple_text_and_quoted_braces() {
    compile(
        r####"scene start():
    val triple = "Value ${"""a" } b"""}"
    val nested = "Value ${"Inner ${"}"}"}"
    val mapping = "Map ${{"quote": """x" } y"""}["quote"]!!}"
    prose:
        ${triple}|${nested}|${mapping}
"####,
    )
    .unwrap();
}
#[test]
fn interpolation_continues_across_code_comments_and_preserves_literal_rules() {
    compile(
        r####"scene start():
    val message = "Value ${(
        1 + // } """ ignored
        2
    )}!"
    prose:
        ${message}
"####,
    )
    .unwrap();
    for source in [
        "scene start():\n    val value = \"first\nsecond\"\n",
        r#"scene start():
    val value = "\u{+41}"
"#,
        r#"scene start():
    val value = "\u+041"
"#,
        r#"scene start():
    val value = "\U0000D800"
"#,
    ] {
        assert!(compile(source).is_err(), "unexpected success: {source}");
    }
}
#[test]
fn prose_keeps_expression_newlines_and_literal_directive_prefixes() {
    compile(
        r####"scene start():
    var score = 0
    prose:
        Count ${(
            1 + // } """ ignored
            2
        )}; ${"""first
        @not a directive

        last""".length}.
        @score += 1
        End ${score}.
"####,
    )
    .unwrap();
    let source = "scene start():\n    val first = 1\n    val text = \"unclosed\n";
    let parsed = crate::parser::parse(source, "example.bfl").err().unwrap();
    let discovered = crate::parser::imports(source, "example.bfl").err().unwrap();
    assert_eq!(parsed, discovered);
    assert!(parsed.starts_with("example.bfl:3:"), "{parsed}");
}
#[test]
fn interpolated_functions_retain_nonzero_source_locations() {
    let module = compile(
        r#"scene start():
    val text = "${((value: Int) => value + 1)(2)}"
    choice:
        option "${((value: Int) => value + 1)(3)}":
            prose:
                ${((value: Int) => value + 1)(4)}|${text}
"#,
    )
    .unwrap();
    assert!(
        module
            .functions
            .values()
            .flat_map(|function| &function.code)
            .all(|instruction| instruction.span.line > 0)
    );
}
#[test]
fn conditional_lookahead_preserves_following_blocks_and_literal_prose() {
    compile(
        r#"scene start():
    if true:
        Unit
    val number = if true:
        1
    else:
        2
    val callback = () =>:
        3
    val other = if false:
        4
    else:
        5
    val tail = if true:
        6
    else:
        7
    prose:
        @if true:
            kept
        else: literal narrative
        ${number}|${callback()}|${other}|${tail}
"#,
    )
    .unwrap();
}
#[test]
fn negation_preserves_identifier_keyword_prefixes() {
    compile(
        r#"fun <T> identity(value: T) -> T = value
scene start():
    val inside = true
    val isReady = false
    val list: List<Int> = [1, 2]
    prose:
        ${!inside}|${!isReady}|${1 !in list}|${"x" !is Int}
"#,
    )
    .unwrap();
}
#[test]
fn continued_expression_functions_keep_their_physical_source_lines() {
    for (source, line) in [
        (
            r#"scene start():
    val message =
      "${((value: Int) => 10 / value)(0)}"
    prose:
        ${message}
"#,
            3,
        ),
        (
            r#"scene start():
    prose:
        First narrative line.
        Second narrative line.
        ${(
            (value: Int) => 10 / value
        )(0)}
"#,
            6,
        ),
        (
            r#"scene start():
    val divide = (value: Int) =>
      10 / value
    val answer = divide(0)
    prose:
        ${answer}
"#,
            3,
        ),
        (
            r#"scene start():
    if true && (
        ((value: Int) => 10 / value)(0) == 1
    ):
        prose:
            unreachable
"#,
            3,
        ),
    ] {
        let module = compile(source).unwrap();
        let locations = module
            .functions
            .values()
            .flat_map(|function| &function.code)
            .filter(|instruction| matches!(&instruction.op, crate::model::Op::Binary { operator } if operator == "/"))
            .map(|instruction| instruction.span.line)
            .collect::<Vec<_>>();
        assert_eq!(locations, vec![line]);
    }
}
#[test]
fn continued_block_headers_keep_the_opening_structural_level() {
    compile(
        r#"scene start():
    val branch = if true &&
      true:
        3
    else:
        4
    val transform = (value:
      Int) =>:
        value + 1
    prose:
        ${branch}|${transform(branch)}
"#,
    )
    .unwrap();
}

#[test]
fn long_continuations_and_postfix_chains_fail_with_depth_diagnostics() {
    let arithmetic = "scene start():\n    val total = 0 +\n".to_owned() + &"      0 +\n".repeat(20_000) + "      0\n";
    let postfix =
        "fun chained(value: String) = value.\n".to_owned() + &"      length.\n".repeat(20_000) + "      length\n";
    for source in [arithmetic, postfix] {
        assert!(source.len() < crate::model::MAX_SOURCE);
        let error = crate::parser::parse(&source, "src/continuations.bfl").unwrap_err();
        assert!(error.contains("expression tree budget exceeded"), "{error}");
    }
}

#[test]
fn chained_conditions_keep_their_own_header_diagnostics() {
    for header in ["elif 3:", "else if 3:", "elif missing:"] {
        let source = format!("scene start():\n    if false:\n        Unit\n    {header}\n        Unit\n");
        let error = super::compile(&source).unwrap_err();
        assert!(error.contains("src/test.bfl:4:"), "{error}");
    }
}
