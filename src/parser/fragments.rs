//! Interpolation fragments use the ordinary layout and expression grammar.

use super::{Cursor, ParseError, Parser, layout::logical_lines};
use crate::syntax::Expr;

pub(super) fn expression(source: &str, line: usize, indent: usize, depth: usize) -> Result<(Expr, usize), ParseError> {
    // A prefix puts leading if/when headers in the same expression context as
    // an initializer. It adds no physical line and never enters the source AST.
    let fragment = format!("{}__interpolation__ = {source}", "    ".repeat(indent));
    let mut parser = Parser {
        source_lines: logical_lines(&fragment).map_err(|error| error.offset(line - 1))?,
        line_index: 0,
        line_offset: line - 1,
        expression_depth: depth,
        block_depth: 0,
        prose_mode: false,
    };
    let mut cursor: Cursor = parser.statement_cursor()?;
    cursor.position = 2;
    let origin = cursor.line();
    let value = cursor.expr(0, depth)?;
    cursor.end()?;
    parser.line_index += 1;
    parser.skip_blank();
    if parser.line_index < parser.source_lines.len() {
        return Err(parser.error("unexpected statement after interpolation expression"));
    }
    Ok((value, origin))
}
