//! Private access follows the enclosing declaration, not a user-chosen name.

use super::compile;

#[test]
fn private_members_are_available_across_instantiations_of_the_same_class() {
    compile(
        r#"data class Box<T>:
    private var value: T
    private fun reveal() -> T = value
    fun read(other: Box<Int>) -> Int = other.value
    fun inferred(other: Box<Int>) = other.value
    fun write(other: Box<Int>, next: Int):
        other.value = next
    fun call(other: Box<Int>) -> Int = other.reveal()
    fun reference(other: Box<Int>) -> () -> Int = other::reveal
    fun nested(other: Box<Int>) -> () -> Int:
        fun readNested() = other.value
        return ::readNested
scene start():
    val owner = Box("owner")
    val target = Box(3)
    owner.write(target, 7)
    val reader = owner.reference(target)
    val nestedReader = owner.nested(target)
    reader() + nestedReader()
"#,
    )
    .unwrap();
}

#[test]
fn ordinary_this_bindings_do_not_grant_private_access() {
    let declarations = r#"data class Box:
    private val value: Int
    private fun reveal() -> Int = value
"#;
    for body in [
        "fun read(this: Box) -> Int = this.value",
        "fun read(this: Box) = this.value",
        "fun read(this: Box) -> Int = this.reveal()",
        "fun read(this: Box) -> () -> Int = this::reveal",
        "scene start():\n    val this = Box(3)\n    val result = this.value",
    ] {
        let source = format!("{declarations}{body}\n");
        let error = compile(&source).expect_err("a source-level name cannot grant private access");
        assert!(error.contains("private"), "{error}\n{source}");
    }
}

#[test]
fn a_genuine_receiver_from_another_class_has_no_private_permission() {
    let source = r#"data class Box:
    private val value: Int
data class Reader:
    fun read(other: Box) -> Int = other.value
scene start():
    Unit
"#;
    assert!(compile(source).unwrap_err().contains("private"));
}
