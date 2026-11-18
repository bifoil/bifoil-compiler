use super::prose_content;
use crate::parser::parse;

#[test]
fn a_tab_crossing_the_structural_boundary_preserves_its_remaining_alignment() {
    assert_eq!(prose_content(" \tΩ", 4), ("Ω", 1));
    assert_eq!(prose_content("   \t😀", 4), ("😀", 3));
    assert_eq!(prose_content("\t Ω", 4), (" Ω", 0));
    assert_eq!(prose_content("Ω", 4), ("Ω", 0));
    assert_eq!(prose_content("   ", 4), ("", 0));
}

#[test]
fn prose_directives_reject_an_extra_structural_level_at_the_authored_line() {
    let error = parse("scene start():\n    prose:\n            @Unit\n", "directive.bfl").unwrap_err();
    assert_eq!(
        error,
        "directive.bfl:3: directive requires one additional structural level"
    );
    assert!(parse("scene start():\n    prose:\n        @Unit\n", "directive.bfl").is_ok());
}

fn nested_prose(levels: usize) -> String {
    let mut source = String::from("scene start():\n");
    for level in 0..levels {
        source.push_str(&"    ".repeat(level + 1));
        source.push_str(if level == 0 { "prose:\n" } else { "@prose:\n" });
    }
    source.push_str(&"    ".repeat(levels + 1));
    source.push_str("Original text.\n");
    source
}

#[test]
fn prose_nesting_obeys_the_same_block_budget_as_ordinary_code() {
    assert!(parse(&nested_prose(31), "nested.bfl").is_ok());
    let error = parse(&nested_prose(32), "nested.bfl").unwrap_err();
    assert!(error.contains("block nesting budget exceeded"), "{error}");
}
