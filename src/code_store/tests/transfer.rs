use super::*;

fn mixed_module() -> Module {
    let source = r#"shared val base = 2
export fun read() -> Int = base
export suspend fun paused() -> Int = base
scene start():
    prose:
        Ready.
"#;
    crate::compiler::compile(
        crate::parser::parse(source, "src/mixed.bfl").unwrap(),
        "mixed",
        "src/mixed.bfl",
        &BTreeMap::new(),
        Instant::now(),
    )
    .unwrap()
}

#[test]
fn unneeded_bodies_keep_their_bytes_without_entering_dependency_analysis() {
    let mut module = mixed_module();
    let expected_bytes = serde_json::to_vec(&module).unwrap();
    let expected =
        crate::initialization::summarize(&module.functions, &module.shared, &BTreeMap::new(), Instant::now()).unwrap();
    assert!(expected.contains_key("mixed.read"));
    assert!(!expected.contains_key("mixed.paused"));
    assert!(!expected.contains_key("mixed.start"));
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
                crate::initialization::SummaryContext::requires_body,
                |id, function| {
                    assert!(!function.code.is_empty());
                    let summary = analysis.analyze(function)?.expect("only requested bodies are decoded");
                    observed.insert(id.to_owned(), summary);
                    Ok(())
                },
            )
            .unwrap();
    }
    assert_eq!(observed.keys().collect::<Vec<_>>(), expected.keys().collect::<Vec<_>>());
    for (id, summary) in expected {
        assert_eq!(observed[&id].bindings, summary.bindings);
        assert_eq!(observed[&id].calls, summary.calls);
        assert_eq!(observed[&id].indirect_call, summary.indirect_call);
        assert_eq!(observed[&id].error, summary.error);
    }
    assert_eq!(completed.instructions, expected_count);
    assert_eq!(pending.instructions, 0);
    assert!(pending.entries.is_empty());
    let headers = std::mem::take(&mut module.functions);
    assert_eq!(
        serde_json::to_vec(&module.with_functions(completed.functions(&headers))).unwrap(),
        expected_bytes
    );
}

#[test]
fn legacy_calls_force_typed_collection_even_when_analysis_needs_no_body() {
    let mut module = mixed_module();
    let function = module.functions.get_mut("mixed.paused").unwrap();
    function.code.insert(
        0,
        crate::model::Instruction {
            op: crate::model::Op::LegacyCall {
                target: LegacyTarget {
                    transfer: false,
                    scene: "native".into(),
                    label: Some("answer".into()),
                    result: None,
                },
                arguments: 0,
            },
            span: Span {
                file: "src/mixed.bfl".into(),
                line: 3,
            },
        },
    );
    let expected = serde_json::to_vec(&module).unwrap();
    let mut pending = CodeStore::default();
    pending.record_pending(&mut module, Instant::now()).unwrap();
    assert!(pending.legacy.is_empty());
    let mut completed = CodeStore::default();
    let mut decoded = Vec::new();
    completed
        .transfer_pending(
            &mut pending,
            &module.functions,
            Instant::now(),
            |_| false,
            |id, _| {
                decoded.push(id.to_owned());
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(decoded, ["mixed.paused"]);
    assert_eq!(completed.legacy.len(), 1);
    let reference = &completed.legacy[0];
    assert_eq!(&*reference.function, "mixed.paused");
    assert_eq!(reference.target.scene, "native");
    assert_eq!(reference.target.label.as_deref(), Some("answer"));
    assert_eq!(reference.span.line, 3);
    let headers = std::mem::take(&mut module.functions);
    assert_eq!(
        serde_json::to_vec(&module.with_functions(completed.functions(&headers))).unwrap(),
        expected
    );
}

#[test]
fn skipping_dependency_decode_still_rejects_changed_bytes_and_headers() {
    for damage_bytes in [false, true] {
        let mut module = super::module("guarded");
        let mut pending = CodeStore::default();
        pending.record_pending(&mut module, Instant::now()).unwrap();
        if damage_bytes {
            pending.file.borrow_mut().as_mut().unwrap().set_len(1).unwrap();
        } else {
            Arc::make_mut(&mut module.functions.get_mut("guarded.answer").unwrap().signature).result =
                crate::model::Type::Float;
        }
        let mut completed = CodeStore::default();
        let error = completed
            .transfer_pending(
                &mut pending,
                &module.functions,
                Instant::now(),
                |_| false,
                |_, _| panic!("damaged input must not reach analysis"),
            )
            .unwrap_err();
        assert!(
            error.contains(if damage_bytes {
                "checksum changed"
            } else {
                "header changed"
            }),
            "{error}"
        );
        assert!(completed.entries.is_empty());
        assert!(!pending.entries.is_empty());
    }
}
