use super::*;

#[test]
fn canonical_module_views_preserve_bytes_and_return_headers_after_inspection_errors() {
    let mut input = module("views");
    let expected = serde_json::to_vec(&input).unwrap();
    let signature = input.functions["views.answer"].signature.clone();
    let mut store = CodeStore::default();
    store.record_pending(&mut input, Instant::now()).unwrap();
    let (input, serialized) = store.inspect_module(input, |stored| {
        serde_json::to_vec(stored).map_err(|error| error.to_string())
    });
    assert_eq!(serialized.unwrap(), expected);
    assert!(input.functions.values().all(|function| function.code.is_empty()));
    assert!(Arc::ptr_eq(&input.functions["views.answer"].signature, &signature));
    let (input, rejected) = store.inspect_module(input, |_| Err::<(), String>("rejected inspection".into()));
    assert_eq!(rejected.unwrap_err(), "rejected inspection");
    assert!(Arc::ptr_eq(&input.functions["views.answer"].signature, &signature));
    let (_, retried) = store.inspect_module(input, |stored| {
        serde_json::to_vec(stored).map_err(|error| error.to_string())
    });
    assert_eq!(retried.unwrap(), expected);
}

#[test]
fn canonical_module_views_cannot_replace_changed_resident_code_with_stale_stored_bodies() {
    let mut input = module("resident");
    let mut changed = input.clone();
    let function = changed.functions.get_mut("resident.answer").unwrap();
    let constant = function
        .code
        .iter_mut()
        .rev()
        .find_map(|instruction| {
            if let crate::model::Op::Constant { value, .. } = &mut instruction.op {
                Some(value)
            } else {
                None
            }
        })
        .unwrap();
    *constant = serde_json::json!(7);
    let expected = serde_json::to_vec(&changed).unwrap();
    let mut store = CodeStore::default();
    store.record_pending(&mut input, Instant::now()).unwrap();
    let (changed, rejected): (_, Result<(), String>) = store.inspect_module(changed, |_| {
        panic!("unspooled instructions must be rejected before publication")
    });
    assert!(rejected.unwrap_err().contains("stored module still has instructions"));
    assert_eq!(serde_json::to_vec(&changed).unwrap(), expected);
}

#[test]
fn body_inspection_keeps_pending_records_after_success_skips_and_callback_failure() {
    let mut input = module("inspect");
    let expected = serde_json::to_vec(&input).unwrap();
    let mut store = CodeStore::default();
    store.record_pending(&mut input, Instant::now()).unwrap();
    let instructions = store.instructions;
    let mut observed = Vec::new();
    store
        .inspect_functions(
            &input.functions,
            Instant::now(),
            |_| true,
            |id, function| {
                assert!(!function.code.is_empty());
                assert!(input.functions[id].code.is_empty());
                observed.push(id.to_owned());
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(observed, ["inspect.answer"]);
    store
        .inspect_functions(
            &input.functions,
            Instant::now(),
            |_| false,
            |_, _| panic!("unselected body must not reach analysis"),
        )
        .unwrap();
    let error = store
        .inspect_functions(
            &input.functions,
            Instant::now(),
            |_| true,
            |_, _| Err("analysis rejected the body".into()),
        )
        .unwrap_err();
    assert_eq!(error, "analysis rejected the body");
    assert_eq!(store.instructions, instructions);
    assert!(store.entries.contains_key("inspect.answer").unwrap());
    let (_, serialized) = store.inspect_module(input, |stored| {
        serde_json::to_vec(stored).map_err(|error| error.to_string())
    });
    assert_eq!(serialized.unwrap(), expected);
}

#[test]
fn unreadable_stored_bodies_fail_before_analysis_without_losing_pending_identities() {
    let mut input = module("closed");
    let mut store = CodeStore::default();
    store.record_pending(&mut input, Instant::now()).unwrap();
    let instructions = store.instructions;
    store.close_for_test();
    let error = store
        .inspect_functions(
            &input.functions,
            Instant::now(),
            |_| true,
            |_, _| panic!("unreadable body must not reach analysis"),
        )
        .unwrap_err();
    assert!(error.contains("closed"), "{error}");
    assert_eq!(store.instructions, instructions);
    assert!(store.entries.contains_key("closed.answer").unwrap());
}
