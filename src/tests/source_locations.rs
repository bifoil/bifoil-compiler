//! Source spans share module paths without changing serialized diagnostics.

use crate::model::{Module, Span};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

#[test]
fn source_diagnostics_preserve_precise_locations_without_confusing_filename_prefixes() {
    for file in ["src/story.bfl", "C:\\original\\story.bfl", "src/😀.bfl"] {
        let precise = format!("{file}:4: original failure");
        assert_eq!(crate::diagnostics::at_source(file, Some(19), precise.clone()), precise);
        let adjacent = format!("{file}-copy:4: other source failure");
        assert_eq!(
            crate::diagnostics::at_source(file, Some(19), adjacent.clone()),
            format!("{file}:19: {adjacent}")
        );
        assert_eq!(
            crate::diagnostics::at_source(file, None, file.into()),
            format!("{file}: {file}")
        );
    }
}

#[test]
fn generated_and_nested_bodies_share_the_path_and_preserve_wire_locations() {
    let file = "src/λ-\"quoted\"\\location.bfl";
    let mut module = crate::compiler::compile(
        crate::parser::parse(
            r#"data class Box:
    val value: Int
shared val original: Box = Box(7)
fun read() -> Int:
    fun local() -> Int = original.value
    val callback: () -> Int = () => local()
    return callback()
scene start():
    val copied = original.copy()
    choice:
        option "Read ${copied.value}" once:
            prose:
                ${read()}
"#,
            file,
        )
        .unwrap(),
        "locations",
        file,
        &BTreeMap::new(),
        Instant::now(),
    )
    .unwrap();
    let spans: Vec<_> = module
        .functions
        .values()
        .flat_map(|function| function.code.iter().map(|i| &i.span))
        .collect();
    assert!(spans.len() > 30);
    let path = &spans[0].file;
    for span in &spans {
        assert!(Arc::ptr_eq(&span.file, path));
        assert_eq!(span.file.as_ref(), file);
        assert!(span.line > 0);
        let expected = serde_json::json!({ "file": file, "line": span.line });
        assert_eq!(serde_json::to_value(span).unwrap(), expected);
        let decoded: Span = serde_json::from_value(expected).unwrap();
        assert_eq!(decoded.file.as_ref(), file);
        assert_eq!(decoded.line, span.line);
    }
    let encoded = serde_json::to_vec(&module).unwrap();
    let mut restored: Module = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(serde_json::to_vec(&restored).unwrap(), encoded);
    crate::module_validation::verify(&mut module).unwrap();
    crate::module_validation::verify(&mut restored).unwrap();
}

#[test]
fn continued_checked_operations_keep_the_operator_line_through_module_serialization() {
    use crate::model::Op;

    let source = r#"fun checked(value: Int, missing: Int?, text: String, boxed: Any,
            items: MutableList<Int>, callback: ((Int) -> Int)?) -> Unit:
    val quotient = (
        value
        / value) # binary
    val negative = (
        - # unary
        value)
    val cast = (
        boxed
        as String) # cast
    val asserted = (
        missing
        !!) # assertion
    val indexed = (
        text
        [value]) # index
    val length = (
        text
        .length) # member
    val segment = (
        text.substring # callable-member
        (0, value)) # call
    val called = (
        callback
        ?.(value)) # safe-call
    val ascending = (
        value
        .. value) # range
    val stepped = (
        ascending
        step value) # step
    items[value] = value # address
fun bound(text: String) -> (Int, Int) -> String:
    return (
        text
        ::substring) # reference
scene start():
    prose:
        Ready.
"#;
    let module = crate::compiler::compile(
        crate::parser::parse(source, "src/operators.bfl").unwrap(),
        "operators",
        "src/operators.bfl",
        &BTreeMap::new(),
        Instant::now(),
    )
    .unwrap();
    let decoded: Module = serde_json::from_slice(&serde_json::to_vec(&module).unwrap()).unwrap();
    for module in [&module, &decoded] {
        let checked = &module.functions["operators.checked"];
        let mut observed = BTreeMap::new();
        for instruction in &checked.code {
            let marker = match &instruction.op {
                Op::Binary { operator } if operator == "/" => "binary",
                Op::Unary { operator } if operator == "-" => "unary",
                Op::Cast { .. } => "cast",
                Op::AssertPresent => "assertion",
                Op::Index => "index",
                Op::Member { name, .. } if name == "length" => "member",
                Op::Call { direct: true, .. } => "call",
                Op::Call { direct: false, .. } => "safe-call",
                Op::Range { .. } => "range",
                Op::RangeStep => "step",
                Op::AddressIndex => "address",
                _ => continue,
            };
            assert!(observed.insert(marker, instruction.span.line).is_none(), "{marker}");
        }
        assert_eq!(observed.len(), 11);
        let member_line = |name: &str| {
            module.functions[name]
                .code
                .iter()
                .find_map(|instruction| {
                    matches!(&instruction.op, Op::Member { name, .. } if name == "substring")
                        .then_some(instruction.span.line)
                })
                .unwrap()
        };
        for (function, marker) in [
            ("operators.checked", "callable-member"),
            ("operators.bound", "reference"),
        ] {
            let expected = source
                .lines()
                .position(|line| line.ends_with(&format!("# {marker}")))
                .unwrap()
                + 1;
            assert_eq!(member_line(function), expected, "{marker}");
        }
        for (marker, actual) in observed {
            let expected = source
                .lines()
                .position(|line| line.ends_with(&format!("# {marker}")))
                .unwrap()
                + 1;
            assert_eq!(actual, expected, "{marker}");
        }
    }
}

#[test]
fn generated_copy_and_implicit_members_retain_real_declaration_locations() {
    use crate::model::Op;

    let source = "data class Meter:\n    val value: Int\n    fun read() -> Int = value\nscene start():\n    val original = Meter(7)\n    val copied = original.copy()\n";
    let mut module = crate::compiler::compile(
        crate::parser::parse(source, "src/generated.bfl").unwrap(),
        "generated",
        "src/generated.bfl",
        &BTreeMap::new(),
        Instant::now(),
    )
    .unwrap();
    let mut sites = BTreeMap::new();
    for (name, function) in &module.functions {
        for instruction in &function.code {
            if matches!(&instruction.op, Op::Member { name, .. } if name == "value") {
                assert_eq!(instruction.span.file.as_ref(), "src/generated.bfl");
                sites.insert(name.as_str(), instruction.span.line);
            }
        }
    }
    assert_eq!(sites.len(), 2);
    assert_eq!(sites["generated.$type:Meter:copy"], 1);
    assert_eq!(sites["generated.$type:Meter:read"], 3);
    crate::module_validation::verify(&mut module).unwrap();
}
