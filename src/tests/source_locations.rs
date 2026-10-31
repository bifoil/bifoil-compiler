//! Source spans share module paths without changing serialized diagnostics.

use crate::model::{Module, Span};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

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
