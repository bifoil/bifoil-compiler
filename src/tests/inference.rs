//! Callable result inference using original Bifoil programs.

use super::compile;

#[test]
fn anonymous_final_when_returns_exhaustive_branch_values() {
    compile(
        r#"enum Mood:
    Happy
    Sad
scene start():
    val choose = (flag: Boolean) =>:
        when flag:
            case true:
                10
            case false:
                20
    val describe = (mood: Mood) =>:
        when mood:
            case Mood.Happy:
                "happy"
            case Mood.Sad:
                "sad"
    val answer: Int = choose(true)
    val description: String = describe(Mood.Happy)
"#,
    )
    .unwrap();
}
#[test]
fn inferred_returns_keep_only_reachable_branch_narrowing() {
    compile(
        r#"fun length(name: String?):
    if name == null:
        return 0
    return name.length
fun textLength(input: Any):
    if input !is String:
        return 0
    return input.length
scene start():
    prose:
        ${length(null)}|${length("Ada")}|${textLength("hello")}
"#,
    )
    .unwrap();
}
#[test]
fn inferred_returns_include_transfers_inside_larger_expressions() {
    compile(
        r#"fun immediate() = return 42
fun branches(flag: Boolean):
    val unused = if flag:
        return 1
    else:
        return 2
fun build() = () => return 7
scene start():
    prose:
        ${immediate()}|${branches(true)}|${branches(false)}|${build()()}
"#,
    )
    .unwrap();
}
#[test]
fn inferred_guarded_when_and_choice_returns_use_branch_proofs() {
    compile(
        r#"fun guarded(name: String?, allowed: Boolean):
    when:
        case allowed if name != null:
            return name.length
        else:
            return 0
suspend fun ask(name: String?):
    choice:
        option "Length ${name.length}" if name != null:
            return name.length
        else:
            return 0
scene start():
    prose:
        ${guarded("Ada", true)}
    ask("Ada")
"#,
    )
    .unwrap();
}
#[test]
fn bottom_results_remain_distinct_from_default_unit_results() {
    let module = compile(
        r#"fun never() -> Nothing:
    while true:
        Unit
fun forwarded() = never()
fun forwardedBlock():
    return never()
fun blockDefault():
    never()
fun asInt() -> Int = forwarded()
scene start():
    Unit
"#,
    )
    .unwrap();
    assert_eq!(
        module.functions["test.forwarded"].signature.result,
        crate::model::Type::Nothing
    );
    assert_eq!(
        module.functions["test.forwardedBlock"].signature.result,
        crate::model::Type::Nothing
    );
    assert_eq!(
        module.functions["test.blockDefault"].signature.result,
        crate::model::Type::Unit
    );
}
#[test]
fn bottom_unary_operands_preserve_the_enclosing_return() {
    compile(
        r#"fun number() = -(return 42)
fun flag() = !(return true)
scene start():
    prose:
        ${number()}|${flag()}
"#,
    )
    .unwrap();
}
#[test]
fn inferred_loop_exits_retain_null_checks_and_labeled_transfers() {
    compile(
        r#"export fun length(var input: String?):
    while input == null:
        input = "seed"
    return input.length
export fun post(var input: String?):
    do:
        input = input ?: "Ada"
    while input == null
    return input.length
export fun breakLength(var input: String?):
    while true:
        if input != null:
            break
        input = "Ada"
    return input.length
export fun rangeLength(input: String?):
    for i in 1..1:
        if input == null:
            return 0
        break
    return input.length
export fun nested(var input: String?):
    outer@ while true:
        for i in 1..1:
            if input == null:
                input = "Ada"
            if input != null:
                break@outer
    return input.length
export fun continued(var input: String?):
    do:
        val suffix = "Ada"
        if input == null:
            input = suffix
            continue
    while suffix.length == 0 || input == null
    return input.length
export fun conditionReturns():
    do:
        val reached = true
    while return 7
export suspend fun askLength(var input: String?):
    while input == null:
        choice:
            option "Fill":
                input = "saved"
    return input.length
scene start():
    prose:
        ${length(null)}
"#,
    )
    .unwrap();
}

#[test]
fn value_blocks_follow_prefix_guards_loops_and_builtin_effects() {
    compile(
        r#"fun guarded(name: String?, flag: Boolean):
    return if flag:
        if name == null:
            return 0
        name.length
    else:
        9
fun filled(var name: String?):
    return if true:
        while name == null:
            name = "seed"
        name.length
    else:
        0
fun rendered(input: String?):
    return if true:
        var name = input
        if name == null:
            return 0
        val text = String(name)
        name.length
    else:
        0
scene start():
    val a: Int = guarded("Ada", true)
    val b: Int = filled(null)
    val c: Int = rendered("yes")
    val numeric = if true:
        1
    else:
        2.5
    val widened: Float = numeric
"#,
    )
    .unwrap();
}

#[test]
fn value_block_prefixes_and_conditions_invalidate_mutable_proofs() {
    for expression in [
        "name = null\n            name.length",
        "if clear():\n                name.length\n            else:\n                0",
    ] {
        let source = r#"scene start():
    var name: String? = "Ada"
    fun clear() -> Boolean:
        name = null
        return true
    if name != null:
        val value = if true:
            BODY
        else:
            0
"#
        .replace("BODY", expression);
        let error = compile(&source).expect_err("a changed binding must lose its null proof");
        assert!(error.contains("nullable member receiver"), "{error}");
    }
}

#[test]
fn value_blocks_resolve_forward_and_mutually_recursive_local_results() {
    compile(
        r#"fun forward(flag: Boolean):
    return if flag:
        val result = width(3)
        fun width(value: Int) = value * 2
        result
    else:
        0
fun recursive(flag: Boolean):
    return if flag:
        val result = even(6)
        fun even(value: Int):
            if value == 0:
                return true
            return odd(value - 1)
        fun odd(value: Int):
            if value == 0:
                return false
            return even(value - 1)
        result
    else:
        false
scene start():
    val number: Int = forward(true)
    val flag: Boolean = recursive(true)
"#,
    )
    .unwrap();
}

#[test]
fn recursive_local_results_still_require_a_concrete_anchor() {
    let error = compile(
        r#"fun unresolved(flag: Boolean):
    return if flag:
        fun recurse() = recurse()
        recurse()
    else:
        0
scene start():
    Unit
"#,
    )
    .expect_err("a return outside the recursive function cannot anchor its result");
    assert!(
        error.contains("cannot infer") || error.contains("compatible joined type"),
        "{error}"
    );
}

#[test]
fn expression_results_anchor_recursive_if_and_when_functions() {
    compile(
        r#"fun factorial(value: Int) = if value <= 1:
    1
else:
    value * factorial(value - 1)
fun even(value: Int) = when value:
    case 0:
        true
    else:
        odd(value - 1)
fun odd(value: Int) = when value:
    case 0:
        false
    else:
        even(value - 1)
scene start():
    val number: Int = factorial(5)
    val flag: Boolean = even(6)
"#,
    )
    .unwrap();
}

#[test]
fn inferred_generic_results_retain_original_binding_identity() {
    let module = compile(
        r#"fun <T> unchanged(value: T):
    if value is String:
        return value
    return value
fun <T> expression(value: T) = if value is String:
    value
else:
    value
fun <T> nested(value: T, flag: Boolean):
    if value is String:
        return if flag:
            value
        else:
            value
    return value
scene start():
    val number: Int = unchanged(3)
    val text: String = expression("Ada")
    val nestedText: String = nested("four", true)
"#,
    )
    .unwrap();
    for name in ["test.unchanged", "test.expression", "test.nested"] {
        let signature = &module.functions[name].signature;
        assert_eq!(
            signature.result,
            crate::model::Type::parameter(signature.generics[0].name.clone())
        );
    }
}

#[test]
fn inferred_identity_does_not_add_any_fallback_or_retype_new_values() {
    for source in [
        r#"fun ambiguous(value: Any):
    if value is String:
        return value
    if value is Int:
        return value
    return false
scene start():
    Unit
"#,
        r#"fun <T> changed(value: T):
    if value is String:
        return value + "!"
    return value
scene start():
    Unit
"#,
    ] {
        let error = compile(source).expect_err("unrelated values still need an explicit compatible result type");
        assert!(
            error.contains("function return paths have incompatible types"),
            "{error}"
        );
    }
}

#[test]
fn sibling_variants_infer_their_declared_common_sum() {
    let module = compile(
        r#"sealed class Outcome:
    case Success(item: String)
    case Failure(code: Int)
sealed class Result<T>:
    case Success(item: T)
    case Pending
fun choose(flag: Boolean):
    return if flag:
        Outcome.Success("yes")
    else:
        Outcome.Failure(1)
fun paths(flag: Boolean):
    if flag:
        return Outcome.Success("yes")
    return Outcome.Failure(1)
fun rows() = [Outcome.Success("yes"), Outcome.Failure(1)]
fun <T> generic(flag: Boolean, item: T):
    return if flag:
        Result.Success(item)
    else:
        Result.Pending<T>()
scene start():
    val selected: (Boolean) -> Outcome = ::choose
    val values = rows()
    val comparison = Outcome.Success("yes") == Outcome.Failure(1)
"#,
    )
    .unwrap();
    let outcome = crate::model::Type::named("test.Outcome", vec![]);
    for name in ["test.choose", "test.paths"] {
        assert_eq!(module.functions[name].signature.result, outcome);
    }
    assert_eq!(
        module.functions["test.rows"].signature.result,
        crate::model::Type::named("List", vec![outcome])
    );
    let generic = &module.functions["test.generic"].signature;
    assert_eq!(
        generic.result,
        crate::model::Type::named(
            "test.Result",
            vec![crate::model::Type::parameter(&generic.generics[0].name)]
        )
    );
}

#[test]
fn common_sum_inference_does_not_join_unrelated_types_or_invariant_arguments() {
    for (first, second) in [
        ("Left.Ready", "Right.Ready"),
        ("Result.Success(1)", "Result.Pending<String>()"),
    ] {
        let source = format!(
            r#"sealed class Left:
    case Ready
sealed class Right:
    case Ready
sealed class Result<T>:
    case Success(item: T)
    case Pending
fun choose(flag: Boolean):
    return if flag:
        {first}
    else:
        {second}
scene start():
    Unit
"#
        );
        let error = compile(&source).expect_err("no implicit Any or invariant payload adapter is available");
        assert!(error.contains("compatible"), "{error}\n{source}");
    }
}

#[test]
fn proven_present_generic_reads_do_not_make_inferred_results_nullable() {
    let module = compile(
        r#"data class Box<T>:
    val value: T?
fun <T> binding(value: T?, fallback: T):
    if value is String:
        return value
    return fallback
fun <T> field(holder: Box<T>, fallback: T):
    if holder.value is String:
        return holder.value
    return fallback
fun <T> list(values: List<T?>, fallback: T):
    if values[0] is String:
        return values[0]
    return fallback
fun <T> map(values: Map<String, T>, fallback: T):
    if values["value"] is String:
        return values["value"]
    return fallback
fun <T> conditional(value: T?, fallback: T) = if value is String:
    value
else:
    fallback
fun <T> matched(value: T?, fallback: T) = when value:
    case is String:
        value
    else:
        fallback
scene start():
    val a: Int = binding(3, 0)
    val b: Int = field(Box<Int>(3), 0)
    val c: Int = list([3], 0)
    val d: Int = map({"value": 3}, 0)
    val e: Int = conditional(3, 0)
    val f: Int = matched(3, 0)
"#,
    )
    .unwrap();
    for name in ["binding", "field", "list", "map", "conditional", "matched"] {
        let signature = &module.functions[&format!("test.{name}")];
        assert_eq!(
            signature.signature.result,
            crate::model::Type::parameter(&signature.signature.generics[0].name)
        );
    }
}

#[test]
fn generic_result_presence_proofs_do_not_erase_real_nulls_or_identify_computed_values() {
    let cases = [
        (
            r#"fun <T> maybe(value: T?):
    if value is String:
        return value
    return null
scene start():
    val text: String = maybe("Ada")
"#,
            "expected String",
        ),
        (
            r#"fun <T> invalid(value: T?, fallback: T) -> T:
    if value is String:
        return value + "changed"
    return fallback
"#,
            "expected T",
        ),
    ];
    for (source, expected) in cases {
        let error = compile(source).expect_err("presence is a path proof, not a conversion or a new generic identity");
        assert!(error.contains(expected), "{error}\n{source}");
    }
}

#[test]
fn prepared_module_inference_tracks_shared_forward_and_overloaded_results() {
    let module = compile(
        r#"shared val message = seed()
fun seed() = "Hello"
fun measure() = forward()
fun forward() = message.length
fun select(value: Int) = measure() + value
fun select(value: Float) = Float(measure()) + value
fun sample() = select(0.5)
scene start():
    val result: Float = sample()
    prose:
        ${result}
"#,
    )
    .unwrap();
    assert_eq!(module.shared["test.message"].ty, crate::model::Type::String);
    for (name, expected) in [
        ("seed", crate::model::Type::String),
        ("measure", crate::model::Type::Int),
        ("forward", crate::model::Type::Int),
        ("sample", crate::model::Type::Float),
    ] {
        assert_eq!(module.functions[&format!("test.{name}")].signature.result, expected);
    }
}
