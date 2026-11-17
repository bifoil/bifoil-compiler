//! A bounded original corpus checks recovery and emitted-artifact validity after source edits.
use super::compile;
use crate::model::{Module, digest};

fn verify_completed_output(source: &str) {
    match compile(source) {
        Ok(module) => {
            let bytes = serde_json::to_vec(&module).unwrap();
            let mut restored: Module = serde_json::from_slice(&bytes).unwrap();
            restored.digest.clear();
            restored.digest = digest(&restored).unwrap();
            crate::verify(&mut restored).unwrap_or_else(|error| {
                panic!("accepted source emitted an invalid reusable module: {error}\n{source}")
            });
        }
        Err(error) => assert!(!error.trim().is_empty(), "missing diagnostic for {source}"),
    }
}

// Whole-token substitutions preserve grammar often enough to reach operand,
// call, capture, and return checks rather than only lexical recovery.
fn exercise_semantic_edits(source: &str, maximum: usize) -> usize {
    let mut checked = 0;
    for (token, alternatives) in [
        (
            "Int",
            &[
                "Float",
                "Boolean",
                "String",
                "Char",
                "Int?",
                "Any",
                "Any?",
                "Nothing",
                "Unit",
                "List<Int>",
                "MutableList<Int>",
                "(Int) -> Int",
                "Scene",
            ][..],
        ),
        (
            "0",
            &[
                "false",
                "\"Other\"",
                "'x'",
                "null",
                "Unit",
                "[1, 2]",
                "mutable [1]",
                "{\"key\": 1}",
                "(item: Int) => item",
                "1..3",
                "1.5",
                "2147483648",
                "1 / 0",
                "return",
            ][..],
        ),
        (
            "true",
            &[
                "false",
                "0",
                "null",
                "Unit",
                "\"Other\"",
                "[1]",
                "return",
                "break",
                "continue",
            ][..],
        ),
        (
            "String",
            &[
                "Int",
                "String?",
                "Any",
                "Char",
                "Boolean",
                "(Int) -> Unit",
                "List<String>",
            ][..],
        ),
        ("Boolean", &["Int", "String", "Boolean?", "Any", "Nothing", "Unit"][..]),
        ("Float", &["Int", "Float?", "String", "Boolean", "Any", "Nothing"][..]),
        (
            "false",
            &["true", "0", "null", "Unit", "\"Other\"", "[1]", "return"][..],
        ),
    ] {
        for (offset, _) in source.match_indices(token) {
            let previous = source[..offset].chars().next_back();
            let following = source[offset + token.len()..].chars().next();
            if [previous, following]
                .into_iter()
                .flatten()
                .any(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                continue;
            }
            for replacement in alternatives {
                let mut changed = source.to_owned();
                changed.replace_range(offset..offset + token.len(), replacement);
                verify_completed_output(&changed);
                checked += 1;
                if checked == maximum {
                    return checked;
                }
            }
        }
    }
    checked
}

#[test]
fn coverage_single_character_edits_never_panic_or_publish_malformed_modules() {
    let originals = [
        r#"fun increment(value: Int = 1) -> Int = value + 1
scene start():
    var total = 0
    for value in 1..3 step 2:
        total += increment(value)
    prose:
        Original total ${total}.
"#,
        r#"data class Counter:
    val name: String
    var score: Int = 0
    fun read() -> Int = score
scene start():
    val counter = Counter("Original")
    counter.score = 2
    val copied = counter.copy(score = 3)
    val callback = copied::read
    callback()
"#,
        r#"sealed class Outcome:
    case Ready(value: String)
    case Empty
fun describe(value: Outcome) -> String:
    return when value:
        case is Outcome.Ready(text) if text != "":
            text
        else:
            "Original"
scene start():
    describe(Outcome.Ready("Ada"))
"#,
        r#"fun <T> identity(value: T) -> T = value
scene start():
    val callback = (value: Int) -> Int => value * 2
    val values: List<Float> = [identity(1), 2.5]
    val selected = if true:
        callback(2)
    else:
        0
    val mapping = {"original": selected}
    val result = mapping?["original"] ?: 0
"#,
        r#"suspend fun ask() -> Int:
    var selected = 0
    choice:
        option "First" if true enabledIf true once:
            selected = 1
        else:
            selected = 2
    return selected
scene start():
    val answer = ask()
    do:
        val finished = true
    while !finished
"#,
        r#"fun counter(initial: Int) -> () -> Int:
    var state = initial
    fun next() -> Int:
        state += 1
        return state
    return ::next
scene start():
    val advance = counter(0)
    advance()
"#,
        r#"fun gather(prefix: String = "Original", vararg values: Int) -> Int = values.size
scene start():
    val values = mutable [1, 2, 3]
    values[0] *= 2
    values.add(4)
    values.removeAt(1)
    val total = gather("Ada", *values, 5)
    val callback: ((Int) -> Int)? = null
    val result = callback?.(total) ?: 0
"#,
        r#"enum Mood:
    Happy
    Quiet
data class Box<out T>:
    val value: T
fun <T: Any> identity(value: T) -> T = value
scene start():
    val view: Box<Any> = Box(identity(4))
    val present = view is Box<Int>
    val mood: Mood = Happy
    val label = when mood:
        case Mood.Happy:
            "Happy"
        case Mood.Quiet:
            "Quiet"
"#,
        r#"fun repeat(stop: Boolean) -> Int:
    var count = 0
    outer@ do:
        val finished = count > 2
        count += 1
        if stop:
            break@outer
    while !finished
    return count
scene start():
    var turn = 0
    while turn < 3:
        turn += 1
        if turn == 2:
            continue
        repeat(false)
"#,
        r#"shared var gold: Int = 1
legacy var oldScore: Int = "score"
legacy fun echo(value: Int) -> Int = "helpers" label "echo" result "answer"
fun local(input: Int) -> Int:
    val answer: Int
    fun read() -> Int = answer
    if input > 0:
        answer = input
    else:
        answer = 0
    return read()
scene start():
    oldScore = echo(local(gold))
"#,
    ];
    for original in originals {
        compile(original).unwrap_or_else(|error| panic!("invalid corpus control: {error}\n{original}"));
        exercise_semantic_edits(original, usize::MAX);
        for (offset, character) in original.char_indices() {
            // Remove one complete scalar, retaining valid UTF-8. The accepted
            // language may change; recovery and artifact validity must survive.
            let mut deleted = original.to_owned();
            deleted.replace_range(offset..offset + character.len_utf8(), "");
            verify_completed_output(&deleted);
            if offset % 5 == 0 && !character.is_whitespace() {
                for replacement in ["?", ":", ")"] {
                    let mut changed = original.to_owned();
                    changed.replace_range(offset..offset + character.len_utf8(), replacement);
                    verify_completed_output(&changed);
                }
            }
        }
    }
}

#[test]
fn coverage_semantic_edits_of_original_regressions_preserve_module_admission() {
    let test_modules = [
        include_str!("capture_initialization.rs"),
        include_str!("captured_writes.rs"),
        include_str!("single_use_initializers/mod.rs"),
        include_str!("single_use_initializers/forward.rs"),
        include_str!("single_use_initializers/defaults.rs"),
        include_str!("single_use_initializers/choice_preparation.rs"),
        include_str!("single_use_initializers/structured.rs"),
        include_str!("single_use_initializers/addresses.rs"),
        include_str!("exclusive_initialization.rs"),
        include_str!("overloaded_callbacks.rs"),
        include_str!("lambda_inference.rs"),
        include_str!("overload_captures.rs"),
        include_str!("shared_callbacks.rs"),
        include_str!("single_use_initializers/generics.rs"),
        include_str!("single_use_initializers/results.rs"),
        include_str!("generic_widening.rs"),
        include_str!("generic_bounds.rs"),
        include_str!("private_members.rs"),
        include_str!("generic_result_context.rs"),
        include_str!("generic_references.rs"),
        include_str!("dependent_bounds.rs"),
        include_str!("safe_paths.rs"),
        include_str!("generic_inference.rs"),
        include_str!("types.rs"),
        include_str!("range_membership.rs"),
        include_str!("syntax.rs"),
        include_str!("flow.rs"),
        include_str!("inference.rs"),
        include_str!("members.rs"),
        include_str!("equality_contracts.rs"),
    ];
    let mut programs = std::collections::BTreeSet::new();
    for module in test_modules {
        // These test modules use ordinary one-hash raw strings for original
        // Bifoil examples. Every extracted string is treated as untrusted input;
        // it need not itself be a valid program to exercise compiler recovery.
        let mut remaining = module;
        while let Some(start) = remaining.find("r#\"") {
            let body = &remaining[start + 3..];
            let Some(end) = body.find("\"#") else { break };
            let source = &body[..end];
            if source.contains('\n') && source.len() <= 4096 {
                programs.insert(source);
            }
            remaining = &body[end + 2..];
        }
    }
    assert!(programs.len() >= 25, "the original regression corpus disappeared");
    let mut checked = 0;
    for source in programs {
        verify_completed_output(source);
        checked += exercise_semantic_edits(source, 400);
    }
    assert!(checked >= 1000, "the semantic mutation corpus became vacuous");
}
