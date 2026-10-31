//! Direct forward writers prove mutable cells without imposing val restrictions.

use super::compile;

#[test]
fn forward_writers_establish_mutable_initialization_after_normal_return() {
    compile(
        r#"scene start():
    var result: Int
    val status = initialize(3)
    fun initialize(value: Int, increment: Int = 1) -> String:
        result = value + increment
        return "ready"
    prose:
        ${status}|${result}
"#,
    )
    .unwrap();
}

#[test]
fn unreachable_forward_calls_do_not_require_unexecuted_capture_cells() {
    compile(
        r#"fun run():
    val result: Int
    initialize(return)
    val later = 3
    fun initialize(value: Unit):
        result = later
scene start():
    run()
"#,
    )
    .unwrap();
}

#[test]
fn mutable_forward_writers_allow_existing_values_and_argument_writes() {
    for setup in ["    result = 1\n", ""] {
        let source = format!(
            "scene start():\n    var result: Int\n{setup}    fun fill() -> Int:\n        result = 5\n        return 3\n    val status = initialize(fill())\n    fun initialize(value: Int) -> String:\n        result = value\n        return \"ready\"\n    val observed = result\n"
        );
        compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
}

#[test]
fn forward_checks_follow_arguments_that_initialize_other_dependencies() {
    compile(
        r#"scene start():
    var dependency: Int
    var result: Int
    fun fill() -> Int:
        dependency = 4
        return 2
    val status = initialize(fill())
    fun initialize(value: Int) -> String:
        result = dependency + value
        return "ready ${result}"
    val observed = result
"#,
    )
    .unwrap();
}

#[test]
fn forward_mutable_facts_preserve_defaults_generics_variadics_and_returned_readers() {
    for (call, header, body) in [
        (
            "initialize(3)",
            "fun initialize(value: Int = result)",
            "result = value\n        return \"ready ${result}\"",
        ),
        (
            "initialize<String>(\"Ada\")",
            "fun <T: Equatable> initialize(value: T)",
            "if value == value:\n            result = 3\n        else:\n            result = 4\n        return \"ready\"",
        ),
        (
            "initialize(3, 4)",
            "fun initialize(vararg values: Int)",
            "result = values.size\n        return \"ready\"",
        ),
    ] {
        let source = format!(
            "scene start():\n    var result: Int\n    val status = {call}\n    {header} -> String:\n        {body}\n    val observed = result\n"
        );
        compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
    compile(
        r#"scene start():
    var result: Int
    val reader = initialize(3)
    fun initialize(value: Int) -> () -> Int:
        result = value
        return () => result
    val observed = reader() + result
"#,
    )
    .unwrap();
}

#[test]
fn forward_mutable_suspension_and_read_after_write_keep_normal_frames() {
    compile(
        r#"scene start():
    var result: Int
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
fn incomplete_mutable_writers_do_not_claim_definite_initialization() {
    for call in ["initialize(false)", "false && initialize(true)"] {
        let source = format!(
            "scene start():\n    var result: Int\n    val status = {call}\n    fun initialize(flag: Boolean) -> Boolean:\n        if flag:\n            result = 3\n        return true\n    val invalid = result\n"
        );
        let error = compile(&source)
            .map(|_| ())
            .expect_err("only mandatory writes establish caller facts");
        assert!(error.contains("not definitely initialized"), "{source}\n{error}");
    }
}

#[test]
fn refreshed_call_metadata_retains_defaults_and_argument_capture_reads() {
    for source in [
        r#"scene start():
    var result: Int
    initialize()
    fun initialize(value: Int = result):
        result = value
"#,
        r#"scene start():
    var result: Int
    fun read() -> Int = result
    initialize(::read)
    fun initialize(reader: () -> Int):
        result = 3
"#,
        r#"scene start():
    var result: Int
    initialize()
    fun initialize():
        result = result + 1
"#,
    ] {
        let error = compile(source)
            .map(|_| ())
            .expect_err("an early read cannot be retired by the helper's later write");
        assert!(error.contains("initialized"), "{source}\n{error}");
    }
}

#[test]
fn forward_mutable_checks_use_the_declaration_scope_and_generic_constraints() {
    for (source, diagnostic) in [
        (
            r#"scene start():
    var result: Int
    if true:
        val hidden = 5
        initialize(3)
    val hidden = 7
    fun initialize(value: Int):
        result = hidden + value
"#,
            "unknown name 'hidden'",
        ),
        (
            r#"scene start():
    var result: Int
    initialize<String>("Ada")
    fun <T: Any> initialize(value: T):
        result = value.length
"#,
            "length",
        ),
    ] {
        let error = compile(source)
            .map(|_| ())
            .expect_err("forward checks cannot borrow caller locals or assume a concrete generic argument");
        assert!(error.contains(diagnostic), "{source}\n{error}");
    }
}

#[test]
fn supplied_defaults_do_not_add_body_preconditions_but_omitted_defaults_still_read() {
    for forward in [false, true] {
        for supplied in [false, true] {
            let definition =
                "    fun initialize(value: Int = read()) -> Int:\n        result = value\n        return result\n";
            let call = if supplied {
                "    val status = initialize(7)\n"
            } else {
                "    val status = initialize()\n"
            };
            let sequence = if forward {
                format!("{call}{definition}")
            } else {
                format!("{definition}{call}")
            };
            let source = format!(
                "scene start():\n    var result: Int\n    fun read() -> Int = result\n{sequence}    val observed = result\n"
            );
            if supplied {
                compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
            } else {
                let error = compile(&source)
                    .map(|_| ())
                    .expect_err("the selected default reads the old captured value");
                assert!(error.contains("initialized"), "{source}\n{error}");
            }
        }
    }
}

#[test]
fn default_only_writes_and_early_returns_are_not_mandatory_body_writes() {
    for (parameter, call, body) in [
        (
            "value: Int = (if true:\n        result = 3\n        7\n    else:\n        8\n    )",
            "initialize(7)",
            "if false:\n            result = 5",
        ),
        (
            "value: Int = (if true:\n        return\n    else:\n        7\n    )",
            "initialize()",
            "result = 5",
        ),
    ] {
        let source = format!(
            "scene start():\n    var result: Int\n    {call}\n    fun initialize({parameter}):\n        {body}\n    val invalid = result\n"
        );
        let error = compile(&source)
            .map(|_| ())
            .expect_err("default edges must remain part of the write proof");
        assert!(error.contains("not definitely initialized"), "{source}\n{error}");
    }
}
