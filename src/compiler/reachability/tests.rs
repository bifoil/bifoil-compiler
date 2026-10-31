use super::prune;
use crate::model::{Instruction, Op, Span, Type, unit};
use std::collections::BTreeMap;
use std::time::Instant;

fn instructions(ops: Vec<Op>) -> Vec<Instruction> {
    ops.into_iter()
        .enumerate()
        .map(|(pc, op)| Instruction {
            op,
            span: Span {
                file: "src/pruning.bfl".into(),
                line: pc + 1,
            },
        })
        .collect()
}

#[test]
fn removes_straight_line_code_after_returns_and_traps() {
    for terminal in [Op::Return, Op::Unreachable] {
        let mut code = instructions(vec![unit(), terminal, unit(), Op::Return]);
        prune(&mut code, Instant::now()).unwrap();
        assert_eq!(code.len(), 2);
        assert_eq!(code[1].span.line, 2);
    }
}

#[test]
fn remaps_immediate_and_deferred_edges_without_changing_opaque_ids() {
    // Span lines identify original addresses independently of the compaction.
    // Dead slots occur before defaults, branches, offers, and a backward loop.
    let mut code = instructions(vec![
        Op::Default {
            name: "value".into(),
            target: 4,
        },
        unit(),
        Op::Return,
        unit(),
        Op::JumpFalse { target: 8 },
        Op::JumpTrue { target: 8 },
        Op::JumpNull {
            target: 8,
            when_null: true,
        },
        Op::Jump { target: 10 },
        unit(),
        Op::Return,
        Op::BeginChoice,
        Op::Offer {
            target: 16,
            once: Some("option:99".into()),
        },
        Op::Choose { fallback: Some(20) },
        Op::Jump { target: 24 },
        unit(),
        Op::Return,
        Op::IterStart { id: 99 },
        Op::IterNext {
            id: 99,
            name: "item".into(),
            ty: Type::Int,
            target: 22,
        },
        Op::Jump { target: 17 },
        Op::Return,
        unit(),
        Op::Return,
        Op::IterEnd { id: 99 },
        Op::Return,
        Op::StackMark { id: 99 },
        Op::StackReset { id: 99 },
        Op::StackEnd { id: 99 },
        Op::Return,
        unit(),
    ]);
    prune(&mut code, Instant::now()).unwrap();
    assert_eq!(
        code.iter().map(|i| i.span.line).collect::<Vec<_>>(),
        [
            1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 17, 18, 19, 21, 22, 23, 24, 25, 26, 27, 28
        ]
    );
    let targets: Vec<_> = code.iter().filter_map(|i| i.op.instruction_target()).collect();
    assert_eq!(targets, [3, 7, 7, 7, 9, 13, 16, 20, 18, 14]);
    assert!(matches!(&code[10].op, Op::Offer { once: Some(site), .. } if site == "option:99"));
    for pc in [13, 14, 18, 20, 21, 22] {
        assert!(matches!(
            code[pc].op,
            Op::IterStart { id: 99 }
                | Op::IterNext { id: 99, .. }
                | Op::IterEnd { id: 99 }
                | Op::StackMark { id: 99 }
                | Op::StackReset { id: 99 }
                | Op::StackEnd { id: 99 }
        ));
    }
    let once = serde_json::to_vec(&code).unwrap();
    prune(&mut code, Instant::now()).unwrap();
    assert_eq!(serde_json::to_vec(&code).unwrap(), once);
}

#[test]
fn rejects_invalid_targets_before_compacting_any_code() {
    let mut code = instructions(vec![Op::Return, Op::Jump { target: 2 }]);
    let before = serde_json::to_vec(&code).unwrap();
    assert!(
        prune(&mut code, Instant::now())
            .unwrap_err()
            .contains("invalid instruction target")
    );
    assert_eq!(serde_json::to_vec(&code).unwrap(), before);
}

fn compile(source: &str) -> Result<crate::model::Module, String> {
    crate::compiler::compile(
        crate::parser::parse(source, "src/pruning.bfl")?,
        "pruning",
        "src/pruning.bfl",
        &BTreeMap::new(),
        Instant::now(),
    )
}

#[test]
fn source_checks_still_reject_invalid_unreachable_statements() {
    let error = compile("fun invalid() -> Int:\n    return 7\n    val bad: Int = true\n").unwrap_err();
    assert!(error.contains("expected Int, found Boolean"), "{error}");
}

#[test]
fn prunes_after_lowering_hoisted_local_functions_and_generated_bodies() {
    let mut module = compile(
        r#"data class Box:
    val value: Int
shared val initial: Box = Box(7)
fun immediate() -> Int = 7
fun enclosing() -> Int:
    return later()
    fun later() -> Int = initial.value
scene start():
    val read: () -> Int = () => 7
    val copied = initial.copy()
    prose:
        ${enclosing()}|${read()}|${copied.value}
"#,
    )
    .unwrap();
    assert_eq!(module.functions["pruning.immediate"].code.len(), 2);
    assert!(module.functions.values().any(|function| function.local));
    // All callable kinds are finalized before pruning, including generated copy
    // helpers and closures whose declaration follows an early source return.
    for function in module.functions.values() {
        assert!(matches!(function.code.last().unwrap().op, Op::Return | Op::Unreachable));
        assert!(!function.code.windows(2).any(|pair| matches!(pair[0].op, Op::Return)
            && pair[1].op.instruction_target().is_none()
            && matches!(pair[1].op, Op::Constant { ty: Type::Unit, .. })));
    }
    crate::module_validation::verify(&mut module).unwrap();
}
