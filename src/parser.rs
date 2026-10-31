//! Indentation-aware Bifoil parser. Source AST types live independently in syntax.

use crate::lexer::{indentation, name};
use crate::model::MAX_SOURCE;
use crate::syntax::{Ast, Global, Statement, StatementKind};
use std::borrow::Cow;

mod cursor;
mod declarations;
mod expressions;
mod imports;
mod layout;
mod legacy;
mod operators;
mod patterns;
mod prose;
mod statements;
mod text;
mod types;

use cursor::Cursor;
use layout::logical_lines;
use text::template;

pub(crate) use imports::discover as imports;
pub(crate) use layout::protected_lines;

struct Parser<'a> {
    source_lines: Vec<Cow<'a, str>>,
    line_index: usize,
    file: &'a str,
    block_depth: usize,
    prose_mode: bool,
}

pub fn parse(source: &str, file: &str) -> Result<Ast, String> {
    if source.len() > MAX_SOURCE {
        return Err(format!("{file}: source byte budget exceeded"));
    }
    let mut parser = Parser {
        source_lines: logical_lines(source).map_err(|error| format!("{file}:{error}"))?,
        line_index: 0,
        file,
        block_depth: 0,
        prose_mode: false,
    };
    let mut ast = Ast {
        legacy_globals: vec![],
        aliases: vec![],
        types: vec![],
        imports: vec![],
        globals: vec![],
        functions: vec![],
    };
    parser.skip_blank();
    while parser.line_index < parser.source_lines.len() {
        if indentation(&parser.source_lines[parser.line_index]).0 != 0 {
            return Err(parser.error("top-level declarations must be unindented"));
        }
        let mut cursor = parser.statement_cursor()?;
        if cursor.kw("import") {
            ast.imports
                .push(imports::parse(&mut cursor).map_err(|error| parser.error(error))?);
            parser.line_index += 1;
        } else {
            let exported = cursor.kw("export");
            let legacy_variable = cursor
                .tokens
                .get(cursor.position)
                .is_some_and(|token| name(token, "legacy"))
                && cursor
                    .tokens
                    .get(cursor.position + 1)
                    .is_some_and(|token| name(token, "val") || name(token, "var"));
            if legacy_variable {
                ast.legacy_globals.push(parser.parse_legacy_global(cursor, exported)?);
            } else if cursor.kw("typealias") {
                ast.aliases.push(parser.parse_alias(cursor, exported)?);
            } else if cursor
                .tokens
                .get(cursor.position)
                .is_some_and(|t| ["data", "sealed", "enum"].iter().any(|k| name(t, k)))
            {
                ast.types.push(parser.parse_type_declaration(cursor, exported)?);
            } else if cursor.kw("shared") {
                let line = parser.line_index + 1;
                let kind = parser.parse_variable(cursor)?;
                if matches!(kind, StatementKind::Uninitialized { .. }) {
                    return Err(parser.error("shared declarations require initializers"));
                }
                parser.line_index += 1;
                ast.globals.push(Global {
                    stmt: Statement { kind, line },
                    exported,
                });
            } else {
                ast.functions.push(parser.parse_function(cursor, exported)?);
            }
        }
        parser.skip_blank();
    }
    if ast.functions.len() > crate::model::MAX_FUNCTIONS {
        return Err(parser.error("function budget exceeded"));
    }
    if ast.functions.iter().filter(|f| f.scene).count() > 1 {
        return Err(parser.error("one scene declaration per module is supported"));
    }
    Ok(ast)
}
