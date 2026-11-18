//! Pattern coverage and guard flow retain ordinary operand and transfer checks.

use super::compile;
use crate::model::{Op, Type};

#[test]
fn negated_membership_cases_keep_the_negated_runtime_operator() {
    let module = compile(
        "fun outside(value: Int):\n    when value:\n        case !in 1..5:\n            return true\n        else:\n            return false\n",
    )
    .unwrap();
    let function = &module.functions["test.outside"];
    assert_eq!(function.signature.result, Type::Boolean);
    assert!(
        function
            .code
            .iter()
            .any(|instruction| { matches!(&instruction.op, Op::Binary { operator } if operator == "!in") })
    );
}

#[test]
fn a_whole_builtin_type_case_is_exhaustive_without_a_nominal_definition() {
    let module =
        compile("fun classify(value: IntRange):\n    when value:\n        case is IntRange:\n            return 1\n")
            .unwrap();
    assert_eq!(module.functions["test.classify"].signature.result, Type::Int);
}

#[test]
fn a_guard_that_returns_on_every_path_does_not_create_a_fallthrough_result() {
    let module = compile(
        "fun complete(flag: Boolean):\n    when:\n        case true if (if flag:\n            return\n        else:\n            return\n        ):\n            Unit\n",
    )
    .unwrap();
    assert_eq!(module.functions["test.complete"].signature.result, Type::Unit);
}

#[test]
fn unrelated_equality_operands_cannot_acquire_a_shared_family_from_capabilities() {
    for source in [
        "fun <T: Equatable> bad(a: Int, b: T) = a == b\n",
        "fun bad(a: List<Int>, b: Set<Int>) = a == b\n",
        "fun bad(a: List<Int>, b: MutableList<String>) = a == b\n",
    ] {
        let error = compile(source).unwrap_err();
        assert!(error.contains("cannot combine"), "{error}\n{source}");
    }
}

#[test]
fn constructed_subjectless_type_cases_are_rejected_before_module_publication() {
    let source = "fun example(value: Int):\n    when value:\n        case is Int:\n            Unit\n        else:\n            Unit\n";
    let mut ast = crate::parser::parse(source, "src/test.bfl").unwrap();
    let crate::syntax::StatementKind::When { subject, .. } = &mut ast.functions[0].body[0].kind else {
        panic!("expected when statement");
    };
    *subject = None;
    let error = crate::compiler::compile(
        ast,
        "test",
        "src/test.bfl",
        &std::collections::BTreeMap::new(),
        std::time::Instant::now(),
    )
    .unwrap_err();
    assert!(error.contains("type pattern requires a subject"), "{error}");
}
