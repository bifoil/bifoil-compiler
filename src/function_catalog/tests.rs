use super::*;
use crate::code_store::CodeStore;
use crate::model::Module;

fn module(name: &str, source: &str) -> Module {
    let file = format!("src/{}.bfl", name.replace('.', "/"));
    let ast = crate::parser::parse(source, &file).unwrap();
    crate::compiler::compile(ast, name, &file, &BTreeMap::new(), Instant::now()).unwrap()
}

#[test]
fn nested_module_ids_interleave_without_changing_ownership_or_canonical_bytes() {
    let mut parent = module(
        "a",
        "data class Middle:\n    val value: Int\n    fun read() -> Int = value\nexport fun z() -> Int = 7\n",
    );
    let mut child = module("a.Middle", "export fun adjacent() -> Int = 11\n");
    let shared = parent.functions["a.$type:Middle:read"].signature.clone();
    let mut expected = parent.functions.clone();
    expected.extend(child.functions.clone());
    let expected_bytes = serde_json::to_vec(&expected).unwrap();
    let mut store = CodeStore::default();
    store.record(&mut parent, Instant::now()).unwrap();
    store.record(&mut child, Instant::now()).unwrap();
    let mut catalog = FunctionCatalog::default();
    catalog.insert("a.Middle".into(), child.functions).unwrap();
    catalog.insert("a".into(), parent.functions).unwrap();
    catalog.insert("empty".into(), BTreeMap::new()).unwrap();
    catalog.validate(Instant::now()).unwrap();
    assert_eq!(catalog.len(), expected.len());
    assert_eq!(catalog.values().count(), expected.len());
    assert!(catalog.get("missing").is_none());
    assert!(std::sync::Arc::ptr_eq(
        &shared,
        &catalog.get("a.$type:Middle:read").unwrap().signature
    ));
    assert!(catalog.groups["a"].contains_key("a.$type:Middle:read"));
    assert!(!catalog.groups["a.Middle"].contains_key("a.$type:Middle:read"));
    let mut sorted = catalog.sorted();
    assert_eq!(sorted.heads.len(), 2);
    let mut ids = Vec::new();
    while let Some((id, _)) = sorted.next() {
        ids.push(id);
        assert!(sorted.heads.len() <= 2);
    }
    assert!(
        ids.iter().position(|id| *id == "a.Middle.adjacent").unwrap() < ids.iter().position(|id| *id == "a.z").unwrap()
    );
    assert_eq!(ids, expected.keys().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(serde_json::to_vec(&store.functions(&catalog)).unwrap(), expected_bytes);

    // The grouped serializer must retain the ordinary spool/header checks.
    catalog
        .groups
        .get_mut("a")
        .unwrap()
        .get_mut("a.$type:Middle:read")
        .unwrap()
        .init = true;
    let error = serde_json::to_vec(&store.functions(&catalog)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("stored function header changed: a.$type:Middle:read"),
        "{error}"
    );
}

#[test]
fn duplicate_function_and_module_identities_remain_errors() {
    let functions = module("same", "export fun answer() -> Int = 3\n").functions;
    let mut catalog = FunctionCatalog::default();
    catalog.insert("first".into(), functions.clone()).unwrap();
    assert_eq!(
        catalog.insert("first".into(), BTreeMap::new()).unwrap_err(),
        "duplicate linked module identity first"
    );
    assert_eq!(catalog.len(), functions.len());
    catalog.insert("second".into(), functions).unwrap();
    assert_eq!(
        catalog.validate(Instant::now()).unwrap_err(),
        "duplicate linked function identity same.answer"
    );
}

#[test]
fn publication_freeze_releases_signatures_and_preserves_ordered_records() {
    let mut parent = module("a", "export fun z(value: Int = 2) -> Int = value + 1\n");
    let mut child = module("a.Middle", "export fun adjacent() -> Int = 11\n");
    let expected = {
        let mut functions = parent.functions.clone();
        functions.extend(child.functions.clone());
        serde_json::to_vec(&functions).unwrap()
    };
    let witnesses = parent
        .functions
        .values()
        .chain(child.functions.values())
        .map(|function| std::sync::Arc::downgrade(&function.signature))
        .collect::<Vec<_>>();
    let mut store = CodeStore::default();
    store.record(&mut parent, Instant::now()).unwrap();
    store.record(&mut child, Instant::now()).unwrap();
    let mut catalog = FunctionCatalog::default();
    catalog
        .insert("a".into(), std::mem::take(&mut parent.functions))
        .unwrap();
    catalog
        .insert("a.Middle".into(), std::mem::take(&mut child.functions))
        .unwrap();
    catalog.insert("empty".into(), BTreeMap::new()).unwrap();
    drop(parent);
    drop(child);
    let _cache = store.cache_scope(1024 * 1024).unwrap();
    assert_eq!(serde_json::to_vec(&store.functions(&catalog)).unwrap(), expected);
    assert!(witnesses.iter().all(|signature| signature.upgrade().is_some()));
    let frozen = catalog.freeze(Instant::now()).unwrap();
    assert!(witnesses.iter().all(|signature| signature.upgrade().is_none()));
    frozen.validate(Instant::now()).unwrap();
    assert_eq!(frozen.len(), 2);
    assert!(frozen.groups["a"].contains_key("a.z"));
    assert!(frozen.groups["a.Middle"].contains_key("a.Middle.adjacent"));
    assert!(frozen.groups["empty"].is_empty());
    let mut sorted = frozen.sorted();
    assert_eq!(sorted.heads.len(), 2);
    assert_eq!(sorted.next().unwrap().0, "a.Middle.adjacent");
    assert!(sorted.heads.len() <= 2);
    assert_eq!(sorted.next().unwrap().0, "a.z");
    assert!(sorted.next().is_none());
    assert_eq!(serde_json::to_vec(&store.functions(&frozen)).unwrap(), expected);
    let empty = FunctionCatalog::<Function>::default().freeze(Instant::now()).unwrap();
    assert_eq!(empty.len(), 0);
    assert_eq!(serde_json::to_vec(&store.functions(&empty)).unwrap(), b"{}");
}

#[test]
fn publication_freeze_rejects_unspooled_or_mismatched_headers() {
    let original = module("identity", "export fun answer() -> Int = 3\n");
    let mut catalog = FunctionCatalog::default();
    catalog.insert("identity".into(), original.functions).unwrap();
    assert!(
        catalog
            .freeze(Instant::now())
            .err()
            .unwrap()
            .contains("unspooled function")
    );

    let mut original = module("identity", "export fun answer() -> Int = 3\n");
    let mut store = CodeStore::default();
    store.record(&mut original, Instant::now()).unwrap();
    let function = original.functions.remove("identity.answer").unwrap();
    let mut catalog = FunctionCatalog::default();
    catalog
        .insert("identity".into(), BTreeMap::from([("wrong.key".into(), function)]))
        .unwrap();
    assert!(
        catalog
            .freeze(Instant::now())
            .err()
            .unwrap()
            .contains("stored function header changed")
    );
}
