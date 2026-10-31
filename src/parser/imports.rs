//! Discover dependencies without constructing declaration or expression ASTs.
//!
//! Both discovery and full parsing use the same import grammar. Logical lines
//! hide triple-quoted contents before inspecting unindented import headers.

use super::{Cursor, layout::logical_lines};
use crate::lexer::{indentation, lex};
use crate::model::MAX_SOURCE;
use crate::syntax::Import;

pub(super) fn parse(cursor: &mut Cursor) -> Result<Import, String> {
    let mut pieces = vec![cursor.ident()?];
    while cursor.sym(".") {
        pieces.push(cursor.ident()?);
    }
    if pieces.len() < 2 {
        return Err("import requires module.symbol".into());
    }
    let symbol = pieces.pop().unwrap();
    let alias = if cursor.kw("as") {
        cursor.ident()?
    } else {
        symbol.clone()
    };
    cursor.end()?;
    Ok(Import {
        module: pieces.join("."),
        symbol,
        alias,
    })
}

pub(crate) fn discover(source: &str, file: &str) -> Result<Vec<Import>, String> {
    if source.len() > MAX_SOURCE {
        return Err(format!("{file}: source byte budget exceeded"));
    }
    let mut imports = Vec::new();
    let lines = logical_lines(source).map_err(|error| format!("{file}:{error}"))?;
    for (row, line) in lines.iter().enumerate() {
        if indentation(line).0 != 0 {
            continue;
        }
        let text = line.trim_start();
        let Some(rest) = text.strip_prefix("import") else {
            continue;
        };
        if rest.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_') {
            continue;
        }
        let error = |message| format!("{file}:{}: {message}", row + 1);
        let mut cursor = Cursor::new(lex(text).map_err(error)?);
        if cursor.kw("import") {
            imports.push(parse(&mut cursor).map_err(error)?);
        }
    }
    Ok(imports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_ignores_literal_prose_and_nested_headers() {
        let source = r#"# import wrong.comment
import story.helpers.answer as result
fun display() -> String = """
import wrong.literal
"""
scene start():
    prose:
        import wrong.prose
        """ordinary prose
    val important = 1
    prose:
        ${important}
"#;
        let imports = discover(source, "test.bfl").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module, "story.helpers");
        assert_eq!(imports[0].symbol, "answer");
        assert_eq!(imports[0].alias, "result");
        assert!(super::super::parse(source, "test.bfl").is_ok());
    }

    #[test]
    fn discovery_and_full_parser_share_import_errors_and_line_numbers() {
        for header in [
            "import missing",
            "import story.fun",
            "import story.item as val",
            "import story.item extra",
        ] {
            let source = format!("# comment\n{header}\nscene start():\n    return\n");
            let discovered = discover(&source, "test.bfl").err().unwrap();
            let parsed = super::super::parse(&source, "test.bfl").err().unwrap();
            assert_eq!(discovered, parsed);
            assert!(discovered.starts_with("test.bfl:2:"));
        }
    }
}
