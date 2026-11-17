//! Observable rules selected after the original steering questionnaire.

use super::compile;
use crate::model::{Module, Type};

const OUTCOMES: &str = r#"sealed class Outcome:
    case Success(number: Int)
    case Failure(code: Int)
"#;

fn outcome_module(body: &str) -> Module {
    compile(&format!("{OUTCOMES}{body}")).unwrap()
}

#[test]
fn unit_context_discards_early_and_final_values() {
    compile(
        r#"scene start():
    val callback: (Int) -> Unit = (number: Int) =>:
        if number > 0:
            return number * 2
        "done"
    val result: Unit = callback(3)
"#,
    )
    .unwrap();
}

#[test]
fn a_call_parameter_can_supply_the_unit_context() {
    compile(
        r#"fun <T> invoke(value: T, action: (T) -> Unit):
    action(value)
scene start():
    invoke(3, (number: Int) =>:
        return number * 2
    )
"#,
    )
    .unwrap();
}

#[test]
fn explicit_anonymous_unit_annotations_use_the_same_discard_rule() {
    compile(
        r#"scene start():
    val callback = (number: Int) -> Unit =>:
        return number + 1
    val result: Unit = callback(3)
"#,
    )
    .unwrap();
}

#[test]
fn unit_discard_respects_nested_named_and_anonymous_return_boundaries() {
    compile(
        r#"scene start():
    val callback: () -> Unit = () =>:
        fun named() -> String:
            return "nested"
        val anonymous = (number: Int) =>:
            if number > 0:
                return number
            0
        val one: String = named()
        val two: Int = anonymous(3)
        return one + two
    callback()
"#,
    )
    .unwrap();
}

#[test]
fn discarding_a_created_closure_does_not_publish_its_uninitialized_capture() {
    compile(
        r#"fun create() -> () -> Unit:
    var pending: Int
    val callback: () -> Unit = () =>:
        return () => pending
    return callback
scene start():
    create()()
"#,
    )
    .unwrap();
}

#[test]
fn discarding_a_value_does_not_permit_an_uninitialized_read() {
    let error = compile(
        r#"fun create() -> () -> Unit:
    var pending: Int
    val callback: () -> Unit = () =>:
        return pending
    return callback
scene start():
    create()()
"#,
    )
    .unwrap_err();
    assert!(error.contains("initializ"), "{error}");
}

#[test]
fn unit_context_can_discard_null_and_suspending_results() {
    compile(
        r#"suspend fun ask() -> Int:
    choice:
        option "Continue":
            return 3
scene start():
    val noValue: () -> Unit = () =>:
        return null
    val action: suspend () -> Unit = suspend () =>:
        return ask()
    noValue()
    action()
"#,
    )
    .unwrap();
}

#[test]
fn ordinary_unit_context_does_not_enable_suspending_calls() {
    let error = compile(
        r#"suspend fun ask() -> Int:
    choice:
        option "Continue":
            return 3
scene start():
    val action: () -> Unit = () =>:
        return ask()
"#,
    )
    .unwrap_err();
    assert!(error.contains("suspend"), "{error}");
}

#[test]
fn named_unit_functions_still_require_unit_return_values() {
    for source in [
        "fun invalid() -> Unit:\n    return 3\nscene start():\n    Unit\n",
        "scene start():\n    val action: () -> Unit = () =>:\n        fun invalid() -> Unit:\n            return 3\n        return 4\n",
    ] {
        let error = compile(source).unwrap_err();
        assert!(error.contains("Unit") && error.contains("Int"), "{error}");
    }
}

#[test]
fn module_values_require_explicit_shared_storage() {
    for declaration in [
        "val count = 1",
        "var count: Int = 1",
        "export val count = 1",
        "val count: Int",
    ] {
        let error = compile(&format!("{declaration}\nscene start():\n    Unit\n")).unwrap_err();
        assert!(error.contains("module-level values require shared"), "{error}");
    }
    compile(
        r#"shared val title = "Original"
shared var count: Int = 0
fun increment() -> Int:
    count += 1
    return count
scene start():
    val local = increment()
"#,
    )
    .unwrap();
}

#[test]
fn legacy_storage_interfaces_are_not_owned_module_initializers() {
    let module = compile(
        r#"legacy var nativeGold: Int = "gold"
scene start():
    Unit
"#,
    )
    .unwrap();
    assert_eq!(module.legacy_globals.len(), 1);
}

#[test]
fn generated_copy_reserves_only_its_exact_member_name() {
    for member in [
        "val copy: Int",
        "private var copy: Int = 1",
        "fun copy() -> Int = 1",
        "private fun copy(value: Int) -> Int = value",
    ] {
        let error = compile(&format!("data class Record:\n    {member}\nscene start():\n    Unit\n")).unwrap_err();
        assert!(error.contains("copy is generated"), "{error}");
    }
    compile(
        r#"data class Record:
    val Copy: Int
    fun copycat() -> Int = Copy
fun copy(number: Int) -> Int = number
scene start():
    val original = Record(1)
    val duplicate = original.copy(Copy = 2)
    val result = copy(duplicate.copycat())
"#,
    )
    .unwrap();
}

#[test]
fn invariant_map_key_bounds_can_choose_a_reconstructed_nominal_key_type() {
    let module = outcome_module(
        r#"fun <T: Map<Outcome, Int>> preserve(values: T) -> T = values
fun result() = preserve({Outcome.Success(1): 7})
scene start():
    Unit
"#,
    );
    assert_eq!(
        module.functions["test.result"].signature.result,
        Type::named("Map", vec![Type::named("test.Outcome", vec![]), Type::Int])
    );
}

#[test]
fn map_key_inference_composes_nominal_widening_with_contravariant_callbacks() {
    let module = outcome_module(
        r#"fun <K: StableKey> combine(values: Map<K, Int>, seed: K, first: (K) -> Int, second: (K) -> Int) -> Map<K, Int> = values
fun narrow(value: Outcome) -> Int = 1
fun broad(value: Outcome?) -> Int = 2
fun result():
    val seed: Outcome = Outcome.Failure(2)
    return combine({Outcome.Success(1): 7}, seed, ::narrow, ::broad)
scene start():
    Unit
"#,
    );
    assert_eq!(
        module.functions["test.result"].signature.result,
        Type::named("Map", vec![Type::named("test.Outcome", vec![]), Type::Int])
    );
}

#[test]
fn map_key_conversion_keeps_mutable_families_invariant() {
    let error = compile(&format!(
        r#"{OUTCOMES}fun <T: MutableMap<Outcome, Int>> preserve(values: T) -> T = values
scene start():
    val result = preserve(mutable {{Outcome.Success(1): 7}})
"#,
    ))
    .unwrap_err();
    assert!(error.contains("satisfy") || error.contains("convert"), "{error}");
}

#[test]
fn discarded_closure_returns_in_expression_positions_keep_their_own_boundary() {
    compile(
        r#"fun create() -> () -> Unit:
    var pending: Int
    val callback: () -> Unit = () =>:
        val absent: Int? = null
        val unreachable = absent ?: return (() => pending)
    return callback
scene start():
    create()()
"#,
    )
    .unwrap();
}

#[test]
fn discarding_an_invoked_closure_result_does_not_hide_its_read() {
    let error = compile(
        r#"fun create() -> () -> Unit:
    var pending: Int
    val callback: () -> Unit = () =>:
        return (() => pending)()
    return callback
scene start():
    create()()
"#,
    )
    .unwrap_err();
    assert!(error.contains("initializ"), "{error}");
}
