//! Re-signed hostile metadata must fail structural checks after integrity succeeds.
use super::compile;
use crate::model::{Module, digest};
use serde_json::{Value, json};

type InvalidMetadataCase = (fn(&mut Value), &'static str);

fn fixture() -> Module {
    compile(
        r#"export data class Counter:
    val name: String
    var score: Int = 0
    fun read() -> Int = score
export sealed class Outcome:
    case Success(value: Int)
    case Pending
export enum Mood:
    Happy
    Sad
export fun add(left: Int, right: Int = 1) -> Int = left + right
shared var total: Int = 0
"#,
    )
    .unwrap()
}

fn reject(base: &Module, change: fn(&mut Value), expected: &str) {
    let mut encoded = serde_json::to_value(base).unwrap();
    change(&mut encoded);
    let mut changed: Module = serde_json::from_value(encoded).unwrap();
    changed.digest.clear();
    changed.digest = digest(&changed).unwrap();
    let message = crate::verify(&mut changed).unwrap_err();
    assert!(message.contains(expected), "expected {expected:?}, got {message:?}");
}

#[test]
fn coverage_artifact_boundaries_validate_group_identity_before_using_dependencies() {
    let base = fixture();
    for members in [
        json!([]),
        json!(["other"]),
        json!(["test", "test"]),
        json!(["z", "test"]),
        json!(["test", "z-invalid"]),
    ] {
        let mut encoded = serde_json::to_value(&base).unwrap();
        encoded["group"] = json!({"members":members,"digest":"a".repeat(64)});
        let mut changed: Module = serde_json::from_value(encoded).unwrap();
        changed.digest.clear();
        changed.digest = digest(&changed).unwrap();
        assert!(
            crate::verify(&mut changed)
                .unwrap_err()
                .contains("cyclic module group identity")
        );
    }
    for digest_text in ["a".repeat(63), "g".repeat(64)] {
        let mut changed = base.clone();
        changed.group = Some(std::sync::Arc::new(crate::model::ModuleGroup {
            members: vec!["test".into()],
            digest: digest_text,
        }));
        assert!(
            crate::verify(&mut changed)
                .unwrap_err()
                .contains("cyclic module group identity")
        );
    }
    for (change, expected) in [
        (
            (|m: &mut Value| m["format"] = json!("unknown")) as fn(&mut Value),
            "unsupported module",
        ),
        (|m| m["profile"] = json!("unknown"), "unsupported module"),
        (|m| m["name"] = json!("not/a/module"), "unsupported module"),
    ] {
        reject(&base, change, expected);
    }
}

#[test]
fn coverage_artifact_boundaries_reject_malformed_function_parameters_and_exports() {
    let base = fixture();
    let cases: &[InvalidMetadataCase] = &[
        (
            |m| m["functions"]["test.add"]["signature"]["id"] = json!("test.changed"),
            "function identity",
        ),
        (|m| m["functions"]["test.add"]["code"] = json!([]), "function identity"),
        (
            |m| m["functions"]["test.add"]["signature"]["params"][0]["name"] = json!(""),
            "parameter interface",
        ),
        (
            |m| m["functions"]["test.add"]["signature"]["params"][1]["name"] = json!("left"),
            "parameter interface",
        ),
        (
            |m| m["functions"]["test.add"]["signature"]["params"][0]["vararg"] = json!(true),
            "parameter interface",
        ),
        (
            |m| m["functions"]["test.add"]["signature"]["params"][1]["vararg"] = json!(true),
            "parameter interface",
        ),
        (
            |m| m["exports"]["add"]["signature"]["id"] = json!("test.missing"),
            "signature does not match",
        ),
        (
            |m| m["exports"]["add"]["signature"]["result"] = json!({"kind":"float"}),
            "signature does not match",
        ),
        (
            |m| m["exports"]["Counter"]["id"] = json!("other.Counter"),
            "not owned by its module",
        ),
        (
            |m| m["exports"]["Counter"]["constructor"]["id"] = json!("test.missing"),
            "signature does not match",
        ),
        (
            |m| {
                m["exports"]["foreign"] = json!({"kind":"global","id":"other.total","ty":{"kind":"int"},"mutable":true})
            },
            "binding is not owned",
        ),
        (|m| m["init"] = json!("test.missing"), "module initializer"),
        (|m| m["init"] = json!("test.add"), "module initializer"),
    ];
    for (change, expected) in cases {
        reject(&base, *change, expected);
    }
}

#[test]
fn coverage_artifact_boundaries_validate_nominal_fields_methods_and_variant_ownership() {
    let base = fixture();
    let cases: &[InvalidMetadataCase] = &[
        (
            |m| m["types"]["test.Counter"]["field_order"] = json!(["name", "name"]),
            "field order",
        ),
        (
            |m| m["types"]["test.Counter"]["field_order"] = json!(["name"]),
            "field order",
        ),
        (
            |m| m["types"]["test.Counter"]["field_order"] = json!(["name", "missing"]),
            "field order",
        ),
        (
            |m| m["types"]["test.Counter"]["methods"]["read"][0]["id"] = json!("test.missing"),
            "method signature",
        ),
        (
            |m| m["types"]["test.Counter"]["kind"] = json!("unknown"),
            "nominal kind",
        ),
        (
            |m| m["types"]["test.Counter"]["parent"] = json!({"kind":"int"}),
            "nominal kind",
        ),
        (
            |m| {
                m["types"]["test.Outcome"]["variants"] = json!(["test.Outcome.Pending"]);
                m["types"]["test.Outcome.Success"]["parent"] = json!({"kind":"named","name":"test.Missing","args":[]});
            },
            "variant parent",
        ),
        (
            |m| {
                m["types"]["test.Outcome"]["variants"] = json!(["test.Outcome.Pending"]);
                m["types"]["test.Outcome.Success"]["parent"] = json!({"kind":"named","name":"test.Mood","args":[]});
            },
            "variant parent",
        ),
        (
            |m| m["types"]["test.Outcome"]["variants"] = json!(["test.Outcome.Success", "test.Outcome.Success"]),
            "declared variant",
        ),
        (
            |m| m["types"]["test.Outcome"]["variants"] = json!(["other.Success"]),
            "declared variant",
        ),
        (
            |m| m["types"]["test.Mood"]["variants"] = json!(["test.Mood."]),
            "declared variant",
        ),
        (
            |m| {
                m["types"]["test.Outcome"]["variants"] =
                    json!(["test.Outcome.Success", "test.Outcome.Pending", "test.Outcome.Missing"])
            },
            "declared variant",
        ),
    ];
    for (change, expected) in cases {
        reject(&base, *change, expected);
    }
}

#[test]
fn coverage_artifact_boundaries_reject_foreign_legacy_state_and_inconsistent_shared_initializers() {
    let base = fixture();
    let cases: &[InvalidMetadataCase] = &[
        (
            |m| m["legacy_globals"] = json!({"other.value":{"ty":{"kind":"int"},"mutable":true,"target":"score"}}),
            "legacy state interface",
        ),
        (
            |m| m["legacy_globals"] = json!({"test.value":{"ty":{"kind":"any"},"mutable":true,"target":"score"}}),
            "legacy state interface",
        ),
        (
            |m| m["legacy_globals"] = json!({"test.value":{"ty":{"kind":"int"},"mutable":true,"target":"*invalid"}}),
            "legacy state interface",
        ),
        (
            |m| {
                let value = m["shared"].as_object_mut().unwrap().remove("test.total").unwrap();
                m["shared"]["other.total"] = value;
            },
            "shared binding identity",
        ),
        (
            |m| {
                let value = m["shared"]["test.total"].take();
                m["shared"]["test.add"] = value;
                m["shared"].as_object_mut().unwrap().remove("test.total");
            },
            "shared binding identity",
        ),
        (
            |m| m["shared"]["test.total"]["initializer"] = json!("test.missing"),
            "shared initializer signature",
        ),
        (
            |m| m["shared"]["test.total"]["initializer"] = json!("test.add"),
            "shared initializer signature",
        ),
        (
            |m| m["shared"]["test.total"]["ty"] = json!({"kind":"float"}),
            "shared initializer signature",
        ),
    ];
    for (change, expected) in cases {
        reject(&base, *change, expected);
    }
}
