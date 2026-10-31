//! Ordinary executable statements and conditional chains.

use super::{Cursor, Parser, template};
use crate::lexer::{Token, indentation, name};
use crate::syntax::{ConditionalBranch, Expr, OptionAst, Statement, StatementKind};

impl Parser<'_> {
    pub(super) fn parse_statement(&mut self) -> Result<Statement, String> {
        let line = self.line_index + 1;
        let level = indentation(&self.source_lines[self.line_index]).0;
        let mut cursor = self.statement_cursor()?;
        let loop_label = if cursor.starts_loop_label() {
            let label = cursor.ident().map_err(|e| self.error(e))?;
            cursor.need("@").map_err(|e| self.error(e))?;
            Some(label)
        } else {
            None
        };
        let kind = if cursor.kw("typealias") {
            StatementKind::Alias(self.parse_alias(cursor, false)?)
        } else if cursor.tokens.first().is_some_and(|t| name(t, "fun"))
            || (cursor.tokens.first().is_some_and(|t| name(t, "suspend"))
                && cursor.tokens.get(1).is_some_and(|t| name(t, "fun")))
        {
            return Ok(Statement {
                kind: StatementKind::Function(self.parse_function(cursor, false)?),
                line,
            });
        } else if cursor.tokens.first().is_some_and(|t| name(t, "val") || name(t, "var")) {
            let value = self.parse_variable(cursor)?;
            self.line_index += 1;
            value
        } else if cursor.kw("return") {
            let value = if cursor.position == cursor.tokens.len() {
                None
            } else {
                Some(cursor.expr(0, 0).map_err(|e| self.error(e))?)
            };
            cursor.end().map_err(|e| self.error(e))?;
            self.line_index += 1;
            StatementKind::Return(value)
        } else if cursor.kw("prose") {
            cursor.need(":").map_err(|e| self.error(e))?;
            cursor.end().map_err(|e| self.error(e))?;
            self.line_index += 1;
            let previous = self.prose_mode;
            self.prose_mode = true;
            let body = self.parse_prose_block(level)?;
            self.prose_mode = previous;
            StatementKind::Narrative(body)
        } else if cursor.kw("for") {
            let name = cursor.ident().map_err(|e| self.error(e))?;
            if !cursor.kw("in") {
                return Err(self.error("for requires in"));
            }
            let iterable = cursor.expr(0, 0).map_err(|e| self.error(e))?;
            cursor.need(":").map_err(|e| self.error(e))?;
            cursor.end().map_err(|e| self.error(e))?;
            self.line_index += 1;
            StatementKind::For {
                label: loop_label,
                name,
                iterable,
                body: self.parse_block(level)?,
            }
        } else if cursor.kw("while") {
            let condition = cursor.expr(0, 0).map_err(|e| self.error(e))?;
            cursor.need(":").map_err(|e| self.error(e))?;
            cursor.end().map_err(|e| self.error(e))?;
            self.line_index += 1;
            StatementKind::While {
                label: loop_label,
                condition,
                body: self.parse_block(level)?,
                post: false,
            }
        } else if cursor.kw("do") {
            cursor.need(":").map_err(|e| self.error(e))?;
            cursor.end().map_err(|e| self.error(e))?;
            self.line_index += 1;
            let body = self.parse_block(level)?;
            self.skip_blank();
            if self.line_index >= self.source_lines.len() || indentation(&self.source_lines[self.line_index]).0 != level
            {
                return Err(self.error("do requires an aligned trailing while"));
            }
            let mut tail = self.statement_cursor()?;
            if !tail.kw("while") {
                return Err(self.error("do requires a trailing while"));
            }
            let condition = tail.expr(0, 0).map_err(|e| self.error(e))?;
            tail.end().map_err(|e| self.error(e))?;
            self.line_index += 1;
            StatementKind::While {
                label: loop_label,
                condition,
                body,
                post: true,
            }
        } else if cursor.kw("if") {
            self.parse_conditional(cursor, level, line)?.kind
        } else if cursor.kw("elif") || cursor.kw("else") {
            return Err(self.error("else/elif requires a preceding if branch"));
        } else if cursor.kw("when") {
            self.parse_when(cursor, level)?
        } else if cursor.kw("choice") {
            cursor.need(":").map_err(|e| self.error(e))?;
            cursor.end().map_err(|e| self.error(e))?;
            self.line_index += 1;
            self.skip_blank();
            let mut options = Vec::new();
            let mut fallback = None;
            while self.line_index < self.source_lines.len()
                && indentation(&self.source_lines[self.line_index]).0 > level
            {
                if indentation(&self.source_lines[self.line_index]).0 != level + 1 {
                    return Err(self.error("invalid option indentation"));
                }
                let option_line = self.line_index + 1;
                let mut option = self.statement_cursor()?;
                if option.kw("else") {
                    option.need(":").map_err(|e| self.error(e))?;
                    option.end().map_err(|e| self.error(e))?;
                    if fallback.is_some() {
                        return Err(self.error("duplicate choice fallback"));
                    }
                    self.line_index += 1;
                    fallback = Some(self.parse_block(level + 1)?);
                } else {
                    if fallback.is_some() {
                        return Err(self.error("choice fallback must be last in this profile"));
                    }
                    if !option.kw("option") {
                        return Err(self.error("choice requires option headers"));
                    }
                    let Some(Token::Text(raw)) = option.tokens.get(option.position).cloned() else {
                        return Err(self.error("option label requires a quoted String"));
                    };
                    let label_line = option.line();
                    option.position += 1;
                    let label = template(&raw, 0, label_line).map_err(|e| self.error(e))?;
                    let (visible_line, visible) = if option.kw("if") {
                        let line = option.line();
                        (line, option.expr(0, 0).map_err(|e| self.error(e))?)
                    } else {
                        (option_line, Expr::Boolean(true))
                    };
                    let (enabled_line, enabled) = if option.kw("enabledIf") {
                        let line = option.line();
                        (line, option.expr(0, 0).map_err(|e| self.error(e))?)
                    } else {
                        (option_line, Expr::Boolean(true))
                    };
                    let once = option.kw("once");
                    option.need(":").map_err(|e| self.error(e))?;
                    option.end().map_err(|e| self.error(e))?;
                    self.line_index += 1;
                    let body = self.parse_block(level + 1)?;
                    options.push(OptionAst {
                        label,
                        visible,
                        visible_line,
                        enabled,
                        enabled_line,
                        body,
                        once,
                        line: option_line,
                    });
                }
                self.skip_blank();
                if options.len() > 256 {
                    return Err(self.error("choice option budget exceeded"));
                }
            }
            StatementKind::Choice { options, fallback }
        } else {
            let target = cursor.expr(0, 0).map_err(|e| self.error(e))?;
            let assignment = match cursor.tokens.get(cursor.position) {
                Some(Token::Symbol(op)) if ["=", "+=", "-=", "*=", "/=", "%=", "%+=", "%-="].contains(&op.as_str()) => {
                    Some(op.clone())
                }
                _ => None,
            };
            let result = if let Some(operator) = assignment {
                cursor.position += 1;
                let value = cursor.expr(0, 0).map_err(|e| self.error(e))?;
                if let Expr::Name(name) = target {
                    StatementKind::Assign { name, operator, value }
                } else {
                    StatementKind::Write {
                        target,
                        operator,
                        value,
                    }
                }
            } else {
                StatementKind::Expression(target)
            };
            cursor.end().map_err(|e| self.error(e))?;
            self.line_index += 1;
            result
        };
        Ok(Statement { kind, line })
    }

    pub(super) fn parse_conditional(
        &mut self,
        mut cursor: Cursor,
        level: usize,
        line: usize,
    ) -> Result<Statement, String> {
        let mut branches = Vec::new();
        let mut no = Vec::new();
        let mut branch_line = line;
        loop {
            let condition = cursor.expr(0, 0).map_err(|e| self.error(e))?;
            cursor.need(":").map_err(|e| self.error(e))?;
            cursor.end().map_err(|e| self.error(e))?;
            self.line_index += 1;
            branches.push(ConditionalBranch {
                condition,
                body: self.parse_block(level)?,
                line: branch_line,
            });
            self.skip_blank();
            if !self.branch_follows(level) {
                break;
            }
            let mut next = self.statement_cursor()?;
            branch_line = next.source_line;
            if !next.kw("elif") {
                if !next.kw("else") {
                    return Err(self.error("expected else or elif"));
                }
                if !next.kw("if") {
                    next.need(":").map_err(|e| self.error(e))?;
                    next.end().map_err(|e| self.error(e))?;
                    self.line_index += 1;
                    no = self.parse_block(level)?;
                    break;
                }
            }
            cursor = next;
        }
        Ok(Statement {
            line,
            kind: StatementKind::If { branches, no },
        })
    }

    /// Peeking must not parse a following statement: gathering a block expression
    /// or lambda would advance the parser into that unrelated statement's body.
    fn branch_follows(&self, level: usize) -> bool {
        let Some(line) = self.source_lines.get(self.line_index) else {
            return false;
        };
        let (indent, offset) = indentation(line);
        if indent != level {
            return false;
        }
        let mut text = &line[offset..];
        if self.prose_mode {
            let Some(directive) = text.strip_prefix('@') else {
                return false;
            };
            text = directive.trim_start();
        }
        let word = text
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .next()
            .unwrap_or_default();
        word == "else" || word == "elif"
    }
}
