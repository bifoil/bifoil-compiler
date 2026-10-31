//! Ordered operand checks must describe values at their actual evaluation points.

use super::compile;

#[test]
fn effectful_left_operands_revoke_right_hand_proofs() {
    for expression in [
        "mutate() + value",
        "mutate() < value",
        "mutate()..value",
        "mutate()..<value",
    ] {
        let source = format!(
            r#"scene start():
    var value: Int? = 1
    fun mutate() -> Int:
        value = null
        return 2
    if value != null:
        val result = {expression}
"#
        );
        let error = compile(&source).unwrap_err();
        assert!(error.contains("Int?"), "{expression}: {error}");
    }
}

#[test]
fn captured_left_values_remain_valid_after_right_effects() {
    compile(
        r#"scene start():
    var value: Int? = 1
    fun mutate() -> Int:
        value = null
        return 2
    if value != null:
        val result: Int = value + mutate()
"#,
    )
    .unwrap();
}

#[test]
fn logical_bottom_rhs_does_not_exempt_evaluated_left_operand() {
    for operator in ["&&", "||"] {
        for left in ["1", "\"text\"", "null"] {
            let source = format!(
                r#"fun check() -> Int:
    val result = {left} {operator} return 2
    return 3
"#
            );
            let error = compile(&source).unwrap_err();
            assert!(error.contains(operator), "{source}: {error}");
        }
        compile(&format!(
            r#"fun check(flag: Boolean) -> Int:
    val result = flag {operator} return 2
    return 3
"#
        ))
        .unwrap();
    }
}

#[test]
fn invalid_constant_fairmath_base_is_independent_of_dynamic_modifier() {
    for operator in ["%+", "%-"] {
        for base in ["101", "(-1)", "100.1"] {
            let error = compile(&format!(
                r#"fun change() -> Int = 1
fun bad() = {base} {operator} change()
"#
            ))
            .unwrap_err();
            assert!(error.contains("constant fairmath base"), "{base} {operator}: {error}");
        }
    }
}

#[test]
fn ordered_arguments_and_binary_inference_keep_normal_completion_facts() {
    for body in [
        "return accept(x!!, x)",
        r#"return accept((if x == null:
        return 0
    else:
        1
    ), x)"#,
        "return x!!.length + x.length",
        "return x!!.substring(0, x.length).length",
    ] {
        compile(&format!(
            r#"fun accept(a: Int, b: String) -> Int = a + b.length
fun accept(a: String, b: String) -> Int = a.length + b.length
fun test(var x: String?) -> Int:
    {body}
"#
        ))
        .unwrap();
    }
}

#[test]
fn collection_searches_require_equatable_elements_but_storage_does_not() {
    for operation in ["values.contains(::cb)", "changes.remove(::cb)", "values::contains"] {
        let error = compile(&format!(
            r#"fun cb() -> Unit:
    return
scene start():
    val values = [::cb]
    val changes = mutable [::cb]
    val result = {operation}
"#
        ))
        .unwrap_err();
        assert!(error.contains("member"), "{operation}: {error}");
    }
    assert!(compile("fun <T> search(values: List<T>, item: T) -> Boolean = values.contains(item)\n").is_err());
    compile(
        r#"fun <T: Equatable> search(values: List<T>, item: T) -> Boolean = values.contains(item)
fun cb() -> Unit:
    return
scene start():
    val values = mutable [::cb]
    val saved = values.removeAt(0)
    values.clear()
"#,
    )
    .unwrap();
}

#[test]
fn unused_generic_bounds_are_semantically_validated() {
    for declaration in [
        "fun <T: Set<MutableList<Int>>> bad(value: T) -> T = value",
        "data class Bad<T: Map<MutableList<Int>, String>>:\n    val value: T",
        "typealias Bad<T: Set<MutableList<Int>>> = List<T>",
        "data class Bad:\n    fun <T: Set<MutableList<Int>>> bad(value: T) -> T = value",
    ] {
        let error = compile(&format!("{declaration}\n")).unwrap_err();
        assert!(error.contains("StableKey"), "{declaration}: {error}");
    }
    for bound in ["Box<Int, Int>", "Box<() -> Int>"] {
        let error = compile(&format!(
            r#"data class Box<T: Equatable>:
    val item: T
fun <T: {bound}> bad(value: T) -> T = value
"#
        ))
        .unwrap_err();
        assert!(
            error.contains("requires 1 arguments") || error.contains("does not satisfy"),
            "{error}"
        );
    }
    compile(
        r#"data class Node<T>:
    val item: T
fun <T: Node<T>> identity(value: T) -> T = value
"#,
    )
    .unwrap();
}

#[test]
fn nonempty_loop_exit_facts_agree_with_annotated_return_checking() {
    for iterable in ["[1]", "1..1", "\"x\"", "{1}", "{1: 1}", "(2 downTo 1) step 2"] {
        for annotation in ["", " -> Int"] {
            compile(&format!(
                r#"fun length(values: Map<Int, String?>){annotation}:
    for ordinal in {iterable}:
        if values[0] == null:
            return 0
    return values[0].length
"#
            ))
            .unwrap();
        }
    }
    assert!(
        compile(
            r#"fun length(values: Map<Int, String?>, items: List<Int>):
    for ordinal in items:
        if values[0] == null:
            return 0
    return values[0].length
"#
        )
        .is_err()
    );
}

#[test]
fn choice_checks_retain_distinct_opening_header_lines() {
    let module = compile(
        r#"scene start():
    var index = 10
    val numbers = [1]
    choice:
        option "First" if numbers[index] == 1:
            Unit
        option "Second ${numbers[index]}":
            Unit
"#,
    )
    .unwrap();
    let lines = module.functions["test.start"]
        .code
        .iter()
        .filter_map(|instruction| matches!(instruction.op, crate::model::Op::Index).then_some(instruction.span.line))
        .collect::<Vec<_>>();
    assert_eq!(lines, [5, 7]);
}

#[test]
fn pre_fix_typed_artifacts_require_regeneration() {
    let mut module = compile("export fun safe() -> Int = 1\n").unwrap();
    module.format = "bifoil.module.v2".into();
    module.digest.clear();
    module.digest = crate::model::digest(&module).unwrap();
    assert!(
        crate::module_validation::verify(&mut module)
            .unwrap_err()
            .contains("recompile")
    );
}

#[test]
fn later_paragraph_and_multiline_label_interpolations_keep_their_physical_lines() {
    let module = compile(
        r#"scene start():
    var index = 10
    val numbers = [1]
    prose:
        First paragraph line.
        Second line ${numbers[index]}.
    choice:
        option """First line
Second ${numbers[index]}""":
            Unit
"#,
    )
    .unwrap();
    let function = &module.functions["test.start"];
    let lines = function
        .code
        .iter()
        .filter_map(|instruction| matches!(instruction.op, crate::model::Op::Index).then_some(instruction.span.line))
        .collect::<Vec<_>>();
    assert_eq!(lines, [6, 9]);
    let restored: crate::model::Module = serde_json::from_str(&serde_json::to_string(&module).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&function.code).unwrap(),
        serde_json::to_value(&restored.functions["test.start"].code).unwrap(),
    );
}

#[test]
fn effect_invalidation_covers_any_alias_fields_nullable_membership_and_generics() {
    let programs = [
        r#"scene start():
    var value: Any = 1
    fun reset() -> Int:
        value = "changed"
        return 2
    if value is Int:
        val result = reset() + value
"#,
        r#"data class Cell:
    var value: Int?
scene start():
    val cell = Cell(1)
    val alias = cell
    fun reset() -> Int:
        alias.value = null
        return 2
    if cell.value != null:
        val result = reset() + cell.value
"#,
        r#"scene start():
    var values: List<Int>? = [1]
    fun reset() -> Int:
        values = null
        return 1
    if values != null:
        val result = reset() in values
"#,
        r#"fun <T: Int> compute(var value: T?) -> Int:
    fun reset() -> Int:
        value = null
        return 2
    if value != null:
        return reset() + value
    return 0
"#,
    ];
    for source in programs {
        let error = compile(source).unwrap_err();
        assert!(error.contains("cannot combine"), "{error}\n{source}");
    }
}

#[test]
fn operand_effects_do_not_retype_previously_captured_generic_values() {
    compile(
        r#"fun <T: Int> compute(var value: T?) -> Int:
    fun reset() -> Int:
        value = null
        return 2
    if value != null:
        return value + reset()
    return 0
"#,
    )
    .unwrap();
}
