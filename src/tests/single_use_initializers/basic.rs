//! Single-use discovery, source references, and lexical binding lifetimes.

use super::compile;

#[test]
fn single_use_initializers_keep_references_and_repeated_calls_restricted() {
    for tail in [
        "    initialize()\n    initialize()\n",
        "    val reference = ::initialize\n    initialize()\n",
        "    initialize()\n    prose:\n        ${initialize() == Unit}\n",
    ] {
        let source = format!("scene start():\n    val x: Int\n    fun initialize():\n        x = 3\n{tail}");
        let error = compile(&source).expect_err("the writer is no longer confined to one standalone call");
        assert!(error.contains("captured val"), "{error}");
    }
}

#[test]
fn initializer_usage_proofs_do_not_ignore_posttest_conditions_or_self_references() {
    for source in [
        r#"scene start():
    do:
        val x: Int
        fun initialize():
            x = 3
        initialize()
    while initialize() == Unit
"#,
        r#"scene start():
    val callback: () -> Unit
    fun initialize():
        callback = ::initialize
    initialize()
    callback()
"#,
    ] {
        let error = compile(source).expect_err("a hidden second invocation must not be possible");
        assert!(error.contains("captured val"), "{error}");
    }
}

#[test]
fn single_use_initializers_preserve_names_and_definition_site_visibility() {
    for source in [
        r#"scene start():
    val x: Int
    fun initialize():
        x = 3
    initialize()
    val initialize = 7
"#,
        r#"scene start():
    val x: Int
    fun initialize():
        x = later
    val later = 3
    initialize()
"#,
    ] {
        assert!(compile(source).is_err(), "declaration checks were bypassed: {source}");
    }
}

#[test]
fn single_use_initializers_follow_lexical_body_lifetimes() {
    for (header, indentation) in [
        ("    if true:\n", 8),
        ("    when true:\n        case true:\n", 12),
        ("    for i in 1..3:\n", 8),
        ("    while false:\n", 8),
        ("    choice:\n        option \"Initialize\":\n", 12),
    ] {
        let nested = "val x: Int\nfun initialize():\n    x = 3\ninitialize()\nprose:\n    ${x}\n";
        let body = nested
            .lines()
            .map(|line| format!("{}{line}\n", " ".repeat(indentation)))
            .collect::<String>();
        let source = format!("scene start():\n{header}{body}");
        compile(&source).unwrap_or_else(|error| panic!("{source}\n{error}"));
    }
    compile(
        r#"scene start():
    var count = 0
    do:
        val x: Int
        fun initialize():
            x = count + 1
        initialize()
        count = x
    while x < 3
    prose:
        ${count}
"#,
    )
    .expect("a new loop-body binding is initialized once on each iteration");
}

#[test]
fn nested_initializer_proofs_keep_outer_vals_and_late_references_restricted() {
    for source in [
        r#"scene start():
    val x: Int
    for i in 1..3:
        fun initialize():
            x = i
        initialize()
"#,
        r#"scene start():
    do:
        val x: Int
        fun initialize():
            x = 3
        initialize()
    while (() -> Boolean => initialize() == Unit)()
"#,
    ] {
        let error = compile(source).expect_err("the helper cannot acquire an unchecked second invocation");
        assert!(error.contains("captured val"), "{error}");
    }
}
