//! Interpolation shares ordinary block syntax and retains physical source origins.

use super::compile;
use crate::syntax::{Expr, StatementKind};

#[test]
fn interpolation_accepts_value_blocks_in_strings_prose_and_labels() {
    compile(
        r#"scene start():
    val flag = true
    val quoted = "${if flag:
        "yes"
    else:
        "no"
    }"
    prose:
        ${if flag:
            val local = 2
            local + 3
        else:
            0
        }
    choice:
        option "${when quoted:
            case "yes":
                "Accept"
            else:
                "Reject"
        }":
            Unit
"#,
    )
    .unwrap();
}

#[test]
fn interpolation_uses_code_mode_inside_prose_and_preserves_nested_literal_blocks() {
    compile(
        r#"scene start():
    prose:
        ${if true:
            val selected = "${when 2:
                case 2:
                    "nested"
                else:
                    "other"
            }"
            selected
        else:
            "unused"
        }
"#,
    )
    .unwrap();
}

#[test]
fn block_lambda_and_nested_conditional_operands_parse_inside_interpolation() {
    compile(
        r#"scene start():
    val message = "${((value: Int) =>:
        val doubled = value * 2
        doubled
    )(3)}"
    prose:
        ${1 + (if true:
            2
        else:
            4
        )}
        ${message}
"#,
    )
    .unwrap();
}

#[test]
fn interpolated_literals_on_continued_rows_use_their_own_header_indentation() {
    compile(
        r#"scene start():
    val text = (
            "${if true:
                "aligned"
            else:
                "other"
            }"
    )
    prose:
        ${text}
"#,
    )
    .unwrap();
}

#[test]
fn dedented_triple_string_blocks_and_mixed_layout_retain_valid_structure() {
    let source = r#"scene start():
    val text = """
        ${when true:
            case true:
                "chosen"
            else:
                "other"
        }
    """
    prose:
        ${text}
"#;
    compile(source).unwrap();
    compile(&source.replace("    ", "\t").replace('\n', "\r\n")).unwrap();
}

#[test]
fn ordinary_string_blocks_can_suspend_but_prose_and_labels_cannot() {
    let declaration = "suspend fun ask() -> Int:\n    return 7\n";
    let string = "scene start():\n    val result = \"${if true:\n        ask()\n    else:\n        0\n    }\"\n";
    compile(&format!("{declaration}{string}")).unwrap();
    for body in [
        "    prose:\n        ${if true:\n            ask()\n        else:\n            0\n        }\n",
        "    choice:\n        option \"${if true:\n            ask()\n        else:\n            0\n        }\":\n            Unit\n",
    ] {
        let error = compile(&format!("{declaration}scene start():\n{body}")).unwrap_err();
        assert!(error.contains("suspend"), "{error}");
    }
}

#[test]
fn block_interpolation_statements_keep_their_original_physical_lines() {
    let source = "scene start():\n    prose:\n        result ${if true:\n            val chosen = 7\n            chosen\n        else:\n            0\n        }\n";
    let ast = crate::parser::parse(source, "physical.bfl").unwrap();
    let StatementKind::Narrative(body) = &ast.functions[0].body[0].kind else {
        panic!("missing prose")
    };
    let StatementKind::Text(Expr::Format(parts)) = &body[0].kind else {
        panic!("missing template")
    };
    assert_eq!(parts[1].line, 3);
    let Expr::Block(block) = &parts[1].value else {
        panic!("missing block")
    };
    assert_eq!(block[0].line, 3);
    let StatementKind::If { branches, no } = &block[0].kind else {
        panic!("missing if")
    };
    assert_eq!(branches[0].line, 3);
    assert_eq!(branches[0].body[0].line, 4);
    assert_eq!(branches[0].body[1].line, 5);
    assert_eq!(no[0].line, 7);
    compile(source).unwrap();
}

#[test]
fn malformed_interpolation_reports_the_inner_physical_line_once() {
    for (source, line, message) in [
        (
            "scene start():\n    prose:\n        ${if true:\n            val return = 2\n            7\n        else:\n            0\n        }\n",
            4,
            "reserved keyword",
        ),
        (
            "scene start():\n    val text = \"${1\n        2}\"\n",
            3,
            "unexpected statement",
        ),
        (
            "scene start():\n    prose:\n        ${1 +\n            * 2}\n",
            4,
            "expected an expression",
        ),
        (
            "scene start():\n    prose:\n        ${if true:\n            1\n        else:\n                2\n        }\n",
            6,
            "indentation",
        ),
    ] {
        let error = crate::parser::parse(source, "physical.bfl").unwrap_err();
        assert!(error.starts_with(&format!("physical.bfl:{line}: ")), "{error}");
        assert_eq!(error.matches("physical.bfl").count(), 1, "{error}");
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn nested_interpolation_blocks_share_the_expression_depth_budget() {
    let mut expression = "0".to_owned();
    for _ in 0..40 {
        expression = format!("\"${{{expression}}}\"");
    }
    let source = format!("scene start():\n    val text = {expression}\n");
    let error = crate::parser::parse(&source, "nested.bfl").unwrap_err();
    assert!(error.contains("nesting budget"), "{error}");
}
