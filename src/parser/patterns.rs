//! When subjects, alternatives, guards, and payload binding syntax.

use super::{Cursor, Parser};
use crate::lexer::{indentation, symbol};
use crate::syntax::{Pattern, StatementKind, WhenCase};

impl Parser<'_> {
    pub(super) fn parse_when(&mut self, mut cursor: Cursor, level: usize) -> Result<StatementKind, String> {
        let mut binding = None;
        let mut annotation = None;
        if cursor.kw("val") {
            binding = Some(cursor.ident().map_err(|e| self.error(e))?);
            if cursor.sym(":") {
                annotation = Some(cursor.ty().map_err(|e| self.error(e))?)
            }
            cursor.need("=").map_err(|e| self.error(e))?;
        }
        let subject = if cursor.sym(":") {
            None
        } else {
            let expression = cursor.expr(0, 0).map_err(|e| self.error(e))?;
            cursor.need(":").map_err(|e| self.error(e))?;
            Some(expression)
        };
        if binding.is_some() && subject.is_none() {
            return Err(self.error("when binding requires a subject"));
        }
        cursor.end().map_err(|e| self.error(e))?;
        self.line_index += 1;
        self.skip_blank();
        let mut cases = vec![];
        let mut saw_else = false;
        while self.line_index < self.source_lines.len() && indentation(&self.source_lines[self.line_index]).0 > level {
            if indentation(&self.source_lines[self.line_index]).0 != level + 1 {
                return Err(self.error("when branches require one additional level"));
            }
            let line = self.line_index + 1;
            let mut branch = self.statement_cursor()?;
            let fallback = branch.kw("else");
            if saw_else {
                return Err(self.error("when fallback must be unique and last"));
            }
            saw_else |= fallback;
            let mut patterns = vec![];
            if !fallback {
                if !branch.kw("case") {
                    return Err(self.error("when requires case or else"));
                }
                loop {
                    let pattern = if branch.kw("is") || branch.sym("!is") {
                        let negate = branch
                            .tokens
                            .get(branch.position - 1)
                            .is_some_and(|token| symbol(token, "!is"));
                        let ty = branch.ty().map_err(|e| self.error(e))?;
                        let bindings = if branch.sym("(") {
                            if negate {
                                return Err(self.error("negated patterns cannot bind payloads"));
                            }
                            let mut values = vec![];
                            if !branch.sym(")") {
                                loop {
                                    values.push(branch.ident().map_err(|e| self.error(e))?);
                                    if branch.sym(")") {
                                        break;
                                    }
                                    branch.need(",").map_err(|e| self.error(e))?;
                                }
                            }
                            Some(values)
                        } else {
                            None
                        };
                        Pattern::Type { ty, bindings, negate }
                    } else if branch.kw("in") || branch.sym("!in") {
                        let negate = branch
                            .tokens
                            .get(branch.position - 1)
                            .is_some_and(|token| symbol(token, "!in"));
                        Pattern::Membership {
                            value: branch.expr(0, 0).map_err(|e| self.error(e))?,
                            negate,
                        }
                    } else {
                        Pattern::Value(branch.expr(0, 0).map_err(|e| self.error(e))?)
                    };
                    if subject.is_none() && !matches!(pattern, Pattern::Value(_)) {
                        return Err(self.error("subjectless cases require Boolean expressions"));
                    }
                    patterns.push(pattern);
                    if !branch.sym(",") {
                        break;
                    }
                }
            }
            let guard = if branch.kw("if") {
                Some(branch.expr(0, 0).map_err(|e| self.error(e))?)
            } else {
                None
            };
            if fallback && guard.is_some() {
                return Err(self.error("fallback cannot have a guard"));
            }
            branch.need(":").map_err(|e| self.error(e))?;
            branch.end().map_err(|e| self.error(e))?;
            self.line_index += 1;
            let body = self.parse_block(level + 1)?;
            cases.push(WhenCase {
                patterns,
                guard,
                body,
                fallback,
                line,
            });
            self.skip_blank();
        }
        if cases.is_empty() {
            return Err(self.error("when requires branches"));
        }
        Ok(StatementKind::When {
            subject,
            binding,
            annotation,
            cases,
        })
    }
}
