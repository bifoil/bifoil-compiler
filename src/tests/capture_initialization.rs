//! Known writers establish definite initialization only on every normal return path.

use super::compile;

#[test]
fn mandatory_primitive_capture_writes_establish_caller_initialization() {
    for source in [
        r#"scene start():
    var x: Int
    fun initialize():
        x = 3
    initialize()
    prose:
        ${x}
"#,
        r#"scene start():
    var x: Int
    fun initialize(flag: Boolean) -> Int:
        if flag:
            x = 1
        else:
            x = 2
        return 7
    val answer = initialize(true)
    prose:
        ${x}|${answer}
"#,
        r#"scene start():
    var x: Int
    fun initialize():
        do:
            x = 3
        while false
    initialize()
    prose:
        ${x}
"#,
        r#"scene start():
    var x: Int
    fun initializeAndStop() -> Boolean:
        x = 4
        return false
    while initializeAndStop():
        Unit
    prose:
        ${x}
"#,
        r#"scene start():
    var x: String?
    fun initialize():
        x = null
    initialize()
    prose:
        ${x == null}
"#,
    ] {
        compile(source).unwrap();
    }
}

#[test]
fn skipped_calls_and_early_returns_do_not_claim_initialization() {
    for source in [
        r#"scene start():
    var x: Int
    fun initialize(flag: Boolean):
        if flag:
            return
        x = 3
    initialize(true)
    prose:
        ${x}
"#,
        r#"scene start():
    var x: Int
    fun initialize(flag: Boolean):
        while flag:
            x = 3
    initialize(false)
    prose:
        ${x}
"#,
        r#"scene start():
    var x: Int
    fun initialize() -> Boolean:
        x = 3
        return true
    val ignored = false && initialize()
    prose:
        ${x}
"#,
        r#"scene start():
    var x: Int
    fun initialize(flag: Boolean):
        x = if flag:
            return
        else:
            3
    initialize(true)
    prose:
        ${x}
"#,
        r#"scene start():
    fun initialize():
        var x: Int
        x = 3
    var x: Int
    initialize()
    prose:
        ${x}
"#,
    ] {
        let error = compile(source).unwrap_err();
        assert!(
            error.contains("binding 'x' is not definitely initialized"),
            "{source}: {error}"
        );
    }
}

#[test]
fn ready_callback_cells_initialize_while_captured_vals_keep_their_guards() {
    compile(
        r#"scene start():
    var callback: () -> Int
    fun initialize():
        callback = () => 7
    initialize()
    prose:
        ${callback()}
"#,
    )
    .unwrap();
    compile(
        r#"scene start():
    val x: Int
    fun initialize():
        x = 3
    initialize()
    prose:
        ${x}
"#,
    )
    .unwrap();
}

#[test]
fn every_choice_return_can_initialize_a_captured_primitive() {
    compile(
        r#"export suspend fun run() -> Int:
    var answer: Int
    suspend fun initialize():
        choice:
            option "First":
                answer = 5
            option "Second":
                answer = 7
    initialize()
    return answer
"#,
    )
    .unwrap();
}

#[test]
fn large_writers_keep_facts_per_straight_region_instead_of_per_instruction() {
    let expression = std::iter::repeat_n("1", 8).collect::<Vec<_>>().join(" + ");
    for branching in [false, true] {
        let mut source = String::from("scene start():\n");
        for number in 0..3000 {
            source += &format!("    var x{number:04}: Int\n");
        }
        if branching {
            source += "    fun initialize(flag: Boolean) -> Unit:\n        if flag:\n";
            for number in 0..3000 {
                source += &format!("            x{number:04} = {expression}\n");
            }
            source += "        else:\n";
            for number in 0..3000 {
                source += &format!("            x{number:04} = {expression}\n");
            }
            source += "    initialize(true)\n";
        } else {
            source += "    fun initialize() -> Unit:\n";
            for number in 0..3000 {
                source += &format!("        x{number:04} = {expression}\n");
            }
            source += "    initialize()\n";
        }
        source += "    prose:\n        ${x0000}|${x2999}\n";
        let module = compile(&source).unwrap();
        assert_eq!(module.functions.len(), 2);
    }
}

#[test]
fn captured_initialization_supports_structures_collections_and_scene_values() {
    compile(
        r#"data class Box:
    val value: Int
scene start():
    var box: Box
    var items: List<() -> Int>
    var optional: (() -> Int)?
    var destination: Scene
    fun initialize():
        val ready = 9
        box = Box(ready)
        items = [() => ready]
        optional = null
        destination = ::start
    initialize()
    val saved: Scene = destination
    val missing = optional == null
    val total = box.value + items[0]()
"#,
    )
    .unwrap();
}

#[test]
fn proven_overwrites_retire_only_the_targets_old_deferred_requirements() {
    compile(
        r#"scene start():
    var future: Int
    var callback: () -> Int = () => future
    fun initialize():
        val ready = 9
        callback = () => ready
    initialize()
    val value = callback()
"#,
    )
    .unwrap();
    for tail in ["val value = previous()", "val value = callback()"] {
        let setup = if tail.contains("previous") {
            "    val previous = callback\n    initialize()\n"
        } else {
            "    val skipped = false && initialize()\n"
        };
        let source = format!(
            r#"scene start():
    var future: Int
    var callback: () -> Int = () => future
    fun initialize() -> Boolean:
        callback = () => 9
        return true
{setup}    {tail}
"#
        );
        let error = compile(&source).unwrap_err();
        assert!(error.contains("not initialized"), "{error}\n{source}");
    }
}
