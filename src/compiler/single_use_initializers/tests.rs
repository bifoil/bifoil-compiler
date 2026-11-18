//! Helper admission and compact calls must not bypass ordinary argument or body checks.
use super::*;

fn state() -> (Compiler, FunctionContext) {
    let compiler = Compiler {
        empty_dependencies: Default::default(),
        empty_publications: Default::default(),
        initialized_captures: Default::default(),
        single_use_initializers: Default::default(),
        immutable_iterables: Default::default(),
        functions: BTreeMap::new(),
        file: "src/helper.bfl".into(),
        started: std::time::Instant::now(),
    };
    let context = FunctionContext {
        scopes: vec![Arc::default()],
        instructions: vec![],
        source_file: "src/helper.bfl".into(),
        function_id: "owner".into(),
        return_type: crate::model::Type::Unit,
        discard_return_values: false,
        may_suspend: false,
        loop_stack: vec![],
        next_iterator_id: 0,
        is_reachable: true,
        prose_depth: 0,
        value_requirements: Default::default(),
        value_scopes: vec![],
    };
    (compiler, context)
}

fn plan(protected: &[&str]) -> Plan {
    Plan {
        creator: "owner".into(),
        protected: protected.iter().map(|name| (*name).into()).collect(),
        expansion: None,
        called_before_definition: false,
        forward: None,
    }
}

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

#[test]
fn captured_initializer_calls_reject_other_owners_and_missing_completed_write_proofs() {
    let _session = crate::declarations::AnalysisSession::enter();
    let (mut compiler, mut context) = state();
    compiler
        .uninitialized("target", &crate::model::Type::Int, &false, &mut context, 1)
        .unwrap();
    let candidate = plan(&["target"]);
    candidate.check_uninitialized(&context).unwrap();
    context.function_id = "other".into();
    assert!(
        candidate
            .check_uninitialized(&context)
            .unwrap_err()
            .contains("declaring function")
    );
    context.function_id = "owner".into();
    compiler.single_use_initializers.insert("helper".into(), candidate);
    let before = context.instructions.len();
    assert!(
        compiler
            .check_initializer_call("helper", &context)
            .unwrap_err()
            .contains("missing captured-val write proof")
    );
    assert_eq!(context.instructions.len(), before);
    assert!(!lookup(&context.scopes, "target").unwrap().may_be_initialized);
}

#[test]
fn forward_definition_preparation_preserves_owned_cells_and_rejects_changed_ownership() {
    let _session = crate::declarations::AnalysisSession::enter();
    let (mut compiler, mut context) = state();
    compiler
        .uninitialized("target", &crate::model::Type::Int, &false, &mut context, 1)
        .unwrap();
    let mut candidate = plan(&["target"]);
    candidate.called_before_definition = true;
    for change in ["mutable", "owner"] {
        let mut scopes = context.scopes.clone();
        let binding = super::super::bindings::symbol_mut(&mut scopes, "target").unwrap();
        if change == "mutable" {
            binding.mutable = true;
        } else {
            binding.owner = "other".into();
        }
        assert!(
            candidate
                .prepare_definition(&mut scopes)
                .unwrap_err()
                .contains("changed ownership or mutability")
        );
    }
    let identity = lookup(&context.scopes, "target").unwrap().identity;
    let binding = super::super::bindings::symbol_mut(&mut context.scopes, "target").unwrap();
    binding.initialized = true;
    binding.may_be_initialized = true;
    binding.narrow = Some(crate::model::Type::Int);
    candidate.prepare_definition(&mut context.scopes).unwrap();
    let binding = lookup(&context.scopes, "target").unwrap();
    assert_eq!(binding.identity, identity);
    assert!(!binding.initialized && !binding.may_be_initialized);
    assert!(binding.narrow.is_none());
}

#[test]
fn discovery_never_schedules_unsupported_or_unused_local_helpers() {
    let source = "fun owner():\n    val target: Int\n    fun helper():\n        target = 1\n    helper()\n";
    let body = crate::parser::parse(source, "src/helper.bfl")
        .unwrap()
        .functions
        .remove(0)
        .body;
    assert_eq!(discover(&body, "owner", None, &vec![]).len(), 1);
    for change in ["scene", "export", "legacy", "empty", "unused"] {
        let mut changed = body.clone();
        if change == "unused" {
            changed.pop();
        } else {
            let StatementKind::Function(helper) = &mut changed[1].kind else {
                panic!("helper declaration disappeared");
            };
            match change {
                "scene" => helper.scene = true,
                "export" => helper.exported = true,
                "legacy" => {
                    helper.legacy = Some(crate::model::LegacyTarget {
                        transfer: false,
                        scene: "original".into(),
                        label: None,
                        result: None,
                    })
                }
                "empty" => helper.body.clear(),
                _ => unreachable!(),
            }
        }
        assert!(discover(&changed, "owner", None, &vec![]).is_empty(), "{change}");
    }
}

#[test]
fn a_corrupted_compact_plan_stops_lowering_before_emitting_its_invalid_statement() {
    let (mut compiler, mut context) = state();
    let mut candidate = plan(&[]);
    candidate.expansion = Some(function("1").body.into());
    compiler.single_use_initializers.insert("helper".into(), candidate);
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compiler.expand_initializer_value("helper", &mut context, 1)
    }));
    assert!(failed.is_err());
    assert!(context.instructions.is_empty());
}
