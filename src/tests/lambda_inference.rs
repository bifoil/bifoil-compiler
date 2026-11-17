//! Higher-order lambdas reuse callable scopes, flow and contextual return checks.

use super::compile;
use crate::model::{Module, Type};

const CALLBACKS: &str = r#"fun <T> pair(left: T, right: T) -> T = left
fun chooseFirst(value: Int) -> Int = value
fun chooseFirst(value: String) -> String = value
fun chooseSecond(value: Int) -> Int = value
fun chooseSecond(value: Boolean) -> Boolean = value
"#;

fn with_callbacks(source: &str) -> Result<Module, String> {
    compile(&format!("{CALLBACKS}{source}"))
}

#[test]
fn expression_factories_infer_their_unique_overloaded_result() {
    let module = with_callbacks(
        r#"fun inferred() = pair(() => ::chooseFirst, () => ::chooseSecond)
scene start():
    val factory = inferred()
    val result: Int = factory()(3)
"#,
    )
    .unwrap();
    let Type::Function { params, result, .. } = &module.functions["test.inferred"].signature.result else {
        panic!("factory");
    };
    assert!(params.is_empty());
    let Type::Function { params, result, .. } = result.as_ref() else {
        panic!("returned callback");
    };
    assert_eq!(params.as_ref(), &[Type::Int]);
    assert_eq!(result.as_ref(), &Type::Int);
}

#[test]
fn block_factories_include_early_returns_loops_and_final_values() {
    with_callbacks(
        r#"scene start():
    val factory = pair(
        (flag: Boolean) =>:
            if flag:
                return ::chooseFirst
            for index in 1..2:
                if index == 2:
                    return ::chooseFirst
            ::chooseFirst
        ,
        (flag: Boolean) =>:
            while flag:
                break
            ::chooseSecond
    )
    val result: Int = factory(false)(3)
"#,
    )
    .unwrap();
}

#[test]
fn value_blocks_and_guarded_when_results_keep_branch_scopes() {
    with_callbacks(
        r#"scene start():
    val callback = pair(if true:
        val marker = 1
        when marker:
            case 1 if marker > 0:
                ::chooseFirst
            else:
                ::chooseFirst
    else:
        ::chooseFirst
    , if false:
        ::chooseSecond
    else:
        ::chooseSecond
    )
    val result: Int = callback(3)
"#,
    )
    .unwrap();
}

#[test]
fn typed_mutable_and_variadic_parameters_keep_their_calling_shapes() {
    with_callbacks(
        r#"scene start():
    val mutableFactory = pair((var flag: Boolean) =>:
        flag = !flag
        ::chooseFirst
    , (var flag: Boolean) =>:
        flag = !flag
        ::chooseSecond
    )
    val variadicFactory = pair((vararg values: Int) =>:
        if values.size > 0:
            return ::chooseFirst
        ::chooseFirst
    , (vararg values: Int) => ::chooseSecond)
    val one: Int = mutableFactory(false)(1)
    val two: Int = variadicFactory(1, 2)(2)
"#,
    )
    .unwrap();
}

#[test]
fn whole_collection_literals_propagate_factory_result_alternatives() {
    with_callbacks(
        r#"scene start():
    val factories = pair([() => ::chooseFirst], [() => ::chooseSecond])
    val result: Int = factories[0]()(3)
"#,
    )
    .unwrap();
}

#[test]
fn nested_anonymous_factories_share_the_same_inference() {
    with_callbacks(
        r#"scene start():
    val factory = pair(() => () => ::chooseFirst, () => () => ::chooseSecond)
    val result: Int = factory()()(3)
"#,
    )
    .unwrap();
}

#[test]
fn partial_result_shapes_infer_a_returned_generic_reference() {
    compile(
        r#"fun <T> extract(left: () -> (Int) -> T, right: () -> (Int) -> T) -> T = left()(1)
fun <U> identity(value: U) -> U = value
fun <U> identity(value: U, extra: U) -> U = value
fun choose(value: Int) -> Int = value
fun choose(value: Boolean) -> Boolean = value
scene start():
    val result = extract(() => ::identity, () => ::choose)
"#,
    )
    .unwrap();
}

#[test]
fn multiple_complete_factory_instantiations_remain_ambiguous() {
    let error = compile(
        r#"fun <T> pair(left: T, right: T) -> T = left
fun chooseFirst(value: Int) -> Int = value
fun chooseFirst(value: String) -> String = value
fun chooseSecond(value: Int) -> Int = value
fun chooseSecond(value: String) -> String = value
scene start():
    val factory = pair(() => ::chooseFirst, () => ::chooseSecond)
"#,
    )
    .expect_err("both Int and String callbacks satisfy the whole call");
    assert!(error.contains("ambiguous"), "{error}");
}

#[test]
fn explicit_factory_types_do_not_create_signature_adapters() {
    assert!(
        with_callbacks(
            r#"scene start():
    val factory = pair<() -> (Float) -> Float>(() => ::chooseFirst, () => ::chooseSecond)
"#
        )
        .is_err()
    );
}

#[test]
fn factory_creation_does_not_freeze_a_captured_var_guard() {
    assert!(
        with_callbacks(
            r#"scene start():
    var name: String? = "ready"
    if name != null:
        val factory = pair(() => name.length, () => 1)
"#
        )
        .is_err()
    );
}

#[test]
fn factory_parameter_names_cannot_shadow_visible_bindings() {
    assert!(
        with_callbacks(
            r#"scene start():
    val factory = pair((chooseFirst: Int) => ::chooseSecond, (chooseFirst: Int) => ::chooseSecond)
"#
        )
        .is_err()
    );
}

#[test]
fn contextual_unit_discard_does_not_adapt_existing_functions() {
    compile(
        r#"fun consume(action: () -> Unit):
    action()
scene start():
    consume(() => 3)
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"fun consume(action: () -> Unit):
    action()
fun number() -> Int = 3
scene start():
    consume(::number)
"#
        )
        .is_err()
    );
}

#[test]
fn deferred_flow_typing_does_not_accept_an_invalid_unused_body() {
    assert!(
        compile(
            r#"fun unused():
    val invalid: () -> Unit = () =>:
        "wrong" - 1
        Unit
scene start():
    Unit
"#
        )
        .is_err()
    );
}

#[test]
fn suspended_factory_choices_keep_the_explicit_effect() {
    with_callbacks(
        r#"scene start():
    val factory = pair(suspend () =>:
        choice:
            option "First":
                return ::chooseFirst
            option "Other":
                return ::chooseFirst
    , suspend () =>:
        choice:
            option "Second":
                return ::chooseSecond
    )
    val result: Int = factory()(3)
"#,
    )
    .unwrap();
}

#[test]
fn generated_fallthrough_is_checked_only_on_reachable_paths() {
    compile(
        r#"scene start():
    val complete: suspend (Boolean) -> Int = suspend (flag: Boolean) =>:
        if flag:
            choice:
                option "One":
                    return 1
        else:
            choice:
                option "Two":
                    return 2
    val result = complete(true)
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"scene start():
    val incomplete: (Boolean) -> Int = (flag: Boolean) =>:
        if flag:
            return 1
"#
        )
        .is_err()
    );
}

#[test]
fn authored_unreachable_returns_retain_their_type_checks() {
    assert!(
        compile(
            r#"scene start():
    val callback: () -> Int = () =>:
        return 1
        return
"#
        )
        .is_err()
    );
}

#[test]
fn coverage_contextual_factories_preserve_explicit_results_and_container_keys() {
    for (left, right, suffix) in [
        ("[() => ::chooseFirst]", "[() => ::chooseSecond]", "[0]()(3)"),
        (
            "mutable [() => ::chooseFirst]",
            "mutable [() => ::chooseSecond]",
            "[0]()(3)",
        ),
        (
            "{\"call\": () => ::chooseFirst}",
            "{\"call\": () => ::chooseSecond}",
            "[\"call\"]!!()(3)",
        ),
        ("[[() => ::chooseFirst]]", "[[() => ::chooseSecond]]", "[0][0]()(3)"),
        (
            "(() -> (Int) -> Int => ::chooseFirst)",
            "(() -> (Int) -> Int => ::chooseSecond)",
            "()(3)",
        ),
    ] {
        let source =
            format!("scene start():\n    val factory = pair({left}, {right})\n    val result: Int = factory{suffix}\n");
        with_callbacks(&source).unwrap_or_else(|error| panic!("{error}\n{source}"));
    }
}

#[test]
fn coverage_unresolved_factory_sets_and_map_keys_remain_ineligible() {
    for (left, right) in [
        ("{::chooseFirst}", "{::chooseSecond}"),
        ("mutable {::chooseFirst}", "mutable {::chooseSecond}"),
        ("{::chooseFirst: 1}", "{::chooseSecond: 2}"),
        ("{null: ::chooseFirst}", "{null: ::chooseSecond}"),
        ("[null]", "[null]"),
        ("{}", "{}"),
    ] {
        let source = format!("scene start():\n    val result = pair({left}, {right})\n");
        assert!(
            with_callbacks(&source).is_err(),
            "untyped or unstable literal admitted:\n{source}"
        );
    }
}

#[test]
fn coverage_annotated_callback_factory_results_are_checked_before_joining_alternatives() {
    with_callbacks(
        r#"scene start():
    val factory = pair(() -> (Int) -> Int => ::chooseFirst, () -> (Int) -> Int => ::chooseSecond)
    val result: Int = factory()(3)
"#,
    )
    .expect("explicit callback results must remain valid when compatible");
    let incompatible = r#"scene start():
    val factory = pair(() -> (Float) -> Float => ::chooseFirst, () -> (Float) -> Float => ::chooseSecond)
"#;
    crate::parser::parse(incompatible, "src/test.bfl").expect("annotated factory syntax must parse");
    assert!(with_callbacks(incompatible).is_err());
}
