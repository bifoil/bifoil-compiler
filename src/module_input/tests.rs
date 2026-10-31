use super::*;
use crate::model::{Op, digest};
use serde::de::DeserializeSeed;

pub(in crate::module_input) fn module() -> Module {
    crate::compiler::compile(
        crate::parser::parse(
            r#"
export shared val saved: Float = -1.0803725086404524e-296
export fun factory(value: Float = -1.0803725086404524e-296) -> () -> String:
    val word = "😀"
    fun read() -> String = "${word}|${value}|${saved}"
    return ::read
"#,
            "src/helpers.bfl",
        )
        .unwrap(),
        "helpers",
        "src/helpers.bfl",
        &BTreeMap::new(),
        Instant::now(),
    )
    .unwrap()
}
fn spooled(module: &Module) -> streamed::StreamedModule {
    let text = serde_json::to_string_pretty(module).unwrap();
    let mut deserializer = serde_json::Deserializer::from_str(&text);
    streamed::Seed::new(Instant::now())
        .deserialize(&mut deserializer)
        .unwrap()
}
fn resign(module: &mut Module) {
    module.digest.clear();
    module.digest = digest(module).unwrap();
}
#[test]
fn spooled_validation_rejects_bad_code_even_with_a_matching_digest() {
    let mut invalid = module();
    invalid.functions.values_mut().next().unwrap().code.clear();
    resign(&mut invalid);
    assert!(
        check(spooled(&invalid))
            .err()
            .unwrap()
            .contains("malformed function identity or body")
    );

    let mut invalid = module();
    invalid.functions.values_mut().next().unwrap().code[0].span.line = 0;
    resign(&mut invalid);
    assert!(
        check(spooled(&invalid))
            .err()
            .unwrap()
            .contains("invalid instruction source location")
    );

    let mut invalid = module();
    invalid.functions.values_mut().next().unwrap().code[0].op = Op::Jump { target: usize::MAX };
    resign(&mut invalid);
    assert!(
        check(spooled(&invalid))
            .err()
            .unwrap()
            .contains("invalid instruction target")
    );

    let mut invalid = module();
    invalid.digest = "0".repeat(64);
    assert!(check(spooled(&invalid)).err().unwrap().contains("digest mismatch"));

    let mut invalid = serde_json::to_value(module()).unwrap();
    invalid["functions"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()["code"][0]["op"] = "unknown-op".into();
    let text = serde_json::to_string(&invalid).unwrap();
    let mut deserializer = serde_json::Deserializer::from_str(&text);
    let error = streamed::Seed::new(Instant::now())
        .deserialize(&mut deserializer)
        .err()
        .unwrap();
    assert!(error.to_string().contains("unknown variant"));
}

#[test]
fn spooled_nominal_metadata_rejects_repeated_or_empty_variant_names() {
    for variants in [
        vec!["helpers.Mood.Happy".to_owned(), "helpers.Mood.Happy".to_owned()],
        vec!["helpers.Mood.".to_owned()],
    ] {
        let mut invalid = module();
        invalid.types.insert(
            "helpers.Mood".into(),
            std::sync::Arc::new(crate::model::TypeDef {
                parameters: vec![],
                field_order: vec![],
                kind: "enum".into(),
                fields: Default::default(),
                methods: Default::default(),
                private_methods: vec![],
                parent: None,
                variants,
            }),
        );
        resign(&mut invalid);
        let error = check(spooled(&invalid)).err().unwrap();
        assert!(error.contains("invalid declared variant"), "{error}");
    }
}

#[test]
fn spooled_signatures_reject_invalid_parameter_interfaces() {
    for mutation in 0..4 {
        let mut invalid = module();
        invalid.exports.clear();
        let signature = std::sync::Arc::make_mut(&mut invalid.functions.get_mut("helpers.factory").unwrap().signature);
        let parameters = std::sync::Arc::make_mut(&mut signature.params);
        match mutation {
            0 => parameters[0].name.clear(),
            1 => {
                let mut duplicate = parameters.to_vec();
                duplicate.push(duplicate[0].clone());
                signature.params = duplicate.into();
            }
            2 => {
                let mut misplaced = parameters.to_vec();
                misplaced[0].vararg = true;
                misplaced[0].default = false;
                let mut following = misplaced[0].clone();
                following.name = "following".into();
                following.vararg = false;
                misplaced.push(following);
                signature.params = misplaced.into();
            }
            3 => parameters[0].vararg = true,
            _ => unreachable!(),
        }
        resign(&mut invalid);
        let error = check(spooled(&invalid))
            .err()
            .expect("malformed parameter interface must be rejected");
        assert!(error.contains("invalid function parameter interface"), "{error}");
    }
}

#[test]
fn retained_facts_preserve_reference_and_capture_diagnostics() {
    fn rejected(mut module: Module, expected: &str, resign_digest: bool) {
        if resign_digest {
            resign(&mut module);
        }
        let streamed = check(spooled(&module)).err().unwrap();
        let direct = check(module).err().unwrap();
        assert_eq!(streamed, direct);
        assert!(streamed.contains(expected), "{streamed}");
    }
    for (target, captures, expected) in [
        (
            "helpers.missing",
            vec!["twice", "twice"],
            "invalid local closure target",
        ),
        (
            "helpers.factory",
            vec!["twice", "twice"],
            "invalid closure capture names",
        ),
        ("helpers.factory", vec![""], "invalid closure capture names"),
    ] {
        let mut invalid = module();
        invalid.functions.values_mut().next().unwrap().code[0].op = Op::FunctionValue {
            function: target.into(),
            captures: Some(captures.into_iter().map(str::to_owned).collect()),
        };
        rejected(invalid, expected, true);
    }

    // A late reference can precede its function in serialized map order. The
    // contract must still agree with sites in every other body of the module.
    let mut invalid = module();
    let mut site = invalid
        .functions
        .values()
        .flat_map(|function| &function.code)
        .find(|instruction| matches!(instruction.op, Op::Closure { .. }))
        .unwrap()
        .clone();
    if let Op::Closure { captures, .. } = &mut site.op {
        *captures = Some(vec!["unexpected".into()]);
    }
    invalid.functions.values_mut().next().unwrap().code.push(site);
    rejected(invalid, "inconsistent named closure captures", true);

    // Malformed bodies do not bypass the canonical digest check or reorder its
    // diagnostic merely because the parse already retained structural facts.
    let mut invalid = module();
    invalid.functions.values_mut().next().unwrap().code[0].span.line = 0;
    invalid.digest = "0".repeat(64);
    rejected(invalid, "digest mismatch", false);
}

#[test]
fn pure_spooled_bodies_share_empty_structural_facts() {
    let source = "fun first() -> Int = 1\nfun second() -> Int = 2\n";
    let original = crate::compiler::compile(
        crate::parser::parse(source, "src/pure.bfl").unwrap(),
        "pure",
        "src/pure.bfl",
        &BTreeMap::new(),
        Instant::now(),
    )
    .unwrap();
    let loaded = spooled(&original);
    let first = loaded.functions["pure.first"].code.structural_facts();
    let second = loaded.functions["pure.second"].code.structural_facts();
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    check(loaded).unwrap();
}
