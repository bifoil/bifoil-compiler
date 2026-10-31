//! Literal prose and explicitly escaped executable directives.

use super::Parser;
use super::text::prose_template;
use crate::lexer::{LiteralState, indentation};
use crate::syntax::{Statement, StatementKind};

impl Parser<'_> {
    pub(super) fn parse_prose_block(&mut self, parent: usize) -> Result<Vec<Statement>, String> {
        self.block_depth += 1;
        if self.block_depth > 32 {
            return Err(self.error("block nesting budget exceeded"));
        }
        let mut statements = vec![];
        let mut paragraph = String::new();
        let mut literals = LiteralState::default();
        let mut start = self.line_index + 1;
        while self.line_index < self.source_lines.len() {
            let raw = &self.source_lines[self.line_index];
            if !raw.trim().is_empty() && indentation(raw).0 <= parent {
                break;
            }
            let in_expression = literals.continues();
            if raw.trim().is_empty() && !in_expression {
                if !paragraph.is_empty() {
                    statements.push(Statement {
                        line: start,
                        kind: StatementKind::Text(prose_template(&paragraph, 0, start).map_err(|e| self.error(e))?),
                    });
                    paragraph.clear();
                }
                statements.push(Statement {
                    line: self.line_index + 1,
                    kind: StatementKind::ParagraphBreak,
                });
                self.line_index += 1;
                continue;
            }
            if raw.trim_start().starts_with('@') && !in_expression {
                if indentation(raw).0 != parent + 1 {
                    return Err(self.error("directive requires one additional structural level"));
                }
                if !paragraph.is_empty() {
                    statements.push(Statement {
                        line: start,
                        kind: StatementKind::Text(prose_template(&paragraph, 0, start).map_err(|e| self.error(e))?),
                    });
                    paragraph.clear();
                }
                statements.push(self.parse_statement()?);
                continue;
            }
            let mut width = (parent + 1) * 4;
            let mut offset = 0;
            let mut residual = 0;
            for ch in raw.chars() {
                if width == 0 {
                    break;
                }
                let n = if ch == '\t' {
                    4
                } else if ch == ' ' {
                    1
                } else {
                    break;
                };
                offset += ch.len_utf8();
                if n > width {
                    residual = n - width;
                    break;
                }
                width -= n;
            }
            if paragraph.is_empty() {
                start = self.line_index + 1
            } else {
                // Retain physical lines until template parsing has recorded locations.
                paragraph.push('\n')
            }
            literals
                .scan_template_line(&raw[offset..])
                .map_err(|error| self.error(error))?;
            paragraph.push_str(&" ".repeat(residual));
            paragraph.push_str(&raw[offset..]);
            self.line_index += 1;
        }
        if !paragraph.is_empty() {
            statements.push(Statement {
                line: start,
                kind: StatementKind::Text(prose_template(&paragraph, 0, start).map_err(|e| self.error(e))?),
            });
        }
        self.block_depth -= 1;
        if statements.is_empty() {
            return Err(self.error("prose block requires content"));
        }
        Ok(statements)
    }
}
