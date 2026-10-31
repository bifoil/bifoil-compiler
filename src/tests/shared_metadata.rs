//! Shared declaration views must not let nested analysis mutate published metadata.

use super::compile;
use crate::declarations::{AnalysisSession, definition, install, update_method};
use crate::model::Type;
use std::sync::Arc;

#[test]
fn nested_inferred_method_updates_preserve_the_published_and_outer_declarations() {
    let module = compile(
        r#"export data class Record:
    val number: Int
    fun read() = number
"#,
    )
    .unwrap();
    let original = &module.types["test.Record"];
    let _outer = AnalysisSession::enter();
    install(module.types.clone());
    assert!(Arc::ptr_eq(&definition("test.Record").unwrap(), original));
    {
        let _inner = AnalysisSession::enter();
        install(module.types.clone());
        let mut method = original.methods["read"][0].clone();
        Arc::make_mut(&mut method).result = Type::Float;
        update_method(&method);
        assert_eq!(
            definition("test.Record").unwrap().methods["read"][0].result,
            Type::Float
        );
        assert_eq!(module.types["test.Record"].methods["read"][0].result, Type::Int);
    }
    assert!(Arc::ptr_eq(&definition("test.Record").unwrap(), original));
    assert_eq!(definition("test.Record").unwrap().methods["read"][0].result, Type::Int);
}

#[test]
fn modules_without_shared_declarations_keep_summaries_for_their_clients() {
    use crate::model::ModuleView;
    use std::collections::BTreeMap;
    use std::time::Instant;

    fn module(source: &str, name: &str, scope: &ModuleView<'_>) -> crate::model::Module {
        let file = format!("src/{name}.bfl");
        crate::compiler::compile(
            crate::parser::parse(source, &file).unwrap(),
            name,
            &file,
            scope,
            Instant::now(),
        )
        .unwrap()
    }
    let state = module("export shared val value: Int = 7\n", "state", &BTreeMap::new());
    let helpers = module(
        "import state.value\nexport fun read() -> Int = value\n",
        "helpers",
        &BTreeMap::from([("state".into(), &state)]),
    );
    assert!(helpers.shared.is_empty());
    assert!(helpers.summaries["helpers.read"].bindings.contains("state.value"));
    let client = module(
        "import helpers.read\nshared val answer: Int = read()\n",
        "client",
        &BTreeMap::from([("state".into(), &state), ("helpers".into(), &helpers)]),
    );
    assert_eq!(client.shared["client.answer"].dependencies.as_ref(), ["state.value"]);
    assert!(helpers.summaries["helpers.read"].bindings.contains("state.value"));
}
