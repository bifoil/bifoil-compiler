//! Incremental joins must preserve every flow fact from an all-path join.

use super::{FlowJoin, ScopeStack, merge_env};
use crate::compiler::bindings::{Binding, Identity};
use crate::compiler::captures::Requirement;
use crate::compiler::field_proofs::Step;
use crate::model::Type;
use std::collections::BTreeMap;
use std::sync::Arc;

fn base() -> ScopeStack {
    let binding = Binding {
        identity: Identity::new(),
        index_key: false,
        global_aliases: None,
        volatile: false,
        implicit_receiver: false,
        escaped: false,
        runtime: "original".into(),
        ty: Type::Any.nullable(),
        mutable: true,
        initialized: false,
        may_be_initialized: false,
        owner: "owner".into(),
        narrow: None,
        field_narrows: None,
        callables: vec![],
        requires: Default::default(),
        callable_requirements: Default::default(),
    };
    vec![Arc::new([("value".into(), binding)].into_iter().collect())]
}

fn path(base: &ScopeStack, bits: usize) -> ScopeStack {
    let mut path = base.clone();
    let binding = Arc::make_mut(&mut path[0]).get_mut("value").unwrap();
    binding.initialized = bits & 1 != 0;
    binding.may_be_initialized = bits & 3 != 0;
    binding.escaped = bits & 2 != 0;
    binding.index_key = bits & 4 != 0;
    binding.narrow = (bits & 8 != 0).then_some(if bits & 4 != 0 { Type::Int } else { Type::String });
    binding.field_narrows = (bits & 8 != 0).then(|| {
        Arc::new(BTreeMap::from([
            (vec![Step::Field("common".into())], Type::String),
            (
                vec![Step::Field("varying".into())],
                if bits & 4 != 0 { Type::Int } else { Type::Float },
            ),
        ]))
    });
    for (flag, name) in [(1, "first"), (2, "second")] {
        if bits & flag != 0 {
            binding.requires.insert(Requirement::Binding(name.into()));
        }
    }
    // Non-flow metadata always comes from the original declaration, including
    // when the first completed path would otherwise supply a different value.
    binding.runtime = "branch-only".into();
    path
}

fn assert_same(actual: &ScopeStack, expected: &ScopeStack) {
    let actual = &actual[0]["value"];
    let expected = &expected[0]["value"];
    assert_eq!(actual.identity, expected.identity);
    assert_eq!(actual.runtime, "original");
    assert_eq!(actual.initialized, expected.initialized);
    assert_eq!(actual.may_be_initialized, expected.may_be_initialized);
    assert_eq!(actual.narrow, expected.narrow);
    assert_eq!(actual.field_narrows, expected.field_narrows);
    assert_eq!(actual.escaped, expected.escaped);
    assert_eq!(actual.index_key, expected.index_key);
    assert_eq!(actual.requires, expected.requires);
}

#[test]
fn incremental_joins_match_all_path_facts_and_release_completed_snapshots() {
    let base = base();
    assert_same(&FlowJoin::default().finish(&base), &base);
    for first in 0..16 {
        for second in 0..16 {
            for third in 0..16 {
                let paths = [path(&base, first), path(&base, second), path(&base, third)];
                let expected = merge_env(&base, &paths);
                let mut join = FlowJoin::default();
                for path in paths {
                    let released = Arc::downgrade(&path[0]);
                    join.add(&base, path);
                    assert!(released.upgrade().is_none(), "completed frame snapshot was retained");
                }
                assert!(!join.is_empty());
                assert_same(&join.finish(&base), &expected);
            }
        }
    }
}
