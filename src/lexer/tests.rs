use super::*;

#[test]
fn multibyte_literals_comments_and_whitespace_preserve_token_locations() {
    let source =
        "val name = \"😀\"\n\u{2003}// комментарий 😀\nval other = \"\"\"\n    α😀\n    β\n\"\"\"\n'x' != \"z\"";
    let (tokens, lines) = lex_lines(source).unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Name("val".into()),
            Token::Name("name".into()),
            Token::Symbol("=".into()),
            Token::Text("😀".into()),
            Token::Name("val".into()),
            Token::Name("other".into()),
            Token::Symbol("=".into()),
            Token::Text("\nα😀\nβ\n".into()),
            Token::Character("x".into()),
            Token::Symbol("!=".into()),
            Token::Text("z".into()),
        ]
    );
    assert_eq!(lines, vec![0, 0, 0, 0, 2, 2, 2, 2, 6, 6, 6]);
    assert_eq!(lex(source).unwrap(), tokens);
    assert!(tokenize(source, false).unwrap().1.is_empty());
}

#[test]
fn ascii_scanning_preserves_numeric_ranges_and_keyword_operator_boundaries() {
    let source = r#""😀" !in items !inside !is String !island as? Type 1_000.25e-2 0xF_F 0b10_01 1..<3 2..4"#;
    let tokens = lex(source).unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Text("😀".into()),
            Token::Symbol("!in".into()),
            Token::Name("items".into()),
            Token::Symbol("!".into()),
            Token::Name("inside".into()),
            Token::Symbol("!is".into()),
            Token::Name("String".into()),
            Token::Symbol("!".into()),
            Token::Name("island".into()),
            Token::Name("as?".into()),
            Token::Name("Type".into()),
            Token::Number("1000.25e-2".into()),
            Token::Number("0xFF".into()),
            Token::Number("0b1001".into()),
            Token::Number("1".into()),
            Token::Symbol("..<".into()),
            Token::Number("3".into()),
            Token::Number("2".into()),
            Token::Symbol("..".into()),
            Token::Number("4".into()),
        ]
    );
    assert_eq!(lex("\"😀\"§").unwrap_err(), "unexpected character '§'");
}
