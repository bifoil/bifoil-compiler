//! Types, generic bounds, and projections using original Bifoil programs.

use super::compile;

#[test]
fn scalar_types_reject_discarded_type_arguments() {
    for scalar in ["Int", "Float", "Boolean", "String", "Char", "Unit", "Nothing", "Any"] {
        let source = format!(
            r#"fun unused(value: {scalar}<String>):
    return
"#
        );
        assert!(compile(&source).unwrap_err().contains("does not accept type arguments"));
    }
}
#[test]
fn generic_null_checks_preserve_non_null_parameter_identity() {
    compile(
        r#"fun <T> present(value: T) -> Any:
    if value != null:
        return value
    return Unit
"#,
    )
    .unwrap();
    compile(
        r#"fun <T> checked(value: T) -> Any = value!!
"#,
    )
    .unwrap();
    compile(
        r#"fun <T> same(value: T) -> T:
    if value != null:
        return value
    return value
"#,
    )
    .unwrap();
    compile(
        r#"fun <T> upper(value: T) -> String where T: String?:
    if value != null:
        return value
    return "missing"
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"fun <T> invalid(value: T) -> Any = value
"#
        )
        .unwrap_err()
        .contains("expected Any")
    );
}
#[test]
fn generic_bounds_expose_checked_members_and_preserve_privacy() {
    compile(
        r#"data class Box:
    var value: Int
    fun read() -> Int = value
fun <T: Box> update(item: T) -> Int:
    item.value += 1
    return item.read()
"#,
    )
    .unwrap();
    compile(
        r#"fun <T: MutableList<Int>> update(items: T) -> Int:
    items[0] = 3
    items.add(4)
    return items.size + items[0]
"#,
    )
    .unwrap();
    compile(
        r#"fun <T: List<Int>> total(items: T) -> Int:
    var result = 0
    for item in items:
        result += item
    return result
"#,
    )
    .unwrap();
    compile(
        r#"fun <T: String?> length(value: T) -> Int:
    if value != null:
        return value.length
    return 0
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"data class Box:
    private val secret: Int
fun <T: Box> invalid(item: T) -> Int = item.secret
"#
        )
        .unwrap_err()
        .contains("private")
    );
    assert!(
        compile(
            r#"data class Box:
    private fun secret() -> Int = 1
fun <T: Box> invalid(item: T) -> Int = item.secret()
"#
        )
        .unwrap_err()
        .contains("private")
    );
    assert!(
        compile(
            r#"data class Box:
    var value: Int
fun <T: Box?> invalid(item: T):
    item.value = 1
"#
        )
        .unwrap_err()
        .contains("nullable")
    );
    assert!(
        compile(
            r#"fun <T: MutableList<Int>?> invalid(items: T):
    items[0] = 1
"#
        )
        .unwrap_err()
        .contains("nullable")
    );
}
#[test]
fn primitive_generic_bounds_enable_only_their_checked_operations() {
    compile(
        r#"fun <T: Int> add(value: T) -> Int = value + 1
fun <T: Float> half(value: T) -> Float = value / 2
fun <T: Int> negated(value: T) -> Int = -value
fun <T: Boolean> inverted(value: T) -> Boolean = !value
"#,
    )
    .unwrap();
    compile(
        r#"fun <T: String> append(value: T) -> String = value + '!'
fun <T: Int> display(value: T):
    prose:
        ${value}
"#,
    )
    .unwrap();
    compile(
        r#"fun <T: Boolean> condition(value: T) -> Int:
    if value:
        return 1
    return 0
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"fun <T: Int> invalid(value: T):
    if value:
        return
"#
        )
        .unwrap_err()
        .contains("Boolean")
    );
    assert!(
        compile(
            r#"fun <T: Int?> invalid(value: T) -> Int = value + 1
"#
        )
        .unwrap_err()
        .contains("cannot combine")
    );
}
#[test]
fn primitive_generic_bounds_preserve_scalar_and_nullable_widening() {
    compile(
        r#"fun <T: Int> widen(value: T) -> Float = value
fun <T: Char> text(value: T) -> String = value
fun <T: Int?> optional(value: T) -> Float? = value
"#,
    )
    .unwrap();
    compile(
        r#"fun <T: Int> compare(value: T) -> Boolean = value == 1.0
fun <T: Char> compareChar(value: T) -> Boolean = value == "x"
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"fun <T: Equatable> invalid(value: T) -> Boolean = value == "x"
"#
        )
        .unwrap_err()
        .contains("cannot combine")
    );
}
#[test]
fn function_and_scene_bounds_retain_calls_nullability_and_effects() {
    compile(
        r#"fun <T: (Int) -> Int> apply(callback: T) -> Int = callback(3)
fun <T: ((Int) -> Int)?> optional(callback: T) -> Int = callback?.(3) ?: 0
"#,
    )
    .unwrap();
    compile(
        r#"suspend fun <T: suspend () -> Int> apply(callback: T) -> Int = callback()
"#,
    )
    .unwrap();
    compile(
        r#"suspend fun <T: Scene<Int>> visitScene(destination: T):
    visit destination(3)
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"fun <T: suspend () -> Int> invalid(callback: T) -> Int = callback()
"#
        )
        .unwrap_err()
        .contains("suspend context")
    );
    assert!(
        compile(
            r#"fun <T: ((Int) -> Int)?> invalid(callback: T) -> Int = callback(3)
"#
        )
        .unwrap_err()
        .contains("nullable")
    );
    assert!(
        compile(
            r#"fun <T: (Int) -> Int> invalid(callback: T) -> Int = callback(value = 3)
"#
        )
        .unwrap_err()
        .contains("parameter names")
    );
}
#[test]
fn instance_methods_need_receivers_and_enum_results_remain_callable() {
    let declarations = r#"enum Mood:
    Happy
    Sad
data class Box:
    val value: Int
    fun mood() -> Mood = Mood.Happy
    fun toString() -> String = "box"
"#;
    for operation in [
        "val invalid = Box.mood",
        "val invalid = Box.mood()",
        "val invalid = Box.toString()",
    ] {
        let program = format!("{declarations}scene start():\n    {operation}\n");
        let error = compile(&program).unwrap_err();
        assert!(error.contains("member"), "{error}");
    }
    compile(&format!(
        r#"{declarations}scene start():
    val box = Box(4)
    val read = box::mood
    val mood: Mood = read()
    val constructor = ::Box
    val description: String = constructor.toString()
"#
    ))
    .unwrap();
}
#[test]
fn bounded_projections_support_equality_and_stable_keys() {
    compile(
        r#"data class KeyBox<T: StableKey>:
    val value: T
data class EqualityBox<T: Equatable>:
    val value: T
scene start():
    val left: KeyBox<*> = KeyBox(3)
    val right: KeyBox<*> = KeyBox(3)
    val keys: Set<KeyBox<*>> = {left, right}
    val compared: EqualityBox<*> = EqualityBox(4)
    prose:
        ${keys.size}|${left == right}|${compared == compared}
"#,
    )
    .unwrap();
}

#[test]
fn concrete_bounds_preserve_projected_capabilities_through_fields() {
    compile(
        r#"export data class Key:
    val value: Int
export data class Box<T: Key>:
    val value: T
export data class Wrapped:
    val box: Box<*>
export data class IntBox<T: Int>:
    val value: T
export data class ListBox<T: List<Int>>:
    val value: T
export data class EqualityBox<T: MutableList<Int>>:
    val value: T
scene start():
    val key: Box<*> = Box(Key(3))
    val keys: Set<Wrapped> = {Wrapped(key)}
"#,
    )
    .unwrap();
}

#[test]
fn projected_bound_dependencies_reject_late_mutable_fields() {
    assert!(
        compile(
            r#"data class A:
    val value: Box<*>
data class Box<T: ZMutable>:
    val value: T
data class ZMutable:
    var value: Int
scene start():
    val item = A(Box(ZMutable(3)))
    val keys: Set<A> = {item}
"#
        )
        .unwrap_err()
        .contains("StableKey")
    );
}

#[test]
fn bounded_when_checks_and_preserves_generic_subjects() {
    compile(
        r#"export enum Mood:
    Happy
    Sad
export sealed class Outcome<T>:
    case Success(value: T)
    case Pending
fun needMood(value: Mood) -> String = "present"
fun needBoolean(value: Boolean) -> String = value.toString()
export fun <T: Mood> enumName(value: T) = when value:
    case Happy:
        "happy"
    case Sad:
        "sad"
export fun <T: Mood?> optionalMood(value: T) = when value:
    case null:
        "none"
    case Happy, Sad:
        needMood(value)
export fun <T: Boolean?> booleanName(value: T) = when value:
    case true:
        needBoolean(value)
    case false:
        needBoolean(value)
    case null:
        "missing"
export fun <T: Outcome<Int>> inspect(value: T) = when value:
    case is Outcome.Success<Int>(number):
        number
    case is Outcome.Pending<Int>:
        0
export fun <T: Outcome<Int>?> optionalOutcome(value: T) = when value:
    case null:
        -1
    case is Outcome.Success<Int>(number):
        number
    case is Outcome.Pending<Int>:
        0
export fun <T: Boolean?> booleanAfterNull(value: T):
    when value:
        case null if true:
            return "missing"
    return needBoolean(value)
export fun <T> identity(value: T) -> T:
    when value:
        case null:
            return value
        else:
            return value
export fun nonNullAlternative(value: String?, alternative: String) = when value:
    case alternative, "x":
        value.length
    else:
        0
scene start():
    prose:
        ${enumName(Mood.Happy)}
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"enum Mood:
    Happy
fun <T: Equatable> invalid(value: T) -> Boolean = value == Mood.Happy
scene start():
    invalid(3)
"#
        )
        .unwrap_err()
        .contains("cannot combine")
    );
}

#[test]
fn diagnostic_types_use_source_spelling_without_internal_parameter_ids() {
    for (source, expected) in [
        (
            "fun wrong(value: Int?) -> String = value\n",
            "expected String, found Int?",
        ),
        (
            "fun wrong(value: (Int) -> Int) -> (String) -> Int = value\n",
            "expected (String) -> Int, found (Int) -> Int",
        ),
        (
            "fun wrong(value: ((Int) -> Int)?) -> ((String) -> Int)? = value\n",
            "expected ((String) -> Int)?, found ((Int) -> Int)?",
        ),
        (
            "fun wrong(value: Scene<Int>) -> Scene<String> = value\n",
            "expected Scene<String>, found Scene<Int>",
        ),
        (
            "fun wrong(value: (vararg Int) -> Unit) -> (Int) -> Unit = value\n",
            "expected (Int) -> Unit, found (vararg Int) -> Unit",
        ),
        (
            "fun wrong(value: suspend (Int) -> Unit) -> (Int) -> Unit = value\n",
            "expected (Int) -> Unit, found suspend (Int) -> Unit",
        ),
        (
            "fun <T> wrong(value: T) = value + 1\n",
            "operator '+' cannot combine T and Int",
        ),
    ] {
        let error = compile(source).unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("test::") && !error.contains("Parameter {"), "{error}");
    }
}

#[test]
fn projected_builtin_keys_retain_only_their_declared_capabilities() {
    compile(
        r#"export data class Keys:
    val members: Set<*>
    val table: Map<*, Int>
export fun sameSet(left: Set<*>, right: Set<*>) -> Boolean = left == right
export fun sameMap(left: Map<*, Int>, right: Map<*, Int>) -> Boolean = left == right
export data class SetBox<T: Set<*>>:
    val value: T
scene start():
    val raw = {1, 2}
    val projected: Set<*> = raw
    val nested: Set<Set<*>> = {projected}
    val boxed: Set<SetBox<*>> = {SetBox(projected)}
"#,
    )
    .unwrap();
    for source in [
        r#"fun invalid(value: List<*>):
    val keys: Set<List<*>> = {value}
scene start():
    invalid([1])
"#,
        r#"fun invalid(value: MutableSet<*>):
    val keys: Set<MutableSet<*>> = {value}
scene start():
    invalid(mutable {1})
"#,
        r#"fun invalid(value: Map<*, *>):
    val keys: Set<Map<*, *>> = {value}
scene start():
    invalid({1: 3})
"#,
    ] {
        let error = compile(source).unwrap_err();
        assert!(error.contains("StableKey"), "{error}");
    }
}

#[test]
fn projected_copies_use_non_null_value_bounds_and_retain_overload_ranking() {
    compile(
        r#"export fun snapshot(values: List<*>, ignored: Int) -> String = "${values.size}|${values is List<Int>}"
export fun <T: MutableList<Int>> copyBound(values: T) -> List<*> = values
export fun <T: MutableList<Int>?> copyChecked(values: T) -> List<*> = values!!
export fun pick(values: List<Int>) -> String = "exact"
export fun pick(values: List<*>) -> String = "projected"
export suspend fun grow(values: MutableList<Int>) -> Int:
    choice:
        option "Grow":
            values.add(3)
    return 0
scene start():
    val raw = mutable [1, 2]
    val projected: List<*> = raw
    val writable: MutableList<*> = projected
    val bound = copyBound(raw)
    val checked = copyChecked<MutableList<Int>?>(raw)
"#,
    )
    .unwrap();
    let error = compile(
        r#"fun <T: MutableList<Int>?> invalid(values: T) -> List<*> = values
scene start():
    return
"#,
    )
    .unwrap_err();
    assert!(error.contains("expected List<*>"), "{error}");
}

#[test]
fn narrowed_generic_bindings_keep_their_declared_identity_in_context() {
    compile(
        r#"fun <T> identity(value: T) -> T = value
fun <T> preserve(value: T) -> T:
    if value is String:
        val original: T = value
        return original
    return value
fun <T> forwarded(value: T) -> T:
    if value is String:
        return identity<T>(value)
    return value
fun <T> branch(value: T) -> T = if value is String:
    value
else:
    value
scene start():
    val a: String = preserve("Ada")
    val b: String = forwarded("four")
    val c: Int = branch(3)
"#,
    )
    .unwrap();
}

#[test]
fn newly_computed_values_do_not_inherit_a_generic_binding_identity() {
    for expression in ["value + \"!\"", "String(value)"] {
        let source = r#"fun <T> changed(value: T) -> T:
    if value is String:
        return EXPRESSION
    return value
scene start():
    Unit
"#
        .replace("EXPRESSION", expression);
        let error = compile(&source).expect_err("a new String need not belong to the caller's T");
        assert!(error.contains("expected T, found String"), "{error}");
    }
}

#[test]
fn nullable_generic_bounds_compose_with_function_and_nominal_variance() {
    compile(
        r#"data class Box<out T>:
    val item: T
fun <T: String?> widen(reader: () -> T) -> () -> String? = reader
fun <T: String?> narrow(writer: (String?) -> Int) -> (T) -> Int = writer
fun <T: String?> widerBox(item: Box<T>) -> Box<String?> = item
fun <T: String?> widerList(items: List<T>) -> List<String?> = items
fun <T: String?, U: T?> chained(item: Box<U>) -> Box<String?> = item
fun <T: String?> suspendReader(reader: suspend () -> T) -> suspend () -> String? = reader
scene start():
    val reader = widen<String>(() => "Ada")
    val missing = widen<String?>(() => null)
    val writer = narrow<String>((text: String?) => text?.length ?: 0)
    val box = widerBox<String>(Box("yes"))
    val items = widerList<String?>(["yes", null])
"#,
    )
    .unwrap();
}

#[test]
fn nullable_generic_variance_does_not_convert_or_unwrap_function_signatures() {
    for declaration in [
        "fun <T: Int?> invalid(reader: () -> T) -> () -> Float? = reader",
        "fun <T: Char?> invalid(reader: () -> T) -> () -> String? = reader",
        "fun <T: String?> invalid(reader: () -> T) -> () -> String = reader",
        "fun <T: String?> invalid(reader: () -> String?) -> () -> T = reader",
    ] {
        let error = compile(declaration).expect_err("bounds must not create adapters or erase null");
        assert!(error.contains("expected"), "{error}\n{declaration}");
    }
}

#[test]
fn nullable_generic_joins_retain_the_proven_common_upper_type() {
    let module = compile(
        r#"fun <T: String?> choose(flag: Boolean, first: T, second: String?):
    return if flag:
        first
    else:
        second
fun <T: String?> paths(flag: Boolean, first: T, second: String?):
    if flag:
        return first
    return second
suspend fun <T: String?> saved(flag: Boolean, first: T, second: String?):
    choice:
        option "Choose":
            Unit
    return if flag:
        first
    else:
        second
scene start():
    val result = choose(true, "yes", null)
"#,
    )
    .unwrap();
    for name in ["test.choose", "test.paths", "test.saved"] {
        assert_eq!(
            module.functions[name].signature.result,
            crate::model::Type::String.nullable()
        );
    }
}
