//! Borrowed type-name views preserve lexical scope and declaration identities.

use super::compile;
use crate::model::Type;

#[test]
fn sibling_functions_keep_independent_local_aliases() {
    let module = compile(
        r#"typealias Number = Int
fun left(value: Number):
    typealias Local = String
    fun describe(input: Local) -> Local = input
    return describe("A") + value
fun right(value: Number):
    typealias Local = Float
    val measured: Local = value
    return measured
scene start():
    Unit
"#,
    )
    .unwrap();
    assert_eq!(module.functions["test.left"].signature.result, Type::String);
    assert_eq!(module.functions["test.right"].signature.result, Type::Float);
}

#[test]
fn sibling_branches_keep_their_own_alias_declarations() {
    compile(
        r#"fun describe(flag: Boolean) -> String:
    if flag:
        typealias Local = Int
        val first: Local = 1
        return "" + first
    else:
        typealias Local = String
        val second: Local = "z"
        return second
scene start():
    Unit
"#,
    )
    .unwrap();
}

#[test]
fn borrowed_parent_views_do_not_leak_names_or_allow_generic_shadowing() {
    for source in [
        "fun left() -> Int:\n    typealias Hidden = Int\n    return 1\nfun right(value: Hidden) -> Int = value\n",
        "typealias T = Int\nfun <T> invalid(value: T) -> T = value\n",
        "typealias Name = Int\nfun invalid() -> Int:\n    typealias Name = String\n    return 1\n",
    ] {
        assert!(compile(source).is_err());
    }
}

#[test]
fn generic_class_method_and_local_function_names_keep_their_owners() {
    compile(
        r#"data class Pair<out Left, out Right>:
    val left: Left
    val right: Right
data class Holder<T>:
    val value: T
    fun <U> pair(other: U) -> Pair<T, U>:
        fun <V> identity(input: V) -> V = input
        typealias Element = U
        val retained: Element = identity(other)
        return Pair(value, retained)
scene start():
    val result = Holder("Kept").pair(7)
"#,
    )
    .unwrap();
}

#[test]
fn retained_local_aliases_keep_independent_function_parameters() {
    compile(
        r#"fun <T> first(input: T) -> T:
    typealias Local = T
    val retained: Local = input
    return retained
fun <T> second(input: T) -> T:
    typealias Local = T
    val retained: Local = input
    return retained
scene start():
    val text = first("A")
    val number = second(7)
"#,
    )
    .unwrap();
}

#[test]
fn generic_child_frames_reject_visible_parameter_shadowing() {
    for source in [
        "data class Holder<T>:\n    val value: T\n    fun <T> invalid(input: T) -> T = input\n",
        "fun <T> outer(input: T) -> T:\n    fun <T> invalid(value: T) -> T = value\n    return input\n",
    ] {
        assert!(compile(source).unwrap_err().contains("generic parameter shadows"));
    }
}
