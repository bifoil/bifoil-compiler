mod transfer;

use super::*;
use std::io::Write;
fn module(name: &str) -> Module {
    crate::compiler::compile(
        crate::parser::parse("export fun answer(value: Int = 2) -> Int = value + 1\n", "src/test.bfl").unwrap(),
        name,
        "src/test.bfl",
        &BTreeMap::new(),
        Instant::now(),
    )
    .unwrap()
}
#[test]
fn code_storage_preserves_canonical_module_bytes_and_allows_later_appends() {
    let mut first = module("first");
    let expected = serde_json::to_vec(&first).unwrap();
    let mut store = CodeStore::default();
    store.record(&mut first, Instant::now()).unwrap();
    assert!(first.functions.values().all(|function| function.code.is_empty()));
    let headers = std::mem::take(&mut first.functions);
    let first = first.with_functions(store.functions(&headers));
    assert_eq!(serde_json::to_vec(&first).unwrap(), expected);
    drop(first);

    let mut second = module("second");
    store.record(&mut second, Instant::now()).unwrap();
    let decoded: Module = serde_json::from_slice(&expected).unwrap();
    assert_eq!(
        serde_json::to_vec(&store.functions(&headers)).unwrap(),
        serde_json::to_vec(&decoded.functions).unwrap()
    );
    assert_eq!(store.read("second.answer").unwrap().signature.id, "second.answer");
    #[cfg(unix)]
    assert!(!store.path.as_ref().unwrap().exists());
}
#[test]
fn pending_components_restore_independently_and_reclaim_the_empty_spool() {
    let mut parent = module("parent");
    let mut child = module("child");
    let expected_parent = serde_json::to_vec(&parent).unwrap();
    let expected_child = serde_json::to_vec(&child).unwrap();
    let mut store = CodeStore::default();
    store.record_pending(&mut parent, Instant::now()).unwrap();
    store.record_pending(&mut child, Instant::now()).unwrap();
    store.restore_pending(&mut child, Instant::now()).unwrap();
    assert_eq!(serde_json::to_vec(&child).unwrap(), expected_child);
    assert!(parent.functions.values().all(|function| function.code.is_empty()));
    assert!(!store.entries.is_empty());
    store.restore_pending(&mut parent, Instant::now()).unwrap();
    assert_eq!(serde_json::to_vec(&parent).unwrap(), expected_parent);
    assert_eq!(store.instructions, 0);
    assert_eq!(store.file.borrow().as_ref().unwrap().metadata().unwrap().len(), 0);
    store.record_pending(&mut parent, Instant::now()).unwrap();
    store.restore_pending(&mut parent, Instant::now()).unwrap();
    assert_eq!(serde_json::to_vec(&parent).unwrap(), expected_parent);
}
#[test]
fn transfer_preserves_bodies_and_dependency_summaries_without_restoring_headers() {
    let source = "shared val base = 2\nexport fun read() -> Int = base\nexport fun answer(value: Int = read()) -> Int = value + base\n";
    let ast = crate::parser::parse(source, "src/streamed.bfl").unwrap();
    let mut module =
        crate::compiler::compile(ast, "streamed", "src/streamed.bfl", &BTreeMap::new(), Instant::now()).unwrap();
    let expected_bytes = serde_json::to_vec(&module).unwrap();
    let expected =
        crate::initialization::summarize(&module.functions, &module.shared, &BTreeMap::new(), Instant::now()).unwrap();
    assert!(expected["streamed.read"].bindings.contains("streamed.base"));
    assert!(expected["streamed.answer"].calls.contains("streamed.read"));
    let expected_count: usize = module.functions.values().map(|function| function.code.len()).sum();
    let mut pending = CodeStore::default();
    pending.record_pending(&mut module, Instant::now()).unwrap();
    let mut completed = CodeStore::default();
    let mut observed = BTreeMap::new();
    {
        let dependencies = BTreeMap::new();
        let analysis = crate::initialization::SummaryContext::new(
            &module.functions,
            &module.shared,
            &dependencies,
            Instant::now(),
        );
        completed
            .transfer_pending(
                &mut pending,
                &module.functions,
                Instant::now(),
                |_| true,
                |id, function| {
                    assert!(module.functions.values().all(|header| header.code.is_empty()));
                    assert!(!function.code.is_empty());
                    if let Some(summary) = analysis.analyze(function)? {
                        observed.insert(id.to_owned(), summary);
                    }
                    Ok(())
                },
            )
            .unwrap();
    }
    assert_eq!(observed.len(), expected.len());
    for (id, expected) in expected {
        let actual = &observed[&id];
        assert_eq!(actual.bindings, expected.bindings);
        assert_eq!(actual.calls, expected.calls);
        assert_eq!(actual.indirect_call, expected.indirect_call);
        assert_eq!(actual.error, expected.error);
    }
    assert_eq!(completed.instructions, expected_count);
    assert_eq!(pending.instructions, 0);
    assert!(pending.entries.is_empty());
    assert_eq!(pending.file.borrow().as_ref().unwrap().metadata().unwrap().len(), 0);
    let headers = std::mem::take(&mut module.functions);
    assert_eq!(
        serde_json::to_vec(&module.with_functions(completed.functions(&headers))).unwrap(),
        expected_bytes
    );
}

#[test]
fn discarding_stale_code_preserves_pending_ancestors_and_allows_recompilation() {
    let mut parent = module("ancestor");
    let mut child = module("stale");
    let expected_parent = serde_json::to_vec(&parent).unwrap();
    let expected_child = serde_json::to_vec(&child).unwrap();
    let mut store = CodeStore::default();
    store.record_pending(&mut parent, Instant::now()).unwrap();
    store.record_pending(&mut child, Instant::now()).unwrap();
    store.discard_pending(&child, Instant::now()).unwrap();
    assert!(!store.entries.contains_key("stale.answer").unwrap());
    assert!(parent.functions.values().all(|function| function.code.is_empty()));
    store.restore_pending(&mut parent, Instant::now()).unwrap();
    assert_eq!(serde_json::to_vec(&parent).unwrap(), expected_parent);
    assert_eq!(store.instructions, 0);
    child = serde_json::from_slice(&expected_child).unwrap();
    store.record_pending(&mut child, Instant::now()).unwrap();
    store.restore_pending(&mut child, Instant::now()).unwrap();
    assert_eq!(serde_json::to_vec(&child).unwrap(), expected_child);
}

#[test]
fn damaged_stored_code_cannot_replace_a_successful_artifact() {
    let mut module = module("broken");
    let mut store = CodeStore::default();
    store.record(&mut module, Instant::now()).unwrap();
    store.file.borrow_mut().as_mut().unwrap().set_len(1).unwrap();
    let path = std::env::temp_dir().join(format!(
        "bifoil-link-test-{}-{}.json",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    struct OwnedOutput(PathBuf);
    impl Drop for OwnedOutput {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let mut previous = OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
    previous.write_all(b"previous valid output").unwrap();
    drop(previous);
    let _owned = OwnedOutput(path.clone());
    let error =
        crate::artifact::write_json(&path, &store.functions(&module.functions), "", "", Instant::now()).unwrap_err();
    assert!(error.contains("stored function broken.answer"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "previous valid output");
}

#[test]
fn canonical_reuse_preserves_float_spelling_and_text() {
    let mut module = module("bytes");
    let function = module.functions.get_mut("bytes.answer").unwrap();
    let span = function.code[0].span.clone();
    for value in [0.1, 1e-7, 1e20, -0.0, f64::MIN_POSITIVE, f64::MAX] {
        function.code.push(crate::model::Instruction {
            op: crate::model::Op::Constant {
                ty: crate::model::Type::Float,
                value: serde_json::Value::from(value),
            },
            span: span.clone(),
        });
    }
    function.code.push(crate::model::Instruction {
        op: crate::model::Op::Constant {
            ty: crate::model::Type::String,
            value: serde_json::Value::from("😀\n\t\\\"".repeat(20000)),
        },
        span,
    });
    let expected = serde_json::to_vec(&module.functions).unwrap();
    let mut store = CodeStore::default();
    store.record(&mut module, Instant::now()).unwrap();
    assert_eq!(
        serde_json::to_vec(&store.functions(&module.functions)).unwrap(),
        expected
    );
}

#[test]
fn canonical_reuse_checks_headers_and_recorded_instruction_counts() {
    let mut module = module("checked");
    let mut store = CodeStore::default();
    store.record(&mut module, Instant::now()).unwrap();
    for field in ["signature", "init", "local"] {
        let mut headers = module.functions.clone();
        let header = headers.get_mut("checked.answer").unwrap();
        match field {
            "signature" => std::sync::Arc::make_mut(&mut header.signature).result = crate::model::Type::String,
            "init" => header.init = !header.init,
            "local" => header.local = !header.local,
            _ => unreachable!(),
        }
        let error = serde_json::to_vec(&store.functions(&headers)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("stored function header changed: checked.answer")
        );
    }
    let mut location = store.entries.get("checked.answer").unwrap().unwrap().into_owned();
    location.instructions += 1;
    store.entries.replace("checked.answer", location).unwrap();
    let error = serde_json::to_vec(&store.functions(&module.functions)).unwrap_err();
    assert!(error.to_string().contains("stored instruction count changed"));
}

#[test]
fn valid_json_corruption_in_a_private_spool_is_fatal() {
    let mut module = module("integrity");
    let mut store = CodeStore::default();
    store.record(&mut module, Instant::now()).unwrap();
    corrupt_stored_function(&store, "integrity.answer");
    let error = serde_json::to_vec(&store.functions(&module.functions)).unwrap_err();
    assert!(error.to_string().contains("temporary data checksum changed"));
}

#[test]
fn canonical_pending_copy_keeps_ancestors_and_reclaims_only_consumed_records() {
    let mut ancestor = module("ancestor");
    let mut leaf = module("leaf");
    let expected = serde_json::to_vec(&leaf).unwrap();
    let mut source = CodeStore::default();
    source.record_pending(&mut ancestor, Instant::now()).unwrap();
    source.record_pending(&mut leaf, Instant::now()).unwrap();
    let mut target = CodeStore::default();
    target
        .copy_pending_from(&mut source, &leaf.functions, Instant::now())
        .unwrap();
    let headers = std::mem::take(&mut leaf.functions);
    assert_eq!(
        serde_json::to_vec(&leaf.with_functions(target.functions(&headers))).unwrap(),
        expected
    );
    assert!(!source.entries.contains_key("leaf.answer").unwrap());
    assert!(source.entries.contains_key("ancestor.answer").unwrap());
    assert!(source.file.borrow().as_ref().unwrap().metadata().unwrap().len() > 0);
    let mut invalid = ancestor.functions.clone();
    invalid.get_mut("ancestor.answer").unwrap().init = true;
    assert!(
        target
            .copy_pending_from(&mut source, &invalid, Instant::now())
            .unwrap_err()
            .contains("header changed")
    );
    assert!(source.entries.contains_key("ancestor.answer").unwrap());
    target
        .copy_pending_from(&mut source, &ancestor.functions, Instant::now())
        .unwrap();
    assert_eq!(source.instructions, 0);
    assert_eq!(source.file.borrow().as_ref().unwrap().metadata().unwrap().len(), 0);
}

fn corrupt_stored_function(store: &CodeStore, id: &str) {
    let location = store.entries.get(id).unwrap().unwrap();
    let mut handle = store.file.borrow_mut();
    let file = handle.as_mut().unwrap();
    file.seek(SeekFrom::Start(location.offset)).unwrap();
    let mut bytes = vec![0; location.length as usize];
    file.read_exact(&mut bytes).unwrap();
    let position = bytes
        .windows(b"answer".len())
        .position(|part| part == b"answer")
        .unwrap();
    bytes[position] = b'A';
    assert!(serde_json::from_slice::<Function>(&bytes).is_ok());
    file.seek(SeekFrom::Start(location.offset)).unwrap();
    file.write_all(&bytes).unwrap();
    drop(handle);
}

#[test]
fn scoped_canonical_cache_shares_checked_values_and_releases_them() {
    let mut module = module("cached");
    let expected = serde_json::to_vec(&module.functions).unwrap();
    let mut store = CodeStore::default();
    store.record(&mut module, Instant::now()).unwrap();
    let weak;
    {
        let _scope = store.cache_scope(1024 * 1024).unwrap();
        let first = store.encoded("cached.answer").unwrap();
        let second = store.encoded("cached.answer").unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        weak = Arc::downgrade(&first);
        assert_eq!(
            serde_json::to_vec(&store.functions(&module.functions)).unwrap(),
            expected
        );
        assert!(store.cache_scope(1024).is_err());
        assert!(Arc::ptr_eq(&first, &store.encoded("cached.answer").unwrap()));
        drop(first);
        drop(second);
        assert!(weak.upgrade().is_some());
    }
    assert!(weak.upgrade().is_none());
    assert!(store.encoded_cache.borrow().is_none());
}

#[test]
fn an_oversized_cache_entry_remains_valid_without_being_retained() {
    let mut module = module("uncached");
    let expected = serde_json::to_vec(&module.functions).unwrap();
    let mut store = CodeStore::default();
    store.record(&mut module, Instant::now()).unwrap();
    let _scope = store.cache_scope(1).unwrap();
    let first = store.encoded("uncached.answer").unwrap();
    let second = store.encoded("uncached.answer").unwrap();
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(
        serde_json::to_vec(&store.functions(&module.functions)).unwrap(),
        expected
    );
}

#[test]
fn cache_hits_do_not_conceal_changed_headers_damaged_bytes_or_closed_storage() {
    let mut module = module("integrity");
    let mut store = CodeStore::default();
    store.record(&mut module, Instant::now()).unwrap();
    let _scope = store.cache_scope(1024 * 1024).unwrap();
    let cached = store.encoded("integrity.answer").unwrap();
    module.functions.get_mut("integrity.answer").unwrap().init = true;
    let error = serde_json::to_vec(&store.functions(&module.functions)).unwrap_err();
    assert!(error.to_string().contains("stored function header changed"));
    module.functions.get_mut("integrity.answer").unwrap().init = false;
    assert!(Arc::ptr_eq(&cached, &store.encoded("integrity.answer").unwrap()));
    corrupt_stored_function(&store, "integrity.answer");
    let error = store.encoded("integrity.answer").err().unwrap();
    assert!(error.contains("temporary data checksum changed"), "{error}");
    store.close_for_test();
    let error = store.encoded("integrity.answer").err().unwrap();
    assert!(error.contains("temporary code store is closed"), "{error}");
}

#[test]
fn paged_indexes_preserve_pending_ancestors_transfers_and_canonical_bytes() {
    let mut ancestor = module("ancestor");
    let mut leaf = module("leaf");
    let ancestor_bytes = serde_json::to_vec(&ancestor).unwrap();
    let leaf_bytes = serde_json::to_vec(&leaf).unwrap();
    let mut source = CodeStore::default();
    source.force_disk_index_for_test();
    source.record_pending(&mut ancestor, Instant::now()).unwrap();
    source.record_pending(&mut leaf, Instant::now()).unwrap();
    let mut target = CodeStore::default();
    target.force_disk_index_for_test();
    target
        .copy_pending_from(&mut source, &leaf.functions, Instant::now())
        .unwrap();
    assert!(source.entries.contains_key("ancestor.answer").unwrap());
    assert!(!source.entries.contains_key("leaf.answer").unwrap());
    let leaf_headers = std::mem::take(&mut leaf.functions);
    assert_eq!(
        serde_json::to_vec(&leaf.with_functions(target.functions(&leaf_headers))).unwrap(),
        leaf_bytes
    );
    source.restore_pending(&mut ancestor, Instant::now()).unwrap();
    assert_eq!(serde_json::to_vec(&ancestor).unwrap(), ancestor_bytes);
    assert!(source.entries.is_empty());
    assert_eq!(source.file.borrow().as_ref().unwrap().metadata().unwrap().len(), 0);
    source.record_pending(&mut ancestor, Instant::now()).unwrap();
    source.restore_pending(&mut ancestor, Instant::now()).unwrap();
    assert_eq!(serde_json::to_vec(&ancestor).unwrap(), ancestor_bytes);
}

#[test]
fn checked_metadata_reads_keep_flags_and_reject_closed_storage_without_decoding_bodies() {
    let mut module = module("metadata");
    let function = module.functions.get_mut("metadata.answer").unwrap();
    function.init = true;
    function.local = true;
    let expected = function.clone();
    assert!(!expected.code.is_empty());
    let mut store = CodeStore::default();
    store.force_disk_index_for_test();
    store.record(&mut module, Instant::now()).unwrap();
    let header = store.header("metadata.answer").unwrap();
    assert_eq!(header.signature, expected.signature);
    assert_eq!(header.init, expected.init);
    assert_eq!(header.local, expected.local);
    let _scope = store.cache_scope(16384).unwrap();
    store.header("metadata.answer").unwrap();
    store.close_for_test();
    assert!(
        store
            .header("metadata.answer")
            .unwrap_err()
            .contains("temporary code store is closed")
    );
}

#[test]
fn canonical_payload_binds_index_metadata_on_both_cached_and_uncached_reads() {
    for limit in [0, 16384] {
        let mut module = module("bound");
        let mut store = CodeStore::default();
        store.force_disk_index_for_test();
        store.record(&mut module, Instant::now()).unwrap();
        let id = "bound.answer";
        let location = store.entries.get(id).unwrap().unwrap().into_owned();
        let mut changed = module.functions[id].clone();
        Arc::make_mut(&mut changed.signature).result = crate::model::Type::String;
        let mut false_header = location.clone();
        false_header.header_checksum = header_checksum(&changed).unwrap();
        let mut false_count = location;
        false_count.instructions += 1;
        let _scope = store.cache_scope(limit).unwrap();
        let original = store.encoded(id).unwrap();
        assert_eq!(Arc::ptr_eq(&original, &store.encoded(id).unwrap()), limit != 0);
        assert!(
            store
                .encoded_checked_at(id, &changed, &false_header)
                .err()
                .unwrap()
                .contains("stored function header changed")
        );
        assert!(
            store
                .encoded_at(id, &false_count)
                .err()
                .unwrap()
                .contains("stored instruction count changed")
        );
        assert_eq!(store.header(id).unwrap().signature, module.functions[id].signature);
    }
}

#[test]
fn canonical_serialization_rejects_a_header_under_another_function_identity() {
    let mut module = module("identity");
    Arc::make_mut(&mut module.functions.get_mut("identity.answer").unwrap().signature).id = "other.answer".into();
    let mut store = CodeStore::default();
    store.record(&mut module, Instant::now()).unwrap();
    assert!(
        serde_json::to_vec(&store.functions(&module.functions))
            .unwrap_err()
            .to_string()
            .contains("stored function header changed: identity.answer")
    );
    assert!(
        store
            .header("identity.answer")
            .unwrap_err()
            .contains("stored function header changed")
    );
}

#[test]
fn immutable_header_witnesses_do_not_retain_data_or_hide_copy_on_write_and_flag_changes() {
    let mut module = module("witness");
    let mut header = module.functions["witness.answer"].clone();
    header.signature = Arc::new((*header.signature).clone());
    let mut store = CodeStore::default();
    store.record(&mut module, Instant::now()).unwrap();
    let _scope = store.cache_scope(16384).unwrap();
    let first = store.encoded_checked("witness.answer", &header).unwrap();
    assert_eq!(Arc::strong_count(&header.signature), 1);
    assert!(Arc::ptr_eq(
        &first,
        &store.encoded_checked("witness.answer", &header).unwrap()
    ));
    for field in ["init", "local"] {
        let mut changed = header.clone();
        if field == "init" {
            changed.init = !changed.init;
        } else {
            changed.local = !changed.local;
        }
        assert!(
            store
                .encoded_checked("witness.answer", &changed)
                .err()
                .unwrap()
                .contains("stored function header changed")
        );
    }
    let previous = Arc::downgrade(&header.signature);
    Arc::make_mut(&mut header.signature).result = crate::model::Type::String;
    assert!(previous.upgrade().is_none());
    assert!(
        store
            .encoded_checked("witness.answer", &header)
            .err()
            .unwrap()
            .contains("stored function header changed")
    );
    assert_eq!(
        store.header("witness.answer").unwrap().signature,
        module.functions["witness.answer"].signature
    );
}

#[test]
fn frozen_publication_keeps_cached_integrity_checks() {
    use crate::function_catalog::{FunctionCatalog, HeaderDigest};
    let mut module = module("integrity");
    let expected = serde_json::to_vec(&module.functions).unwrap();
    let mut store = CodeStore::default();
    store.record(&mut module, Instant::now()).unwrap();
    let mut catalog = FunctionCatalog::default();
    catalog.insert("integrity".into(), module.functions).unwrap();
    let frozen = catalog.freeze(Instant::now()).unwrap();
    let _cache = encoded_cache::Scope::enter(&store.encoded_cache, 1024 * 1024).unwrap();
    assert_eq!(serde_json::to_vec(&store.functions(&frozen)).unwrap(), expected);

    let invalid = BTreeMap::from([("integrity.answer".into(), HeaderDigest([0; 32]))]);
    let error = serde_json::to_vec(&store.functions(&invalid)).unwrap_err();
    assert!(error.to_string().contains("stored function header changed"), "{error}");
    let original = store.entries.get("integrity.answer").unwrap().unwrap().into_owned();
    let mut changed = original.clone();
    changed.instructions += 1;
    store.entries.replace("integrity.answer", changed).unwrap();
    let error = serde_json::to_vec(&store.functions(&frozen)).unwrap_err();
    assert!(error.to_string().contains("instruction count changed"), "{error}");
    store.entries.replace("integrity.answer", original).unwrap();
    corrupt_stored_function(&store, "integrity.answer");
    let error = serde_json::to_vec(&store.functions(&frozen)).unwrap_err();
    assert!(error.to_string().contains("temporary data checksum changed"), "{error}");
    store.close_for_test();
    let error = serde_json::to_vec(&store.functions(&frozen)).unwrap_err();
    assert!(error.to_string().contains("temporary code store is closed"), "{error}");
}
