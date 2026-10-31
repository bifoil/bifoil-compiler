//! Immutable fields may be narrowed while a reassignable root stays unchanged.

use super::compile;
use crate::model::Type;

#[test]
fn reassignable_roots_support_local_guards_assertions_and_when_results() {
    let module = compile(
        r#"data class Box<T>:
    val value: T
fun length(var holder: Box<String?>):
    if holder.value == null:
        return 0
    return holder.value.length
fun checked(var holder: Box<Any?>):
    if holder.value !is String:
        return 0
    holder.value!!
    return holder.value.length
fun nested(var holder: Box<Box<String?>?>):
    if holder.value == null:
        return 0
    if holder.value.value == null:
        return 0
    return holder.value.value.length
fun match(var holder: Box<String?>) = when holder.value:
    case null:
        0
    else:
        holder.value.length
fun <T> original(var holder: Box<T>) -> T:
    if holder.value is String:
        return holder.value!!
    return holder.value
scene start():
    Unit
"#,
    )
    .unwrap();
    for name in ["test.length", "test.checked", "test.nested", "test.match"] {
        assert_eq!(module.functions[name].signature.result, Type::Int);
    }
}

#[test]
fn calls_closures_loops_and_prepared_choices_cannot_retain_mutable_root_facts() {
    let sources = [
        r#"fun invalid(var person: Person) -> Int:
    fun change() -> Boolean:
        person = Person(null)
        return true
    if person.name != null && change():
        return person.name.length
    return 0
"#,
        r#"fun invalid(var person: Person) -> () -> Int:
    if person.name == null:
        return () => 0
    return () => person.name.length
"#,
        r#"suspend fun pause():
    choice:
        option "Continue":
            Unit
suspend fun invalid(var person: Person) -> Int:
    if person.name == null:
        return 0
    pause()
    return person.name.length
"#,
        r#"fun invalid(var person: Person) -> Int:
    if person.name == null:
        return 0
    while true:
        person = Person(null)
        return person.name.length
"#,
        r#"fun invalid(var person: Person) -> Int:
    fun change() -> String:
        person = Person(null)
        return "Ada"
    when person.name:
        case change():
            return person.name.length
        else:
            return 0
"#,
        r#"suspend fun invalid(var person: Person) -> Int:
    fun change() -> Boolean:
        person = Person(null)
        return false
    choice:
        option "Read" if person.name != null:
            return person.name.length
        option "Prepare" if change():
            return 0
"#,
    ];
    for source in sources {
        let source = format!("data class Person:\n    val name: String?\n{source}scene start():\n    Unit\n");
        let error = compile(&source).expect_err("the root can change after its field was checked");
        assert!(error.contains("nullable member receiver"), "{error}\n{source}");
    }
}

#[test]
fn when_subject_binding_keeps_its_snapshot_after_the_original_root_changes() {
    let module = compile(
        r#"data class Person:
    val name: String?
fun length(var person: Person):
    fun change() -> String:
        person = Person(null)
        return "Ada"
    when val captured = person.name:
        case change():
            return captured.length
        else:
            return 0
scene start():
    Unit
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.length"].signature.result, Type::Int);
}
