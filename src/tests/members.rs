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
