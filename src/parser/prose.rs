//! Literal prose and explicitly escaped executable directives.

use super::text::prose_template;
use super::{ParseError, Parser};
use crate::lexer::{LiteralState, indentation};
use crate::syntax::{Statement, StatementKind};

impl Parser<'_> {
    pub(super) fn parse_prose_block(&mut self, parent: usize) -> Result<Vec<Statement>, ParseError> {
        self.block_depth += 1;
        if self.block_depth > 32 {
            return Err(self.error("block nesting budget exceeded"));
        }
        let mut statements = vec![];
        let mut paragraph = String::new();
        let mut literals = LiteralState::default();
        let mut start = self.line();
        while self.line_index < self.source_lines.len() {
            let raw = &self.source_lines[self.line_index];
            if !raw.trim().is_empty() && indentation(raw).0 <= parent {
                break;
            }
            let in_expression = literals.continues();
            if raw.trim().is_empty() && !in_expression {
                // A separator before a dedent belongs to the surrounding suite,
                // rather than executing once on every loop/branch visit.
                let Some(offset) = self.source_lines[self.line_index..]
                    .iter()
                    .position(|line| !line.trim().is_empty())
                else {
                    break;
                };
                let next = self.line_index + offset;
                if indentation(&self.source_lines[next]).0 <= parent {
                    break;
                }
                self.flush_prose_paragraph(&mut paragraph, start, &mut statements)?;
                statements.push(Statement {
                    line: self.line(),
                    kind: StatementKind::ParagraphBreak,
                });
                // One boundary represents a whole blank run. Re-scanning that
                // run for every empty row would take quadratic time.
                self.line_index = next;
                continue;
            }
            if raw.trim_start().starts_with('@') && !in_expression {
                if indentation(raw).0 != parent + 1 {
                    return Err(self.error("directive requires one additional structural level"));
                }
                let directive = raw.trim_start().strip_prefix('@').unwrap().trim_start();
                if directive.starts_with('#') || directive.starts_with("//") {
                    self.line_index += 1;
                    continue;
                }
                self.flush_prose_paragraph(&mut paragraph, start, &mut statements)?;
                statements.push(self.parse_statement()?);
                continue;
            }
            let (content, residual) = prose_content(raw, (parent + 1) * 4);
            if paragraph.is_empty() {
                start = self.line()
            } else {
                // Retain physical lines until template parsing has recorded locations.
                paragraph.push('\n')
            }
            literals
                .scan_template_line(content)
                .map_err(|error| self.error(error))?;
            paragraph.push_str(&" ".repeat(residual));
            paragraph.push_str(content);
            self.line_index += 1;
        }
        self.flush_prose_paragraph(&mut paragraph, start, &mut statements)?;
        self.block_depth -= 1;
        if statements.is_empty() {
            return Err(self.error("prose block requires content"));
        }
        Ok(statements)
    }

    fn flush_prose_paragraph(
        &self,
        paragraph: &mut String,
        start: usize,
        statements: &mut Vec<Statement>,
    ) -> Result<(), ParseError> {
        if !paragraph.is_empty() {
            statements.push(Statement {
                line: start,
                kind: StatementKind::Text(prose_template(paragraph, 0, start).map_err(|error| self.error(error))?),
            });
            paragraph.clear();
        }
        Ok(())
    }
}

/// Remove structural indentation while preserving a tab's unused alignment spaces.
fn prose_content(raw: &str, mut width: usize) -> (&str, usize) {
    let mut offset = 0;
    let mut residual = 0;
    for ch in raw.chars() {
        if width == 0 {
            break;
        }
        let columns = match ch {
            '\t' => 4,
            ' ' => 1,
            _ => break,
        };
        offset += ch.len_utf8();
        if columns > width {
            residual = columns - width;
            break;
        }
        width -= columns;
    }
    (&raw[offset..], residual)
}

#[cfg(test)]
mod tests;
