//! Transitive calls retain exactly the defaults their call shapes may execute.

use super::compile;

#[test]
fn supplied_defaults_remain_skipped_through_named_and_recursive_wrappers() {
    compile(
        r#"
scene start():
    var future: Int
    fun read(value: Int = future, copy: Int = value) -> Int = copy
    fun through() -> Int = read(value = 7)
    fun reference() -> Int = (::read)(7, 8)
    fun first(flag: Boolean) -> Int:
        if flag:
            return read(value = 7)
        return second(true)
    fun second(flag: Boolean) -> Int:
        if flag:
            return through()
        return first(true)
    val a = through()
    val b = reference()
    val c = first(false)
    future = 3
    val d = read()
    prose:
        ${a}|${b}|${c}|${d}
"#,
    )
    .unwrap();
}

#[test]
fn escaping_wrappers_do_not_require_block_local_values_used_only_by_skipped_defaults() {
    compile(
        r#"
scene start():
    val reader: () -> Int = if true:
        var future: Int
        fun read(value: Int = future) -> Int = value
        fun through() -> Int = read(7)
        ::through
    else:
        () => 0
    choice:
        option "Resume wrapper":
            Unit
    prose:
        ${reader()}
"#,
    )
    .unwrap();
}

#[test]
fn reference_checks_cannot_mask_a_later_omitted_default_dependency() {
    for source in [
        r#"scene start():
    var future: Int
    fun read(value: Int = future, copy: Int = value) -> Int = copy
    fun through() -> Int:
        val reader = ::read
        val supplied = reader(7, 8)
        return read(copy = 7)
    val result = through()
    future = 3
"#,
        r#"scene start():
    val reader: () -> Int = if true:
        var future: Int
        fun read(value: Int = future) -> Int = value
        fun through() -> Int = read()
        ::through
    else:
        () => 0
"#,
        r#"scene start():
    var future: Int
    fun read(value: Int = future) -> Int = value
    fun first(flag: Boolean) -> Int:
        if flag:
            return second()
        return read(7)
    fun second() -> Int = read()
    val result = first(true)
    future = 3
"#,
    ] {
        let error = compile(source).expect_err("an executable omitted default still reads its capture");
        assert!(error.contains("not initialized"), "{error}\n{source}");
    }
}

#[test]
fn empty_graph_memoization_keeps_call_profiles_current_values_and_declaration_identity() {
    for source in [
        r#"scene start():
    var future: Int
    fun read(value: Int = future) -> Int = value
    fun supplied() -> Int = read(7)
    fun omitted() -> Int = read()
    val first = supplied()
    val second = omitted()
    future = 3
"#,
        r#"scene start():
    var future: Int
    var target: () -> Int = () => 7
    fun through() -> Int = target()
    val first = through()
    target = () => future
    val second = through()
    future = 3
"#,
        r#"scene start():
    if true:
        fun read(value: Int = 7) -> Int = value
        fun through() -> Int = read()
        val first = through()
    if true:
        var future: Int
        fun read(value: Int = future) -> Int = value
        fun through() -> Int = read()
        val second = through()
"#,
    ] {
        let error = compile(source).expect_err("empty proofs cannot outlive call profiles or captured-value state");
        assert!(error.contains("not initialized"), "{error}\n{source}");
    }
}
