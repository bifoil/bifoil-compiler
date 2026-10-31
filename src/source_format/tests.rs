use super::format;
use crate::model::MAX_SOURCE;
use std::collections::BTreeMap;
use std::time::Instant;

#[test]
fn mixed_indentation_retains_alignment_spelling_and_newline_style() {
    let source = "scene start():\r\n\t  if true:\r\n\t    Unit\r\n\t  elif false:\r\n\t    Unit\r\n\t  else if false:\r\n\t    Unit";
    let expected = "scene start():\r\n      if true:\r\n        Unit\r\n      elif false:\r\n        Unit\r\n      else if false:\r\n        Unit";
    assert_eq!(format(source, "test.bfl").unwrap(), expected);
    assert_eq!(format(expected, "test.bfl").unwrap(), expected);
}

#[test]
fn prose_directives_and_nested_literals_keep_compiled_meaning() {
    let source = concat!(
        "# \"\"\" is a comment\n",
        "fun text() -> String = \"\"\"first\n",
        "\tliteral indentation\n",
        "    last\"\"\"\n",
        "scene start():\n",
        "\tval message = text()\n",
        "\tprose:\n",
        "\t    Quoted \"\"\" prose and ${message}.\n",
        "\t    @if true:\n",
        "\t\t    Preserved narrative.\n",
        "\t    @val nested = \"\"\"embedded\n",
        "\tcontent inside a directive\n",
        "        \"\"\"\n",
        "\t    Value ${nested}.\n",
        "\tval quoted = \"${\n",
        "\t\"\"\"nested\n",
        "\tinside interpolation\n",
        "\t\"\"\"}\"\n",
        "\tprose:\n",
        "\t    ${quoted}\n",
    );
    let formatted = format(source, "src/main.bfl").unwrap();
    assert!(formatted.contains("\tliteral indentation\n"));
    assert!(formatted.contains("\t    Quoted \"\"\" prose"));
    assert!(formatted.contains("        @if true:"));
    assert!(formatted.contains("\tcontent inside a directive\n"));
    assert!(formatted.contains("\tinside interpolation\n"));
    let compile = |text| {
        crate::compiler::compile(
            crate::parser::parse(text, "src/main.bfl").unwrap(),
            "main",
            "src/main.bfl",
            &BTreeMap::new(),
            Instant::now(),
        )
        .unwrap()
    };
    assert_eq!(compile(source).digest, compile(&formatted).digest);
    assert_eq!(format(&formatted, "src/main.bfl").unwrap(), formatted);
}

#[test]
fn formatting_rejects_invalid_source_and_bounded_expansion() {
    assert!(format("scene start():\n\tval unfinished =\n", "invalid.bfl").is_err());
    let source = "\t".repeat(MAX_SOURCE / 2) + "# comment\n";
    let error = format(&source, "wide.bfl").unwrap_err();
    assert!(error.contains("formatted source byte budget exceeded"), "{error}");
}
