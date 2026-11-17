//! Nullable projected inputs permit null without guessing a hidden type argument.

use super::compile;

#[test]
fn projected_nullable_method_inputs_accept_only_null() {
    compile(
        r#"data class Box<T: Any>:
    var value: T?
    fun replace(next: T? = null) -> T?:
        val previous = value
        value = next
        return previous
    fun count(vararg values: T?) -> Int = values.size
scene start():
    val original = Box<Int>(3)
    val hidden: Box<*> = original
    val previous: Any? = hidden.replace(null)
    hidden.replace()
    val reset: (Nothing?) -> Any? = hidden::replace
    reset(null)
    val count = hidden.count(null, null)
    val copied = hidden.copy(value = null)
    prose:
        ${previous != null}|${count}|${copied.value == null}
"#,
    )
    .unwrap();
}

#[test]
fn nullable_capable_hidden_parameters_do_not_guarantee_nullable_inputs() {
    for declaration in ["T", "T?"] {
        let argument = if declaration == "T" { "null" } else { "7" };
        let source = format!(
            "data class Box<T>:\n    val value: T\n    fun accept(next: {declaration}):\n        Unit\nscene start():\n    val hidden: Box<*> = Box(3)\n    hidden.accept({argument})\n"
        );
        assert!(compile(&source).is_err(), "hidden type was assumed: {source}");
    }
}

#[test]
fn projected_callback_fields_and_default_only_calls_keep_safe_function_types() {
    compile(
        r#"data class Callbacks<T: Any>:
    val callback: (T?) -> T?
    fun get() -> (T?) -> T? = callback
data class Box<T>:
    val value: T
    fun use(next: T = value) -> T = next
    fun count(vararg values: T) -> Int = values.size
scene start():
    val callbacks = Callbacks<Int>((value: Int?) => value)
    val hidden: Callbacks<*> = callbacks
    val direct: (Nothing?) -> Any? = hidden.callback
    val returned: (Nothing?) -> Any? = hidden.get()
    direct(null)
    returned(null)
    val box: Box<*> = Box(7)
    val previous: Any? = box.use()
    val copied = box.copy()
    val count: (vararg Nothing) -> Int = box::count
    prose:
        ${previous != null}|${copied is Box<Int>}|${box.count()}|${count()}
"#,
    )
    .unwrap();
}

#[test]
fn projected_inputs_follow_callable_and_immutable_container_variance() {
    compile(
        r#"data class Box<T>:
    val value: T
    fun <U> map(callback: (T) -> U) -> U = callback(value)
    fun list(values: List<T>) -> Int = values.size
    fun present() = value!!
scene start():
    val hidden: Box<*> = Box(7)
    val text = hidden.map((value: Any?) => String(value))
    val mapped: ((Any?) -> String) -> String = hidden::map
    mapped((value: Any?) => String(value))
    val count = hidden.list([])
    val present: Any = hidden.present()
"#,
    )
    .unwrap();
    let definitions = r#"data class Box<T>:
    val value: T
    fun map(callback: (T) -> String) -> String = callback(value)
    fun produce(callback: () -> T) -> T = callback()
    fun takeMutable(values: MutableList<T>) -> Int = values.size
scene start():
    val hidden: Box<*> = Box(7)
"#;
    compile(&format!("{definitions}    Unit\n")).unwrap();
    for operation in [
        "hidden.map((value: Int) => String(value))",
        "hidden.produce(() => 7)",
        "hidden.takeMutable(mutable [1])",
    ] {
        let error = compile(&format!("{definitions}    {operation}\n"))
            .expect_err("a projected call must not choose a hidden argument");
        assert!(
            error.contains("argument") || error.contains("Nothing"),
            "{operation}: {error}"
        );
    }
}

#[test]
fn projected_write_restrictions_do_not_make_mutable_fields_permanent_facts() {
    let error = compile(
        r#"data class Box<T>:
    var value: T
fun clear(box: Box<String?>):
    box.value = null
scene start():
    val original = Box<String?>("Ada")
    val hidden: Box<*> = original
    if hidden.value is String:
        clear(original)
        prose:
            ${hidden.value.length}
"#,
    )
    .expect_err("a concrete alias can invalidate a projected mutable-field guard");
    assert!(error.contains("nullable") || error.contains("length"), "{error}");
}

#[test]
fn immutable_projected_fields_keep_their_guards_through_calls_and_suspension() {
    compile(
        r#"data class Box<T>:
    val value: T
fun ping():
    Unit
scene start():
    val hidden: Box<*> = Box("Ada")
    if hidden.value is String:
        ping()
        choice:
            option "Continue":
                Unit
        prose:
            ${hidden.value.length}
"#,
    )
    .unwrap();
}

#[test]
fn dependent_projected_bounds_retain_known_and_hidden_sibling_capabilities() {
    compile(
        r#"data class Key<U: StableKey, T: U>:
    val value: T
data class Nested<U: StableKey, T: List<U>>:
    val value: T
scene start():
    val known: Key<Int, *> = Key<Int, Int>(1)
    val hidden: Key<*, *> = Key<String, String>("two")
    val keys: Set<Key<*, *>> = {known, hidden}
    val nested: Nested<*, *> = Nested<Int, List<Int>>([1, 2])
    val containers: Set<Nested<*, *>> = {nested}
"#,
    )
    .unwrap();
}

#[test]
fn dependent_projected_bounds_do_not_turn_equatable_or_mutable_values_into_stable_keys() {
    for bound in ["Equatable", "Any?"] {
        let source = r#"data class Key<U: BOUND, T: U>:
    val value: T
scene start():
    val value: Key<*, *> = Key<MutableList<Int>, MutableList<Int>>(mutable [1])
    val keys: Set<Key<*, *>> = {value}
"#
        .replace("BOUND", bound);
        crate::parser::parse(&source, "negative.bfl").unwrap();
        assert!(compile(&source).unwrap_err().contains("StableKey"), "{source}");
    }
}

#[test]
fn projected_capability_states_retain_outer_generic_argument_requirements() {
    compile(
        r#"data class Inner<U, T: U>:
    val value: T
data class Outer<X>:
    val nested: Inner<X, *>
    val next: Outer<List<X>>? = null
fun <X: StableKey> accept(value: Outer<X>) -> Boolean = value == value
scene start():
    val concrete = Inner<Int, Int>(7)
    val wrapped = Outer<Int>(concrete)
    val keys: Set<Outer<Int>> = {wrapped}
    val accepted = accept(wrapped)
"#,
    )
    .unwrap();
}

#[test]
fn projected_capabilities_keep_every_sufficient_bound_across_multiple_fields() {
    compile(
        r#"data class Inner<A, B, T> where T: A, T: B:
    val value: T
data class Outer<X, Y, Z>:
    val first: Inner<X, Y, *>
    val second: Inner<Y, Z, *>
scene start():
    val middle = Outer<Any?, Int, Any?>(Inner<Any?, Int, Int>(1), Inner<Int, Any?, Int>(2))
    val middleKeys: Set<Outer<Any?, Int, Any?>> = {middle}
    val edges = Outer<Int, Any?, Int>(Inner<Int, Any?, Int>(3), Inner<Any?, Int, Int>(4))
    val edgeKeys: Set<Outer<Int, Any?, Int>> = {edges}
"#,
    )
    .unwrap();
}

#[test]
fn outer_unstable_arguments_do_not_gain_capabilities_from_a_concrete_hidden_value() {
    let definitions = r#"data class Inner<U, T: U>:
    val value: T
data class Outer<X>:
    val nested: Inner<X, *>
scene start():
"#;
    for construction in [
        "Outer<Any?>(Inner<Any?, Int>(7))",
        "Outer<MutableList<Int>>(Inner<MutableList<Int>, MutableList<Int>>(mutable [7]))",
    ] {
        let source = format!("{definitions}    val wrapped = {construction}\n    val keys = {{wrapped}}\n");
        assert!(compile(&source).unwrap_err().contains("StableKey"), "{source}");
    }
}

#[test]
fn sufficient_projected_bounds_match_the_full_three_argument_stability_matrix() {
    let declarations = r#"data class Inner<A, B, T> where T: A, T: B:
    val value: T
data class Outer<X, Y, Z>:
    val first: Inner<X, Y, *>
    val second: Inner<Y, Z, *>
scene start():
"#;
    for mask in 0..8 {
        let args: Vec<_> = (0..3)
            .map(|bit| if mask & (1 << bit) != 0 { "Int" } else { "Any?" })
            .collect();
        let (x, y, z) = (args[0], args[1], args[2]);
        let source = format!(
            "{declarations}    val item = Outer<{x}, {y}, {z}>(Inner<{x}, {y}, Int>(1), Inner<{y}, {z}, Int>(2))\n    val keys = {{item}}\n"
        );
        let stable = (x == "Int" || y == "Int") && (y == "Int" || z == "Int");
        let result = compile(&source);
        if stable {
            result.unwrap_or_else(|error| panic!("{mask}: {error}"));
        } else {
            assert!(result.unwrap_err().contains("StableKey"), "{mask}");
        }
    }
}

#[test]
fn many_independent_projected_bounds_do_not_expand_sufficient_combinations() {
    let mut parameters = Vec::new();
    let mut fields = String::new();
    let mut arguments = Vec::new();
    let mut values = Vec::new();
    for index in 0..28 {
        parameters.extend([format!("A{index}"), format!("B{index}")]);
        fields += &format!("    val field{index}: Inner<A{index}, B{index}, *>\n");
        arguments.extend(["Any?", "Int"]);
        values.push("Inner<Any?, Int, Int>(7)");
    }
    let source = format!(
        "data class Inner<A, B, T> where T: A, T: B:\n    val value: T\ndata class Wide<{}>:\n{fields}scene start():\n    val item = Wide<{}>({})\n    val keys = {{item}}\n",
        parameters.join(", "),
        arguments.join(", "),
        values.join(", ")
    );
    assert!(source.len() < 4000);
    compile(&source).unwrap();
}

#[test]
fn recursive_nominal_state_obstructions_propagate_before_caching_a_result() {
    let declarations = r#"data class First:
    val second: Second? = null
    var changed: Int = 0
data class Second:
    val first: First? = null
scene start():
    val first = First()
    val second = Second()
"#;
    compile(&format!("{declarations}    val equal = second == second\n")).unwrap();
    for operation in ["val keys = {first}", "val keys = {second}"] {
        let error = compile(&format!("{declarations}    {operation}\n")).unwrap_err();
        assert!(error.contains("StableKey"), "{error}");
    }
}

#[test]
fn a_recursive_payload_bound_cannot_prove_its_own_capability() {
    let source = r#"data class Payload<T>:
    val value: T
data class Hidden<T: Payload<T>>:
    val value: T
fun reject(value: Hidden<*>):
    val keys = {value}
scene start():
    Unit
"#;
    let error = compile(source).unwrap_err();
    assert!(error.contains("StableKey"), "{error}");
}

#[test]
fn long_recursive_capability_dependencies_propagate_the_terminal_obstruction() {
    let mut source = String::new();
    for index in 0..1000 {
        source += &format!("data class Link{index}:\n    val next: Link{}? = null\n", index + 1);
    }
    source += "data class Link1000:\n    var terminal: Int = 0\nscene start():\n    val first = Link0()\n    val keys = {first}\n";
    assert!(source.len() < crate::model::MAX_SOURCE);
    let error = compile(&source).unwrap_err();
    assert!(error.contains("StableKey"), "{error}");
}
