//! Reusable modules and artifact validation using original Bifoil programs.

use super::compile;
use crate::compiler;
use crate::model::Module;
use crate::parser;
use crate::verify;
use std::collections::BTreeMap;
use std::time::Instant;

#[test]
fn deeply_nested_supported_types_round_trip_in_modules() {
    let ty = format!("{}Int{}", "(".repeat(31), ") -> Int".repeat(31));
    let module = compile(&format!(
        r#"export fun accept(value: {ty}):
    return
"#
    ))
    .unwrap();
    let json = serde_json::to_string(&module).unwrap();
    let mut restored: Module = serde_json::from_str(&json).unwrap();
    verify(&mut restored).unwrap();
    let too_deep = format!("{}Int{}", "(".repeat(32), ") -> Int".repeat(32));
    assert!(
        compile(&format!(
            r#"fun reject(value: {too_deep}):
    return
"#
        ))
        .is_err()
    );
}
#[test]
fn module_digest_and_validation() {
    let mut module = compile(
        r#"export fun double(value: Int) = value * 2
"#,
    )
    .unwrap();
    verify(&mut module).unwrap();
    let mut changed = module;
    changed.name = "other".into();
    assert!(verify(&mut changed).unwrap_err().contains("digest"));
}
#[test]
fn nominal_artifacts_cannot_replace_owners_or_member_signatures() {
    let module = compile(
        r#"data class Record:
    val value: Int
    fun read() -> Int = value
"#,
    )
    .unwrap();
    let mut foreign = module.clone();
    let definition = foreign.types.remove("test.Record").unwrap();
    foreign.types.insert("other.Record".into(), definition);
    foreign.digest.clear();
    foreign.digest = crate::model::digest(&foreign).unwrap();
    assert!(verify(&mut foreign).unwrap_err().contains("not owned by module"));

    let mut changed = module;
    let definition = std::sync::Arc::make_mut(changed.types.get_mut("test.Record").unwrap());
    std::sync::Arc::make_mut(&mut definition.methods.get_mut("read").unwrap()[0]).result = crate::model::Type::Float;
    changed.digest.clear();
    changed.digest = crate::model::digest(&changed).unwrap();
    assert!(
        verify(&mut changed)
            .unwrap_err()
            .contains("method signature does not match")
    );
}
#[test]
fn constructorless_imports_are_namespaces_and_preserve_generic_variants() {
    let dependency = compile(
        r#"export enum Mood:
    Happy
    Sad
export sealed class Outcome<T>:
    case Success(amount: T)
    case Pending
"#,
    )
    .unwrap();
    let dependencies = BTreeMap::from([("test".into(), &dependency)]);
    let compile_client = |source| {
        compiler::compile(
            parser::parse(source, "src/client.bfl")?,
            "client",
            "src/client.bfl",
            &dependencies,
            Instant::now(),
        )
    };
    compile_client(
        r#"import test.Outcome as Result
scene start():
    val result: Result<Int> = Result.Success(7)
"#,
    )
    .unwrap();
    let error = compile_client(
        r#"import test.Mood
scene start():
    val invalid = Mood
"#,
    )
    .unwrap_err();
    assert!(error.contains("unknown name"), "{error}");
}
#[test]
fn reusable_modules_reject_bad_locations_and_inconsistent_capture_contracts() {
    let module = compile(
        r#"fun factory() -> () -> Int:
    val counter = 1
    fun read() -> Int = counter
    return ::read
"#,
    )
    .unwrap();
    let mut missing_location = module.clone();
    missing_location.functions.values_mut().next().unwrap().code[0]
        .span
        .line = 0;
    missing_location.digest.clear();
    missing_location.digest = crate::model::digest(&missing_location).unwrap();
    assert!(verify(&mut missing_location).unwrap_err().contains("source location"));
    let mut inconsistent = module;
    let function = inconsistent.functions.get_mut("test.factory").unwrap();
    let mut duplicate = function
        .code
        .iter()
        .find(|instruction| matches!(instruction.op, crate::model::Op::Closure { .. }))
        .unwrap()
        .clone();
    if let crate::model::Op::Closure { captures, .. } = &mut duplicate.op {
        *captures = Some(vec![]);
    }
    function.code.push(duplicate);
    inconsistent.digest.clear();
    inconsistent.digest = crate::model::digest(&inconsistent).unwrap();
    assert!(
        verify(&mut inconsistent)
            .unwrap_err()
            .contains("inconsistent named closure")
    );
}

#[test]
fn imported_transparent_enum_aliases_expose_the_owning_declarations_cases() {
    let owner = compile(
        r#"export enum Mood:
    Happy
    Sad
"#,
    )
    .unwrap();
    let alias = compiler::compile(
        parser::parse("import test.Mood\nexport typealias Shade = Mood\n", "src/api.bfl").unwrap(),
        "api",
        "src/api.bfl",
        &BTreeMap::from([("test".into(), &owner)]),
        Instant::now(),
    )
    .unwrap();
    compiler::compile(
        parser::parse(
            r#"import api.Shade as Color
scene start():
    val first: Color = Happy
    val second = Color.Sad
"#,
            "src/client.bfl",
        )
        .unwrap(),
        "client",
        "src/client.bfl",
        &BTreeMap::from([("test".into(), &owner), ("api".into(), &alias)]),
        Instant::now(),
    )
    .unwrap();
}
