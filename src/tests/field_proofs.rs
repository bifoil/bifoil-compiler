//! Stable field paths retain sound null/type facts without runtime adapters.

use super::compile;
use crate::model::Type;

#[test]
fn immutable_field_paths_support_guards_assertions_and_nested_receivers() {
    let module = compile(
        r#"data class Person:
    val name: String?
data class House:
    val person: Person?
fun length(person: Person):
    if person.name == null:
        return 0
    return person.name.length
fun nested(home: House):
    if home.person == null:
        return 0
    if home.person.name == null:
        return 0
    return home.person.name.length
fun checked(person: Person):
    person.name!!
    return person.name.length
fun condition(person: Person):
    if (person.name) != null && person.name.length > 0:
        return person.name.length
    return 0
scene start():
    val size = length(Person("Ada"))
"#,
    )
    .unwrap();
    for name in ["test.length", "test.nested", "test.checked", "test.condition"] {
        assert_eq!(module.functions[name].signature.result, Type::Int);
    }
}

#[test]
fn field_refinements_preserve_generic_identity_and_current_type_views() {
    let module = compile(
        r#"data class Box<T>:
    val value: T
fun <T> keep(holder: Box<T>) -> T:
    if holder.value is String:
        return holder.value
    return holder.value
fun <T> inferred(holder: Box<T>):
    if holder.value is String:
        return holder.value
    return holder.value
fun fieldLength(holder: Box<Any?>):
    if holder.value !is String:
        return 0
    holder.value!!
    return holder.value.length
fun nestedField(holder: Box<Any?>):
    if holder.value !is Box<String?>:
        return 0
    if holder.value.value == null:
        return 0
    return holder.value.value.length
fun scalarLength(value: Any?):
    if value !is String:
        return 0
    value!!
    return value.length
scene start():
    Unit
"#,
    )
    .unwrap();
    let inferred = &module.functions["test.inferred"].signature;
    assert_eq!(inferred.result, Type::parameter(&inferred.generics[0].name));
    assert_eq!(module.functions["test.fieldLength"].signature.result, Type::Int);
    assert_eq!(module.functions["test.nestedField"].signature.result, Type::Int);
    assert_eq!(module.functions["test.scalarLength"].signature.result, Type::Int);
}

#[test]
fn immutable_field_facts_survive_calls_and_capturing_functions() {
    compile(
        r#"data class Person:
    val name: String?
fun callback(person: Person, effect: () -> Unit) -> () -> Int:
    if person.name == null:
        return () => 0
    effect()
    return () => person.name.length
fun guarded(person: Person) -> Int:
    if person.name != null:
        val before = read()
        fun read() -> Int = person.name.length
        return before
    return 0
scene start():
    val reader = callback(Person("Ada"), () => Unit)
    reader()
"#,
    )
    .unwrap();
}

#[test]
fn field_facts_do_not_escape_mutable_paths_or_unproven_branches() {
    let sources = [
        r#"data class Person:
    var name: String?
fun invalid(person: Person) -> Int:
    if person.name == null:
        return 0
    person.name = null
    return person.name.length
"#,
        r#"data class Person:
    val name: String?
fun invalid(var person: Person) -> Int:
    if person.name == null:
        return 0
    person = Person(null)
    return person.name.length
"#,
        r#"data class Person:
    val name: String?
data class House:
    var person: Person?
fun invalid(home: House) -> Int:
    if home.person == null:
        return 0
    home.person = null
    return home.person.name.length
"#,
        r#"data class Person:
    val name: String?
fun invalid(person: Person, flag: Boolean) -> Int:
    if flag:
        person.name!!
    return person.name.length
"#,
        r#"data class Person:
    val name: String?
fun invalid(person: Person, flag: Boolean) -> Int:
    if person.name != null || flag:
        return person.name.length
    return 0
"#,
        r#"data class Person:
    val name: String?
fun make() -> Person = Person(null)
fun invalid() -> Int:
    if make().name == null:
        return 0
    return make().name.length
"#,
    ];
    for source in sources {
        let error = compile(source).expect_err("this field read has no stable dominating proof");
        assert!(error.contains("nullable member receiver"), "{error}\n{source}");
    }
}

#[test]
fn hoisted_field_reads_cannot_use_a_later_same_block_guard() {
    let error = compile(
        r#"data class Person:
    val name: String?
fun invalid(person: Person) -> Int:
    val before = read()
    if person.name == null:
        return 0
    fun read() -> Int = person.name.length
    return before
scene start():
    Unit
"#,
    )
    .unwrap_err();
    assert!(error.contains("nullable member receiver"), "{error}");
}

#[test]
fn converted_when_subjects_do_not_retype_the_original_value() {
    let module = compile(
        r#"fun numeric():
    val original = 1
    when val captured: Float = original:
        case is Float:
            val checked: Float = original + 0
            return original
        else:
            return 0
fun character():
    val original = 'x'
    when val captured: String = original:
        case is String:
            return original.code
        else:
            return 0
fun copied():
    val original = mutable [1]
    when val captured: List<Int> = original:
        case is List<Int>:
            original.add(2)
            return original.size
        else:
            return 0
scene start():
    Unit
"#,
    )
    .unwrap();
    for name in ["test.numeric", "test.character", "test.copied"] {
        assert_eq!(module.functions[name].signature.result, Type::Int);
    }
}

#[test]
fn immutable_field_when_subjects_support_type_cases_and_null_remainders() {
    let module = compile(
        r#"data class Box<T>:
    val value: T
fun length(holder: Box<String?>):
    when holder.value:
        case null:
            return 0
        else:
            return holder.value.length
fun remainder(holder: Box<String?>):
    when holder.value:
        case null:
            return 0
    return holder.value.length
fun typed(holder: Box<Any?>) = when holder.value:
    case is String if holder.value.length > 0:
        holder.value.length
    else:
        0
fun asserted(holder: Box<String?>):
    when holder.value!!:
        case "Ada":
            return holder.value.length
        else:
            return holder.value.length
scene start():
    Unit
"#,
    )
    .unwrap();
    for name in ["test.length", "test.remainder", "test.typed", "test.asserted"] {
        assert_eq!(module.functions[name].signature.result, Type::Int);
    }
}

#[test]
fn immutable_when_fields_remain_stable_across_suspending_case_tests() {
    compile(
        r#"data class Box<T>:
    val value: T
suspend fun choose(holder: Box<String?>, test: suspend () -> String):
    return when holder.value:
        case "unmatched", test():
            holder.value.length
        case null:
            0
        else:
            holder.value.length
scene start():
    Unit
"#,
    )
    .unwrap();
}

#[test]
fn when_tests_cannot_refine_mutable_fields_changed_by_a_case_expression() {
    let error = compile(
        r#"data class Box:
    var value: String?
fun mutate(holder: Box) -> String:
    holder.value = null
    return "Ada"
fun invalid(holder: Box) -> Int:
    when holder.value:
        case mutate(holder):
            return holder.value.length
        else:
            return 0
scene start():
    Unit
"#,
    )
    .unwrap_err();
    assert!(error.contains("nullable member receiver"), "{error}");
}

#[test]
fn map_entry_snapshots_expose_immutable_key_and_value_fields() {
    let module = compile(
        r#"fun length(entry: Entry<String?, String?>):
    if entry.value == null:
        return 0
    when entry.key:
        case null:
            return entry.value.length
        else:
            return entry.key.length + entry.value.length
fun <T> original(entry: Entry<String, T>) -> T:
    if entry.value is String:
        return entry.value!!
    return entry.value
fun describe(entry: Entry<String, Int>) -> String = entry.toString()
scene start():
    val items: MutableMap<String?, String?> = mutable {null: "zero", "a": "Ada"}
    for entry in items:
        if entry.value == null:
            continue
        items[entry.key] = null
        val size = entry.value.length
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.length"].signature.result, Type::Int);
}

#[test]
fn map_entry_key_and_value_bindings_cannot_be_updated() {
    for (member, replacement) in [("key", "\"other\""), ("value", "null")] {
        let source = format!(
            r#"fun invalid(entry: Entry<String, String?>):
    entry.{member} = {replacement}
scene start():
    Unit
"#
        );
        let error = compile(&source).expect_err("snapshot entry fields remain immutable");
        assert!(error.contains("cannot assign to val field"), "{error}\n{source}");
    }
}
