//! Helper admission and compact calls must not bypass ordinary argument or body checks.
use super::*;

fn function(body: &str) -> FunctionAst {
    let body = body.lines().map(|line| format!("    {line}\n")).collect::<String>();
    crate::parser::parse(&format!("fun helper():\n{body}"), "src/helper.bfl")
        .unwrap()
        .functions
        .remove(0)
}

#[test]
fn helper_admission_requires_a_local_nonlegacy_function_with_a_body() {
    let ordinary = function("target = 1");
    assert!(supported(&ordinary));
    for change in ["scene", "export", "legacy", "empty"] {
        let mut rejected = ordinary.clone();
        match change {
            "scene" => rejected.scene = true,
            "export" => rejected.exported = true,
            "legacy" => {
                rejected.legacy = Some(crate::model::LegacyTarget {
                    transfer: false,
                    scene: "original".into(),
                    label: Some("helper".into()),
                    result: None,
                })
            }
            "empty" => rejected.body.clear(),
            _ => unreachable!(),
        }
        assert!(!supported(&rejected), "{change}");
    }
}

#[test]
fn compact_assignment_bodies_require_unique_targets_and_pure_plain_assignments() {
    for (body, expected) in [
        ("target = 1", true),
        ("target = 1\nother = true", true),
        ("target = 1\ntarget = 2", false),
        ("target += 1", false),
        ("target = call()", false),
        ("val local = 1", false),
    ] {
        assert_eq!(simple_assignments(&function(body)), expected, "{body}");
    }
}

#[test]
fn compact_calls_keep_grouping_but_never_discard_supplied_arguments_or_receivers() {
    for (body, expected) in [
        ("helper()", Some("helper")),
        ("(helper)()", Some("helper")),
        ("((helper()))", Some("helper")),
        ("helper(1)", None),
        ("object.helper()", None),
        ("helper", None),
    ] {
        let parsed = function(body);
        let StatementKind::Expression(value) = &parsed.body[0].kind else {
            panic!("original expression fixture lost its statement shape");
        };
        assert_eq!(compact_call(value), expected, "{body}");
    }
}
