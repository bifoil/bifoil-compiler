//! Published artifacts keep canonical identity across streaming and field order.

use super::*;
use crate::module_input::{check, tests::module};
use sha2::Digest;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Input(PathBuf);
impl Input {
    fn new(text: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "bifoil-streamed-test-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, text).unwrap();
        Self(path)
    }
    fn read(&self) -> Result<StreamedModule, String> {
        read(&self.0, crate::model::MAX_ARTIFACT, None, Instant::now()).map(|(module, _)| module)
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn streamed_bodies_preserve_canonical_bytes_after_the_input_is_removed() {
    let original = module();
    let expected = serde_json::to_vec(&original).unwrap();
    let mut value = serde_json::to_value(&original).unwrap();
    let fields = value.as_object_mut().unwrap();
    let functions = fields.remove("functions").unwrap();
    // A consumer cannot depend on metadata preceding code or on compact JSON.
    let rest = fields
        .iter()
        .rev()
        .map(|(name, value)| {
            format!(
                "{}: {}",
                serde_json::to_string(name).unwrap(),
                serde_json::to_string_pretty(value).unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let input = Input::new(&format!(
        "{{\"functions\": {},\n{rest}}}",
        serde_json::to_string_pretty(&functions).unwrap()
    ));
    let bytes = fs::read(&input.0).unwrap();
    let expected_checksum = sha2::Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let (loaded, observed) = read(&input.0, crate::model::MAX_ARTIFACT, None, Instant::now()).unwrap();
    assert_eq!(observed, expected_checksum);
    let first = &loaded.functions.values().next().unwrap().code.store;
    assert_eq!(Rc::strong_count(first), loaded.functions.len());
    assert!(loaded.functions.values().all(|function| function.code.count > 0));
    fs::remove_file(&input.0).unwrap();
    assert_eq!(serde_json::to_vec(&loaded).unwrap(), expected);
    let checked = check(loaded).unwrap();
    let mut pending = CodeStore::default();
    let mut headers = checked.store(&mut pending, Instant::now()).unwrap();
    assert!(headers.functions.values().all(|function| function.code.is_empty()));
    let functions = std::mem::take(&mut headers.functions);
    let restored = headers.with_functions(pending.functions(&functions));
    assert_eq!(serde_json::to_vec(&restored).unwrap(), expected);
}

#[test]
fn streamed_input_rejects_duplicates_missing_fields_trailing_data_and_invalid_bodies() {
    let mut original = module();
    let valid = serde_json::to_string(&original).unwrap();
    for (text, diagnostic) in [
        (format!("{valid} false"), "trailing characters"),
        (
            valid.replacen("\"functions\":{", "\"functions\":{},\"functions\":{", 1),
            "duplicate module field",
        ),
        ("{}".to_owned(), "missing field"),
    ] {
        assert!(Input::new(&text).read().err().unwrap().contains(diagnostic));
    }
    let mut value = serde_json::to_value(&original).unwrap();
    let functions = value["functions"].as_object_mut().unwrap();
    let (id, function) = functions.iter().next().unwrap();
    let entry = format!("{}:{function}", serde_json::to_string(id).unwrap());
    let duplicate = ["{\"functions\":{", &entry, ",", &entry, "}}"].concat();
    assert!(
        Input::new(&duplicate)
            .read()
            .err()
            .unwrap()
            .contains("duplicate function")
    );

    original.functions.values_mut().next().unwrap().code[0].span.line = 0;
    original.digest.clear();
    original.digest = crate::artifact::digest(&original).unwrap();
    let loaded = Input::new(&serde_json::to_string(&original).unwrap()).read().unwrap();
    assert!(
        check(loaded)
            .err()
            .unwrap()
            .contains("invalid instruction source location")
    );
}

#[test]
fn a_failed_parse_drops_the_private_spool_after_completed_function_entries() {
    let original = module();
    let (id, function) = original.functions.iter().next().unwrap();
    let text = format!(
        "{{\"functions\":{{{}:{},\"broken\":{{}}}}}}",
        serde_json::to_string(id).unwrap(),
        serde_json::to_string(function).unwrap()
    );
    let seed = Seed::new(Instant::now());
    let weak = Rc::downgrade(&seed.store);
    let mut deserializer = serde_json::Deserializer::from_str(&text);
    assert!(seed.deserialize(&mut deserializer).is_err());
    assert!(
        weak.upgrade().is_none(),
        "failed input must release its file and function handles"
    );
}

#[test]
fn streamed_shared_dependencies_reuse_storage_and_preserve_canonical_bytes() {
    use std::sync::Arc;
    let source = r#"shared val base: Int = 7
fun read() -> Int = base
shared val first: Int = read()
shared val second: Int = read()
"#;
    let original = crate::compiler::compile(
        crate::parser::parse(source, "src/helpers.bfl").unwrap(),
        "helpers",
        "src/helpers.bfl",
        &BTreeMap::new(),
        Instant::now(),
    )
    .unwrap();
    assert!(Arc::ptr_eq(
        &original.shared["helpers.first"].dependencies,
        &original.shared["helpers.second"].dependencies
    ));
    assert_eq!(original.shared["helpers.first"].dependencies.as_ref(), ["helpers.base"]);
    let expected = serde_json::to_vec(&original).unwrap();
    let loaded = Input::new(std::str::from_utf8(&expected).unwrap()).read().unwrap();
    assert!(Arc::ptr_eq(
        &loaded.shared["helpers.first"].dependencies,
        &loaded.shared["helpers.second"].dependencies
    ));
    assert_eq!(serde_json::to_vec(&loaded).unwrap(), expected);
    check(loaded).unwrap();
}

#[test]
fn duplicate_shared_metadata_is_rejected_after_private_function_spooling() {
    let original = module();
    let (id, definition) = original.shared.iter().next().unwrap();
    let entry = format!(
        "{}:{}",
        serde_json::to_string(id).unwrap(),
        serde_json::to_string(definition).unwrap()
    );
    let text = format!(
        "{{\"functions\":{},\"shared\":{{{entry},{entry}}}}}",
        serde_json::to_string(&original.functions).unwrap()
    );
    let seed = Seed::new(Instant::now());
    let weak = Rc::downgrade(&seed.store);
    let mut deserializer = serde_json::Deserializer::from_str(&text);
    let error = seed.deserialize(&mut deserializer).err().unwrap();
    assert!(error.to_string().contains("duplicate shared binding"));
    assert!(
        weak.upgrade().is_none(),
        "failed metadata must release privately spooled bodies"
    );
}

#[test]
fn canonical_serialization_retains_the_fatal_private_spool_channel() {
    let original = module();
    let text = serde_json::to_string(&original).unwrap();
    let seed = Seed::new(Instant::now());
    let loaded = seed
        .clone()
        .deserialize(&mut serde_json::Deserializer::from_str(&text))
        .unwrap();
    seed.store.borrow().close_for_test();
    let error = serde_json::to_vec(&loaded).unwrap_err();
    assert!(error.to_string().contains("temporary code store is closed"));
    assert!(seed.take_failure().unwrap().contains("temporary code store is closed"));
    assert!(seed.take_failure().is_none());
}

#[test]
fn structural_facts_do_not_bypass_private_spool_verification() {
    let original = module();
    let text = serde_json::to_string(&original).unwrap();
    let seed = Seed::new(Instant::now());
    let loaded = seed
        .clone()
        .deserialize(&mut serde_json::Deserializer::from_str(&text))
        .unwrap();
    // Facts are in memory, but validating the module must still read its
    // checksum-checked canonical bodies and preserve the fatal failure channel.
    seed.store.borrow().close_for_test();
    let error = crate::module_input::check(loaded).err().unwrap();
    assert!(error.contains("temporary code store is closed"), "{error}");
    assert!(seed.take_failure().unwrap().contains("temporary code store is closed"));
    assert!(seed.take_failure().is_none());
}

#[cfg(target_os = "linux")]
#[test]
fn a_late_parse_error_cannot_conceal_a_buffered_write_failure() {
    let original = module();
    let (id, function) = original.functions.iter().next().unwrap();
    let body = serde_json::to_string(function).unwrap();
    assert!(body.len() < 65536);
    let text = format!(
        "{{\"functions\":{{{}:{body},\"broken\":{{}}}}}}",
        serde_json::to_string(id).unwrap()
    );
    let seed = Seed::new(Instant::now());
    seed.store.borrow_mut().fail_writes_for_test().unwrap();
    let error = seed
        .clone()
        .deserialize(&mut serde_json::Deserializer::from_str(&text))
        .err()
        .unwrap();
    assert!(error.to_string().contains("No space left on device"), "{error}");
    assert!(seed.take_failure().unwrap().contains("No space left on device"));
    assert!(seed.take_failure().is_none());
}

#[test]
fn paged_index_damage_retains_the_fatal_private_storage_channel() {
    let original = module();
    let text = serde_json::to_string(&original).unwrap();
    let seed = Seed::new(Instant::now());
    seed.store.borrow_mut().force_disk_index_for_test();
    let loaded = seed
        .clone()
        .deserialize(&mut serde_json::Deserializer::from_str(&text))
        .unwrap();
    let id = loaded.functions.keys().next().unwrap();
    seed.store.borrow_mut().damage_index_for_test(id).unwrap();
    let error = crate::module_input::check(loaded).err().unwrap();
    assert!(error.contains("temporary record index: invalid row size"), "{error}");
    assert!(
        seed.take_failure()
            .unwrap()
            .contains("temporary record index: invalid row size")
    );
    assert!(seed.take_failure().is_none());
}
