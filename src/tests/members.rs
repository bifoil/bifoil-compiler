//! Implicit receivers, member-name conflicts, and generated copy access.

use super::compile;

#[test]
fn lexical_bindings_cannot_hide_implicit_instance_members() {
    let methods = [
        r#"fun replace(value: Int) -> Int:
        this.value = value
        return this.value"#,
        r#"fun read() -> Int:
        val value = 7
        return value"#,
        r#"fun read() -> Int:
        var value: Int
        value = 7
        return value"#,
        r#"fun read() -> Int:
        fun value() -> Int = 7
        return value()"#,
        r#"fun action() -> (Int) -> Int = (value: Int) => value"#,
        r#"fun read():
        for value in 1..2:
            Unit"#,
        r#"fun read():
        when val value = 7:
            case 7:
                Unit"#,
        r#"fun read(item: Outcome):
        when item:
            case is Outcome.Success(value):
                Unit"#,
    ];
    for method in methods {
        let source = r#"sealed class Outcome:
    case Success(item: Int)
data class Counter:
    var value: Int
    METHOD
scene start():
    Unit
"#
        .replace("METHOD", method);
        let error = compile(&source).expect_err("member hiding must not silently change reads or writes");
        assert!(
            error.contains("shadows an implicit instance member"),
            "{error}\n{source}"
        );
    }
}

#[test]
fn implicit_members_and_generated_copies_remain_available_to_nested_functions() {
    compile(
        r#"sealed class Outcome:
    case Success(item: Int)
data class Counter:
    var value: Int
    fun replace(next: Int) -> Int:
        value = next
        return value
    fun duplicate() -> Counter = copy()
    fun reader() -> () -> Int:
        fun read() -> Int = value
        return ::read
    fun inspect(item: Outcome):
        when item:
            case is Outcome.Success(_):
                Unit
scene start():
    val counter = Counter(3)
    val changed = counter.replace(7)
    val other = counter.duplicate()
    val reader = other.reader()
    reader()
"#,
    )
    .unwrap();
}

#[test]
fn local_declarations_cannot_hide_an_instance_method() {
    let error = compile(
        r#"data class Counter:
    var value: Int
    fun read() -> Int = value
    fun action():
        val read = () => 7
        read()
scene start():
    Unit
"#,
    )
    .unwrap_err();
    assert!(
        error.contains("declaration 'read' shadows an implicit instance member"),
        "{error}"
    );
}

#[test]
fn map_key_members_accept_named_keys_and_typed_bound_references() {
    compile(
        r#"enum Mood:
    case Happy
fun <K: StableKey, V> present(mapping: Map<K, V>, key: K) -> Boolean = mapping.containsKey(key = key)
scene start():
    val mapping: Map<Float, String?> = {1: null}
    val exists: Boolean = mapping.containsKey(key = 1)
    val function: (Float) -> Boolean = mapping::containsKey
    val answer = function(1)
    val moods: Map<Mood, Int> = {Mood.Happy: 7}
    val known: Boolean = moods.containsKey(key = Happy)
    val mutableMapping: MutableMap<Float, String?> = {1: null}
    val inferred = present(mutableMapping.toMap(), 1)
    val explicit = present<Float, String?>(mutableMapping, 1)
"#,
    )
    .unwrap();
}

#[test]
fn map_key_member_names_do_not_leak_to_lists_sets_or_function_values() {
    for (body, diagnostic) in [
        (
            "val values = [1]\n    values.containsKey(1)",
            "no accessible member 'containsKey'",
        ),
        (
            "val values = {1}\n    values.containsKey(1)",
            "no accessible member 'containsKey'",
        ),
        (
            "val mapping = {1: 2}\n    mapping.containsKey(element = 1)",
            "unknown argument 'element'",
        ),
        (
            "val mapping = {1: 2}\n    val callable = mapping::containsKey\n    callable(key = 1)",
            "do not expose parameter names",
        ),
    ] {
        let error = compile(&format!("scene start():\n    {body}\n")).unwrap_err();
        assert!(error.contains(diagnostic), "{error}\n{body}");
    }
}

#[test]
fn collection_updates_retain_typed_results_and_entry_fields() {
    compile(
        r#"scene start():
    val values: MutableList<Int> = [1, 2]
    val added: Boolean = values.add(element = 3)
    val inserted: Unit = values.insert(index = 1, element = 4)
    val removed: Int = values.removeAt(index = 0)
    val found: Boolean = values.remove(element = 2)
    val copy: List<Int> = values.toList()
    val emptied: Unit = values.clear()
    val mapping: MutableMap<String, Int?> = {"a": null}
    val old: Int? = mapping.put(key = "a", value = 3)
    val missing: Int? = mapping.remove(key = "b")
    for entry in mapping:
        val key: String = entry.key
        val item: Int? = entry.value
        val retained: Entry<String, Int?> = entry
"#,
    )
    .unwrap();
}

#[test]
fn map_entry_fields_remain_immutable_and_container_mutation_is_checked() {
    for (body, diagnostic) in [
        (
            "val mapping = {1: 2}\n    for entry in mapping:\n        entry.value = 3",
            "cannot assign to val field",
        ),
        (
            "val mapping = {1: 2}\n    for entry in mapping:\n        entry.key = 3",
            "cannot assign to val field",
        ),
        ("val values = [1]\n    values.add(2)", "no accessible member"),
        (
            "val values: MutableList<Int> = [1]\n    values.insert(0, \"wrong\")",
            "argument",
        ),
    ] {
        let error = compile(&format!("scene start():\n    {body}\n")).unwrap_err();
        assert!(error.contains(diagnostic), "{error}\n{body}");
    }
}
