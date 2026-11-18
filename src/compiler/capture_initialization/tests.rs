//! Optional write proofs must fail conservatively on incomplete or oversized graphs.
use super::*;
use crate::model::{LegacyTarget, Span, Type};

fn instructions(ops: Vec<Op>) -> Vec<Instruction> {
    ops.into_iter()
        .enumerate()
        .map(|(index, op)| Instruction {
            op,
            span: Span {
                file: "src/original.bfl".into(),
                line: index + 1,
            },
        })
        .collect()
}

fn target() -> Target {
    Target {
        reads_after_write: false,
        single_use: false,
        scope: 0,
        identity: super::super::bindings::Identity::new(),
        name: "cell".into(),
        runtime: "cell".into(),
        owner: "original".into(),
    }
}

#[test]
fn incomplete_choice_preparation_never_invents_a_successor() {
    for ops in [
        vec![Op::BeginChoice, Op::BeginChoice],
        vec![Op::Offer { target: 0, once: None }],
        vec![Op::Choose { fallback: None }],
        vec![Op::BeginChoice],
    ] {
        assert!(choice_offers(&instructions(ops), Instant::now()).unwrap().is_none());
    }
    let malformed = instructions(vec![Op::Choose { fallback: None }, Op::Return]);
    assert!(analyze(&malformed, vec![target()], Instant::now()).unwrap().is_empty());
    assert!(analyze(&[], vec![target()], Instant::now()).unwrap().is_empty());
}

#[test]
fn invalid_edges_defaults_and_fact_budgets_leave_the_optional_proof_unavailable() {
    let stores = BTreeMap::from([("cell", 0)]);
    let offers = BTreeMap::new();
    for ops in [
        vec![Op::Jump { target: 10 }],
        vec![Op::Pop],
        vec![
            Op::Default {
                name: "input".into(),
                target: 0,
            },
            Op::Return,
        ],
    ] {
        assert!(
            blocks(&instructions(ops), &stores, 1, &offers, Instant::now())
                .unwrap()
                .is_none()
        );
    }
    assert!(
        blocks(
            &instructions(vec![Op::Return]),
            &stores,
            usize::MAX,
            &offers,
            Instant::now()
        )
        .unwrap()
        .is_none()
    );
    assert!(
        analyze(
            &instructions(vec![Op::Jump { target: 10 }]),
            vec![target()],
            Instant::now()
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn write_proofs_intersect_all_returning_paths() {
    for second_writes in [false, true] {
        let second = if second_writes {
            Op::Store { name: "cell".into() }
        } else {
            Op::Pop
        };
        let code = instructions(vec![
            Op::JumpFalse { target: 3 },
            Op::Store { name: "cell".into() },
            Op::Jump { target: 4 },
            second,
            Op::Return,
        ]);
        assert_eq!(
            analyze(&code, vec![target()], Instant::now()).unwrap().len(),
            usize::from(second_writes)
        );
    }
    let malformed_boolean = instructions(vec![
        Op::Constant {
            ty: Type::Boolean,
            value: serde_json::json!("unknown"),
        },
        Op::JumpFalse { target: 2 },
        Op::Return,
    ]);
    assert_eq!(block_successors(&malformed_boolean, 0, 2, &BTreeMap::new()), vec![2, 2]);
}

#[test]
fn transfer_calls_end_the_captured_write_path_but_returning_calls_continue_it() {
    for transfer in [false, true] {
        let expected = if transfer { vec![] } else { vec![7] };
        let scene = Op::SceneCall {
            names: vec![],
            direct: true,
            transfer,
        };
        let native = Op::LegacyCall {
            target: LegacyTarget {
                transfer,
                scene: "original".into(),
                label: None,
                result: None,
            },
            arguments: 0,
        };
        for op in [scene, native] {
            assert_eq!(successors(&op, 7, &[]), expected);
        }
    }
}
