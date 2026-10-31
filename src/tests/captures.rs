//! Lexical captures and initialization using original Bifoil programs.

use super::compile;
use crate::verify;

#[test]
fn forward_capture_reads_require_initialization() {
    assert!(
        compile(
            r#"scene start():
    readX()
    val x = 1
    fun readX() -> Int = x
"#
        )
        .unwrap_err()
        .contains("initialized")
    );
    assert!(
        compile(
            r#"scene start():
    first()
    val x = 1
    fun first() -> Int = second()
    fun second() -> Int = x
"#
        )
        .unwrap_err()
        .contains("initialized")
    );
    compile(
        r#"scene start():
    val x = 1
    val reader = ::readX
    fun readX() -> Int = x
    reader()
"#,
    )
    .unwrap();
}
#[test]
fn closure_visibility_and_types() {
    compile(
        r#"scene start():
    var count = 0
    fun bump() -> Int:
        count += 1
        return count
    val action: () -> Int = ::bump
    action()
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"scene start():
    val x = 1
    if true:
        val x = 2
"#
        )
        .unwrap_err()
        .contains("shadowing")
    );
}
#[test]
fn callbacks_cannot_escape_uninitialized_captures() {
    let programs = [
        r#"fun invoke(callback: () -> Int) -> Int = callback()
scene start():
    val x: Int
    invoke(() => x)
"#,
        r#"fun factory() -> () -> Int:
    val x: Int
    return () => x
"#,
        r#"scene start():
    val x: Int
    var reader: () -> Int = () => 0
    if true:
        reader = () => x
    reader()
"#,
        r#"scene start():
    val x: Int
    val reader = if true:
        () => x
    else:
        () => 0
    reader()
"#,
        r#"scene start():
    val x: Int
    val readers = [() => x]
    readers[0]()
"#,
    ];
    for program in programs {
        let error = compile(program).expect_err(program);
        assert!(error.contains("initialized"), "{error}\n{program}");
    }
}
#[test]
fn captured_vals_cannot_be_initialized_by_repeatable_functions() {
    let error = compile(
        r#"scene start():
    val x: Int
    fun set():
        x = 1
    x = 2
    set()
"#,
    )
    .unwrap_err();
    assert!(error.contains("captured val"), "{error}");
}
#[test]
fn capture_proofs_survive_branching_casts_and_containers() {
    let programs = [
        r#"fun factory() -> Any:
    val x: Int
    return (() => x) as Any
"#,
        r#"fun run(flag: Boolean):
    val x: Int
    var reader: () -> Int = () => 0
    if flag:
        reader = () => x
    reader()
"#,
        r#"scene start():
    val x: Int
    val readers = [() => x]
    for reader in readers:
        reader()
"#,
        r#"scene start():
    val reader = if true:
        val x: Int
        () => x
    else:
        () => 0
    reader()
"#,
        r#"fun run(flag: Boolean):
    val x: Int
    var reader: () -> Int = () => 0
    while flag:
        reader()
        reader = () => x
"#,
        r#"data class Box:
    var callback: () -> Int
scene start():
    val box = Box(() => 0)
    val x: Int
    box.callback = () => x
"#,
    ];
    for program in programs {
        let error = compile(program).expect_err(program);
        assert!(error.contains("initialized"), "{error}\n{program}");
    }
}
#[test]
fn captures_can_be_initialized_before_invocation_or_scope_exit() {
    compile(
        r#"fun invoke(callback: () -> Int, ignored: Int) -> Int = callback()
scene start():
    val x: Int
    val reader = if true:
        () => x
    else:
        () => 0
    val readers = [reader]
    for pause in 1..2:
        val visited = pause
    val answer = invoke(readers[0], if true:
        x = 7
        0
    else:
        x = 8
        0
    )
    var saved: () -> Int = () => 0
    if true:
        val local: Int
        saved = () => local
        local = 9
    saved()
"#,
    )
    .unwrap();
}
#[test]
fn when_bindings_and_expression_returns_preserve_capture_checks() {
    for program in [
        r#"scene start():
    val x: Int
    when val reader = () => x:
        else:
            reader()
"#,
        r#"fun factory() -> () -> Int:
    val x: Int
    val ready: Int? = null
    val value = ready ?: return () => x
    return () => value
"#,
        r#"fun invoke(callback: () -> Int, ignored: Int) -> Int = callback()
scene start():
    val x: Int
    var callback: () -> Int = () => x
    invoke(callback, if true:
        callback = () => 0
        0
    else:
        0
    )
"#,
    ] {
        let error = compile(program).expect_err(program);
        assert!(error.contains("initialized"), "{error}\n{program}");
    }
}
#[test]
fn qualified_captures_resolve_symbols_and_retire_local_receivers() {
    compile(
        r#"enum Mood:
    Happy
    Sad
data class Box:
    val value: Int
scene start():
    val constant = ::happy
    fun happy() -> Mood = Mood.Happy
    val mood = constant()
    var reader: () -> Int = () => 0
    if true:
        val box = Box(4)
        reader = () => box.value
    reader()
"#,
    )
    .unwrap();
    let error = compile(
        r#"data class Box:
    val value: Int
scene start():
    val holder: Box
    val reader = () => holder.value
    reader()
"#,
    )
    .unwrap_err();
    assert!(error.contains("initialized"), "{error}");
}
#[test]
fn captured_mutable_smart_casts_are_not_frozen_at_declaration() {
    for source in [
        r#"scene start():
    var name: String? = "Ada"
    if name != null:
        val reader = () => name.length
        name = null
        reader()
"#,
        r#"scene start():
    var name: String? = "Ada"
    if name != null:
        fun reader() -> Int = name.length
        name = null
        reader()
"#,
    ] {
        let error = compile(source).expect_err(source);
        assert!(error.contains("nullable"), "{error}");
    }
}
#[test]
fn captured_mutable_reads_infer_the_declared_type_and_immutable_proofs_survive() {
    compile(
        r#"scene start():
    var value: Any = "Ada"
    if value is String:
        val reader = () => value
        value = 7
        val observed = reader()
        val integer: Int = observed as Int
    val fixed: String? = "Ada"
    if fixed != null:
        val length = () => fixed.length
        length()
    var name: String? = "Ada"
    val safe = () =>:
        if name != null:
            name.length
        else:
            0
    name = null
    safe()
"#,
    )
    .unwrap();
}
#[test]
fn anonymous_capture_metadata_names_only_referenced_runtime_bindings() {
    let module = compile(
        r#"data class Box:
    val value: Int
scene start():
    val box = Box(4)
    val unused = 9
    val reader = () => box.value
    val constant = () => 1
"#,
    )
    .unwrap();
    let captures = module.functions["test.start"]
        .code
        .iter()
        .filter_map(|instruction| match &instruction.op {
            crate::model::Op::FunctionValue { captures, .. } => Some(captures.clone().unwrap()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(captures, vec![vec!["box".to_string()], vec![]]);
    let serialized = serde_json::to_string(&module).unwrap();
    let mut restored: crate::model::Module = serde_json::from_str(&serialized).unwrap();
    verify(&mut restored).unwrap();
}

#[test]
fn hoisted_functions_cannot_assume_later_immutable_proofs() {
    for declaration in [
        "fun read() -> Int = name.length",
        "fun read(value: Int = name.length) -> Int = value",
    ] {
        let source = r#"fun bypass(name: String?) -> Int:
    val before = read()
    if name == null:
        return 0
    DECLARATION
    return before
scene start():
    Unit
"#
        .replace("DECLARATION", declaration);
        let error = compile(&source).expect_err("an earlier caller can bypass the later guard");
        assert!(error.contains("nullable member receiver"), "{error}");
    }
}

#[test]
fn hoisted_functions_retain_block_entry_proofs_and_lambdas_keep_creation_proofs() {
    compile(
        r#"fun guarded(name: String?) -> Int:
    if name != null:
        val before = read()
        fun read() -> Int = name.length
        return before
    return 0
fun callback(name: String?) -> () -> Int:
    if name == null:
        return () => 0
    return () => name.length
scene start():
    val a: Int = guarded("Ada")
    val b: Int = callback("four")()
"#,
    )
    .unwrap();
}

#[test]
fn supplied_defaults_and_function_values_skip_default_only_captures() {
    compile(
        r#"scene start():
    val direct = read(5)
    val reader: (Int) -> Int = ::read
    val through = reader(6)
    choice:
        option "Initialize":
            Unit
    val seed: Int
    fun read(value: Int = seed) -> Int = value
    seed = 7
    val defaulted = read()
"#,
    )
    .unwrap();
}

#[test]
fn skipped_defaults_do_not_remove_body_or_transitive_capture_checks() {
    for declaration in [
        r#"fun read(value: Int = seed) -> Int = seed + value
    val result = read(5)"#,
        r#"fun read(value: Int = seed) -> Int = value
    fun wrapper() -> Int = read()
    val result = wrapper()"#,
    ] {
        let source = r#"scene start():
    val seed: Int
    DECLARATION
    seed = 7
"#
        .replace("DECLARATION", declaration);
        let error = compile(&source).expect_err("these calls may actually read the captured binding");
        assert!(error.contains("not initialized"), "{error}");
    }
}

#[test]
fn enum_shorthand_is_not_a_lexical_capture() {
    compile(
        r#"enum Mood:
    Happy
    Sad
scene start():
    val before = pick()
    val callback: (Mood) -> Mood = ::pick
    fun pick(value: Mood = Happy) -> Mood:
        fun answer() -> Mood = Happy
        return answer()
    val after = callback(Mood.Happy)
    val block: Mood = if true:
        fun choose() -> Mood = Sad
        choose()
    else:
        Happy
"#,
    )
    .unwrap();
}

#[test]
fn enum_capture_classification_preserves_lexical_initialization_and_conflicts() {
    let uninitialized = compile(
        r#"enum Mood:
    Happy
scene start():
    val result = pick()
    val Happy = Mood.Happy
    fun pick() -> Mood = Happy
"#,
    )
    .unwrap_err();
    assert!(uninitialized.contains("initialized"), "{uninitialized}");
    let conflict = compile(
        r#"enum Mood:
    Happy
scene start():
    val Happy = Mood.Happy
    fun pick() -> Mood = Happy
    val result = pick()
"#,
    )
    .unwrap_err();
    assert!(conflict.contains("conflicts with a visible binding"), "{conflict}");
    let unknown = compile(
        r#"scene start():
    fun pick() -> Int = missing
    val result = pick()
"#,
    )
    .unwrap_err();
    assert!(unknown.contains("unknown"), "{unknown}");
}
