//! Reachability, narrowing, and initialization using original Bifoil programs.

use super::compile;

#[test]
fn immutable_initialization_tracks_possible_writes_across_control_flow() {
    for body in [
        r#"    if flag:
        result = 1
    result = 2
"#,
        r#"    when flag:
        case true:
            result = 1
    result = 2
"#,
        r#"    do:
        if flag:
            result = 1
    while false
    result = 2
"#,
        r#"    result = if flag:
        result = 1
        2
    else:
        3
"#,
    ] {
        let source = format!(
            r#"fun invalid(flag: Boolean):
    val result: Int
{body}"#
        );
        assert!(
            compile(&source)
                .unwrap_err()
                .contains("cannot reassign initialized val")
        );
    }
    for tail in [
        r#"    continue
"#,
        "    Unit\n",
    ] {
        let source = format!(
            r#"fun invalid(flag: Boolean):
    val result: Int
    while flag:
        if flag:
            result = 1
    {tail}"#
        );
        assert!(
            compile(&source)
                .unwrap_err()
                .contains("could be initialized more than once")
        );
    }
    compile(
        r#"fun valid(flag: Boolean) -> Int:
    val result: Int
    if flag:
        result = 1
    else:
        result = 2
    return result
"#,
    )
    .unwrap();
    compile(
        r#"fun valid(flag: Boolean):
    val result: Int
    if flag:
        result = 1
        return
    result = 2
"#,
    )
    .unwrap();
    compile(
        r#"fun valid(flag: Boolean) -> Int:
    var result: Int
    if flag:
        result = 1
    result = 2
    return result
"#,
    )
    .unwrap();
}
#[test]
fn skipped_safe_arguments_cannot_initialize_bindings_or_hide_reachable_paths() {
    let branch = r#"        if true:
            result = 1
            0
        else:
            result = 2
            0
"#;
    for (parameter, operation) in [
        (
            "items: List<Int>?",
            format!(
                r#"    val ignored = items?[
{branch}    ]
"#
            ),
        ),
        (
            "callback: ((Int) -> Unit)?",
            format!(
                r#"    callback?.(
{branch}    )
"#
            ),
        ),
        (
            "receiver: Receiver?",
            format!(
                r#"    receiver?.accept(
{branch}    )
"#
            ),
        ),
    ] {
        let declarations = r#"data class Receiver:
    fun accept(value: Int):
        return
"#;
        let source = format!(
            r#"{declarations}fun invalid({parameter}) -> Int:
    val result: Int
{operation}    return result
"#
        );
        let error = compile(&source).unwrap_err();
        assert!(
            error.contains("not definitely initialized"),
            r#"{error}
{source}"#
        );
    }
    let source = r#"data class Receiver:
    fun accept(value: Int):
        return
fun invalid(receiver: Receiver?) -> Int:
    receiver?.accept(
        if true:
            return 1
        else:
            return 2
    )
"#;
    assert!(compile(source).unwrap_err().contains("must return on every path"));
}
#[test]
fn nonreturning_conditions_do_not_create_reachable_loop_exits() {
    compile(
        r#"fun value() -> Int:
    while return 7:
        Unit
"#,
    )
    .unwrap();
    compile(
        r#"fun value() -> Int:
    do:
        Unit
    while return 8
"#,
    )
    .unwrap();
    compile(
        r#"fun value() -> Int:
    if return 9:
        Unit
"#,
    )
    .unwrap();
}
#[test]
fn constant_conditions_and_unreachable_jumps_preserve_initialization() {
    compile(
        r#"fun value() -> Int:
    val result: Int
    if (true):
        result = 1
    else:
        Unit
    return result
"#,
    )
    .unwrap();
    compile(
        r#"fun value() -> Int:
    while (true):
        return 1
        break
"#,
    )
    .unwrap();
    compile(
        r#"fun value() -> Int:
    val result: Int
    for item in 1..2:
        result = 1
        return result
        continue
"#,
    )
    .unwrap();
}
#[test]
fn choice_preparation_merges_hidden_paths_and_narrows_only_stable_guards() {
    assert!(
        compile(
            r#"scene start():
    val result: Int
    choice:
        option "Hidden ${return}" if false:
            Unit
        option "Continue":
            Unit
    prose:
        ${result}
"#
        )
        .unwrap_err()
        .contains("not definitely initialized")
    );
    compile(
        r#"scene start():
    val name: String? = "Ada"
    choice:
        option "Hello ${name.length}" if name != null:
            prose:
                ${name.length}
        else:
            Unit
"#,
    )
    .unwrap();
    compile(
        r#"suspend fun answer() -> Int:
    choice:
        option "Hidden" if (false):
            Unit
        option "Visible":
            return 1
        else:
            Unit
"#,
    )
    .unwrap();
    assert!(
        compile(
            r#"scene start():
    var name: String? = "Ada"
    fun clear() -> Boolean:
        name = null
        return false
    choice:
        option "First" if name != null:
            prose:
                ${name.length}
        option "Second" if clear():
            Unit
"#
        )
        .unwrap_err()
        .contains("nullable")
    );
}
#[test]
fn when_test_and_guard_effects_follow_each_reachable_path() {
    for program in [
        r#"fun run(flag: Boolean):
    val x: Int
    when:
        case (if flag:
            x = 1
            false
        else:
            x = 2
            false
        ):
            return
        else:
            x = 3
scene start():
    return
"#,
        r#"fun run(flag: Boolean):
    val x: Int
    when:
        case true if (if flag:
            x = 1
            false
        else:
            x = 2
            false
        ):
            return
        else:
            x = 3
scene start():
    return
"#,
        r#"fun run(flag: Boolean):
    val x: Int
    when:
        case flag, (if true:
            x = 1
            true
        else:
            x = 2
            true
        ):
            prose:
                ${x}
        else:
            return
scene start():
    return
"#,
    ] {
        let error = compile(program).expect_err(program);
        assert!(error.contains("initialized"), "{error}\n{program}");
    }
}
#[test]
fn failed_when_tests_initialize_later_branches_and_subjectless_tests_narrow() {
    compile(
        r#"fun run(flag: Boolean, name: String?) -> Int:
    val x: Int
    when:
        case (if flag:
            x = 1
            false
        else:
            x = 2
            false
        ):
            return 0
        else:
            return x
fun length(name: String?) -> Int:
    when:
        case name != null:
            return name.length
        else:
            return 0
scene start():
    return
"#,
    )
    .unwrap();
}
#[test]
fn scalar_constant_conditions_prove_flow_without_replacing_evaluation() {
    compile(
        r#"scene start():
    val first: Int
    if 2 * 3 == 6 && ('a' == "a"):
        first = 1
    val second: Int
    if 3 === 3.0:
        return
    else:
        second = 2
    val third: Int
    when 1 + 2:
        case 3:
            third = 3
    prose:
        ${first}|${second}|${third}
"#,
    )
    .unwrap();
    let error = compile(
        r#"scene start():
    choice:
        option "Impossible" if 2 * 3 < 5:
            return
"#,
    )
    .unwrap_err();
    assert!(error.contains("no selectable options"), "{error}");
}

#[test]
fn when_alternatives_cannot_discard_nullable_matches() {
    for source in [
        r#"fun invalid(value: String?, alternative: String?):
    return when val selected = value:
        case alternative, "x":
            selected.length
        else:
            0
scene start():
    invalid(null, null)
"#,
        r#"fun invalid(value: String?, members: Set<String?>):
    return when val selected = value:
        case in members, "x":
            selected.length
        else:
            0
scene start():
    invalid(null, {null})
"#,
    ] {
        let error = compile(source).expect_err("nullable alternatives must not prove a non-null subject");
        assert!(error.contains("nullable"), "{error}");
    }
}
