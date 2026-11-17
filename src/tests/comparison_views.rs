//! Required operand views for the v3 exact-comparison instruction.
use super::compile;
use crate::model::{Op, Type};

#[test]
fn exact_comparison_retains_declared_views_and_negation_separately() {
    let module = compile(
        r#"data class Box<out T>:
    val number: Int
scene start():
    val original = Box<Int>(3)
    val viewed: Box<Any> = original
    val constructed = Box<Any>(3)
    val same = viewed === constructed
    val different = original !== constructed
"#,
    )
    .unwrap();
    assert_eq!(module.profile, "bifoil-vm.v3");
    let comparisons = module.functions["test.start"]
        .code
        .iter()
        .filter_map(|instruction| {
            if let Op::ExactCompare {
                left_type,
                right_type,
                negate,
            } = &instruction.op
            {
                Some((left_type, right_type, *negate))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let wide = Type::named("test.Box", vec![Type::Any]);
    let narrow = Type::named("test.Box", vec![Type::Int]);
    assert_eq!(comparisons, vec![(&wide, &wide, false), (&narrow, &wide, true)]);
    assert!(!module.functions["test.start"].code.iter().any(
        |instruction| matches!(&instruction.op, Op::Binary { operator } if operator == "===" || operator == "!==")
    ));
}

#[test]
fn the_left_view_is_captured_before_the_right_operand_changes_flow_facts() {
    let module = compile(
        r#"scene start():
    var number: Int? = 3
    fun clear() -> Int:
        number = null
        return 3
    if number != null:
        val same = number === clear()
"#,
    )
    .unwrap();
    assert!(module.functions["test.start"].code.iter().any(|instruction| matches!(
        &instruction.op,
        Op::ExactCompare {
            left_type: Type::Int,
            right_type: Type::Int,
            negate: false
        }
    )));
}

#[test]
fn generic_comparison_views_remain_reifiable_parameters() {
    let module = compile("fun <T: Equatable> same(first: T, second: T) -> Boolean = first === second\n").unwrap();
    let function = &module.functions["test.same"];
    let parameter = &function.signature.generics[0].name;
    assert!(
        function.code.iter().any(|instruction| matches!(
            &instruction.op,
            Op::ExactCompare {
                left_type: Type::Parameter { name: left, .. },
                right_type: Type::Parameter { name: right, .. },
                negate: false,
            } if left == parameter && right == parameter
        )),
        "{:?}",
        function.code
    );
}
