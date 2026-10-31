//! Forward writer frames reuse normal arguments, effects, and declaration scopes.

use super::compile;

#[test]
fn forward_frames_accept_typed_parameters_defaults_and_return_values() {
    compile(
        r#"scene start():
    val result: Int
    val status = initialize(value = 3)
    fun initialize(value: Int, increment: Int = 1) -> String:
        if value > 0:
            result = value + increment
        else:
            result = 0
        return "ready ${result}"
    prose:
        ${status}|${result}
"#,
    )
    .unwrap();
}

#[test]
fn forward_generic_frames_check_the_original_definition() {
    compile(
        r#"scene start():
    val result: Int
    val status = initialize<String>("Ada")
    fun <T: Equatable> initialize(value: T, amount: Int = 3) -> String:
        if value == value:
            result = amount
        else:
            result = 0
        return "ready ${result}"
    prose:
        ${status}|${result}
"#,
    )
    .unwrap();
}

#[test]
fn forward_suspending_frames_keep_their_choice_and_result() {
    compile(
        r#"scene start():
    val result: Int
    val status = "prefix " + initialize(3)
    suspend fun initialize(value: Int) -> String:
        choice:
            option "Initialize":
                result = value
        return "ready ${result}"
    prose:
        ${status}|${result}
"#,
    )
    .unwrap();
}

#[test]
fn forward_frames_can_return_readers_and_accept_variadic_groups() {
    compile(
        r#"scene start():
    val result: Int
    val reader = initialize(3, 4)
    fun initialize(vararg values: Int) -> () -> Int:
        result = values.size
        fun read() -> Int = result
        return ::read
    prose:
        ${reader()}
"#,
    )
    .unwrap();
}

#[test]
fn forward_entry_reads_are_checked_after_supplied_arguments() {
    compile(
        r#"scene start():
    var dependency: Int
    fun fill() -> Int:
        dependency = 4
        return 2
    val result: Int
    val status = initialize(fill())
    fun initialize(value: Int) -> String:
        result = dependency + value
        return "ready ${result}"
    prose:
        ${status}|${result}
"#,
    )
    .unwrap();
}

#[test]
fn forward_frames_do_not_borrow_inner_call_scope_names() {
    let error = compile(
        r#"scene start():
    val result: Int
    if true:
        val hidden = 5
        val status = initialize(3)
    val hidden = 7
    fun initialize(value: Int) -> String:
        result = hidden + value
        return "ready"
"#,
    )
    .expect_err("the declaration's later hidden cell does not exist at the call");
    assert!(error.contains("unknown name 'hidden'"), "{error}");
}

#[test]
fn forward_frames_keep_complete_writes_and_competing_stores_checked() {
    for source in [
        r#"scene start():
    val result: Int
    val status = initialize(3)
    fun initialize(value: Int) -> String:
        if value > 0:
            result = value
        return "ready"
"#,
        r#"scene start():
    val result: Int
    val status = initialize(3)
    result = 4
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
"#,
        r#"scene start():
    val result: Int
    val status = initialize(3)
    val later = 4
    fun initialize(value: Int) -> String:
        result = later + value
        return "ready"
"#,
    ] {
        let error = compile(source).expect_err("a forward prototype is not a completed write proof");
        assert!(
            error.contains("captured val") || error.contains("initialized") || error.contains("unknown name"),
            "{source}\n{error}"
        );
    }
}

#[test]
fn an_argument_transfer_can_make_an_already_initialized_writer_unreachable() {
    compile(
        r#"scene start():
    val result: Int
    result = 3
    val ignored = initialize(return)
    fun initialize(value: Int):
        result = value
"#,
    )
    .unwrap();
}

#[test]
fn retaining_a_named_closure_does_not_hide_early_calls_or_publication() {
    for source in [
        r#"scene start():
    val result: Int
    val status = initialize(3)
    fun initialize(value: Int) -> String:
        fun read() -> Int = result
        val earlier = read()
        result = value
        return "ready"
"#,
        r#"shared var saved: (() -> Int)? = null
scene start():
    val result: Int
    val status = initialize(3)
    fun initialize(value: Int) -> String:
        fun read() -> Int = result
        saved = ::read
        result = value
        return "ready"
"#,
    ] {
        let error = compile(source).expect_err("an uninitialized cell cannot be read or published");
        assert!(error.contains("initialized"), "{source}\n{error}");
    }
}

#[test]
fn forward_frame_defaults_keep_supplied_argument_effects_first() {
    compile(
        r#"shared var calls: Int = 0
fun mark(value: Int) -> Int:
    calls += 1
    return value
scene start():
    var dependency: Int
    fun fill() -> Int:
        dependency = 4
        return mark(2)
    val result: Int
    val status = initialize(fill())
    fun initialize(value: Int, increment: Int = mark(value + 1)) -> String:
        result = dependency + value + increment
        return "ready"
    prose:
        ${status}|${result}|${calls}
"#,
    )
    .unwrap();
}

#[test]
fn forward_frames_require_other_entry_reads_and_generic_definition_capabilities() {
    for source in [
        r#"scene start():
    var dependency: Int
    val result: Int
    val status = initialize(3)
    fun initialize(value: Int) -> String:
        result = dependency + value
        return "ready"
"#,
        r#"scene start():
    val result: Int
    val status = initialize<String>("Ada")
    fun <T: Any> initialize(value: T) -> String:
        result = value.length
        return "ready"
"#,
        r#"scene start():
    val result: Int
    val status = initialize(3)
    fun initialize(value: Int) -> String:
        result = value
        return "ready"
    val escaped = ::initialize
"#,
    ] {
        compile(source).expect_err("the call's specialization or prototype cannot replace definition checks");
    }
}
