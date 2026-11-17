//! Nested persistent cells are initialized once, without capturing call state.
use super::compile;
use crate::model::Op;

#[test]
fn nested_shared_sites_have_global_initializers_and_no_local_declarations() {
    let module = compile(
        r#"fun counter() -> Int:
    shared var value = 0
    value += 1
    return value
fun unused():
    if false:
        shared val hidden = 3
"#,
    )
    .unwrap();
    assert_eq!(module.shared.len(), 2);
    assert!(module.shared.keys().all(|name| name.starts_with("test.$scoped:")));
    assert!(
        !module.functions["test.counter"]
            .code
            .iter()
            .any(|instruction| matches!(&instruction.op,Op::Declare { name, .. } if name=="value"))
    );
}

#[test]
fn scoped_shared_cells_are_independent_of_enclosing_generic_instances() {
    let module = compile(
        r#"fun <T> count(value: T) -> Int:
    shared var count = 0
    count += 1
    return count
"#,
    )
    .unwrap_err();
    assert!(module.contains("shadowing"), "{module}");
    let module = compile(
        r#"fun <T> count(value: T) -> Int:
    shared var total = 0
    total += 1
    return total
"#,
    )
    .unwrap();
    assert_eq!(module.shared.len(), 1);
}

#[test]
fn scoped_shared_initializers_reject_enclosing_values_and_type_parameters() {
    for source in [
        "fun bad(input: Int):\n    shared val stored = input\n",
        "fun bad():\n    val local = 3\n    shared val stored = local\n",
        "fun bad():\n    fun helper() = 3\n    shared val stored = helper()\n",
        "fun <T> bad():\n    shared val stored: List<T> = []\n",
        "fun <T> bad():\n    typealias Hidden = T\n    shared val stored: List<Hidden> = []\n",
        "data class Holder:\n    val value: Int\n    fun bad():\n        shared val stored = value\n",
        "data class Holder<T>:\n    val value: T\n    fun bad():\n        shared val stored: List<T> = []\n",
    ] {
        let error = compile(source).unwrap_err();
        assert!(
            error.contains("nested shared initializer cannot use enclosing"),
            "{source}\n{error}"
        );
    }
}

#[test]
fn nested_shared_initializers_can_use_earlier_shared_cells_and_module_helpers() {
    compile(
        r#"fun <T> identity(value: T) -> T = value
fun values() -> Int:
    shared val first = identity(3)
    shared val second = first + 2
    return second
"#,
    )
    .unwrap();
}

#[test]
fn nested_shared_storage_has_ordinary_lexical_visibility() {
    for source in [
        "fun bad():\n    prose:\n        ${stored}\n    shared val stored = 3\n",
        "fun bad():\n    if true:\n        shared val stored = 3\n    return stored\n",
        "fun first():\n    shared val stored = 3\nfun second() = stored\n",
    ] {
        let error = compile(source).unwrap_err();
        assert!(error.contains("unknown"), "{source}\n{error}");
    }
    compile(
        "fun valid():\n    if true:\n        shared val stored = 3\n    if false:\n        shared val stored = 4\n",
    )
    .unwrap();
}

#[test]
fn nested_shared_initialization_rejects_cycles_suspension_and_reassignment_to_val() {
    for source in [
        "fun recurse() -> Int:\n    shared val stored = recurse()\n    return stored\n",
        "suspend fun ask() -> Int:\n    return 3\nfun bad():\n    shared val stored = ask()\n",
        "fun bad():\n    shared val stored = 3\n    stored = 4\n",
        "fun bad():\n    shared val stored: Int\n",
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
}

#[test]
fn lifting_initializers_preserves_outer_no_shadowing_checks() {
    let error = compile(
        r#"fun bad(input: Int):
    shared val stored = (input: Int) => input
"#,
    )
    .unwrap_err();
    assert!(error.contains("shadowing"), "{error}");
    compile(
        r#"fun valid(input: Int):
    shared val stored = (other: Int) => other
"#,
    )
    .unwrap();
}

#[test]
fn lifted_when_subjects_cannot_shadow_enclosing_bindings() {
    let error = compile(
        r#"fun bad(input: Int):
    shared val callback = (item: Int) =>:
        return when val input = item:
            case 1:
                1
            else:
                0
"#,
    )
    .unwrap_err();
    assert!(error.contains("shadowing or duplicate declaration 'input'"), "{error}");
    assert!(error.contains("src/test.bfl:3:"), "{error}");
}
