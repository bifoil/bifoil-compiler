//! Token navigation, identifiers, generic parameters, and declaration constraints.

use crate::lexer::{Token, name, symbol};
use crate::syntax::Expr;
use std::collections::BTreeSet;

#[derive(Clone)]
pub(super) struct Cursor {
    pub(super) embedded: Vec<Expr>,
    pub(super) indent_level: usize,
    pub(super) source_line: usize,
    pub(super) token_lines: Vec<usize>,
    pub(super) tokens: Vec<Token>,
    pub(super) position: usize,
}

impl Cursor {
    pub(super) fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
            embedded: vec![],
            indent_level: 0,
            source_line: 0,
            token_lines: vec![],
        }
    }

    pub(super) fn located(text: &str, source_line: usize) -> Result<Self, String> {
        let (tokens, lines) = crate::lexer::lex_lines(text)?;
        let mut cursor = Self::new(tokens);
        cursor.source_line = source_line;
        cursor.token_lines = lines.into_iter().map(|offset| source_line + offset).collect();
        Ok(cursor)
    }

    pub(super) fn line(&self) -> usize {
        self.token_lines
            .get(self.position)
            .or_else(|| self.token_lines.last())
            .copied()
            .unwrap_or(self.source_line)
    }

    pub(super) fn kw(&mut self, value: &str) -> bool {
        if self.tokens.get(self.position).is_some_and(|t| name(t, value)) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    pub(super) fn sym(&mut self, value: &str) -> bool {
        if self.tokens.get(self.position).is_some_and(|t| symbol(t, value)) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    pub(super) fn need(&mut self, value: &str) -> Result<(), String> {
        if self.sym(value) {
            Ok(())
        } else {
            Err(format!("expected '{value}'"))
        }
    }

    pub(super) fn ident(&mut self) -> Result<String, String> {
        match self.tokens.get(self.position).cloned() {
            Some(Token::Name(s)) => {
                if [
                    "val",
                    "var",
                    "fun",
                    "suspend",
                    "scene",
                    "shared",
                    "legacy",
                    "export",
                    "import",
                    "as",
                    "if",
                    "elif",
                    "else",
                    "prose",
                    "choice",
                    "option",
                    "enabledIf",
                    "once",
                    "return",
                    "true",
                    "false",
                    "null",
                    "when",
                    "case",
                    "is",
                    "in",
                    "for",
                    "while",
                    "do",
                    "break",
                    "continue",
                    "goto",
                    "visit",
                    "vararg",
                    "mutable",
                    "data",
                    "class",
                    "sealed",
                    "enum",
                    "typealias",
                    "downTo",
                    "step",
                ]
                .contains(&s.as_str())
                {
                    return Err(format!("reserved keyword '{s}' cannot be an identifier"));
                }
                self.position += 1;
                Ok(s)
            }
            _ => Err("expected an identifier".into()),
        }
    }

    pub(super) fn end(&self) -> Result<(), String> {
        if self.position == self.tokens.len() {
            Ok(())
        } else {
            Err(format!(
                "unsupported or unexpected trailing token {}",
                self.tokens[self.position]
            ))
        }
    }

    pub(super) fn generics(&mut self) -> Result<Vec<crate::model::GenericParam>, String> {
        let mut result = vec![];
        let mut names = BTreeSet::new();
        if !self.sym("<") {
            return Ok(result);
        }
        loop {
            let variance = if self.kw("out") {
                1
            } else if self.kw("in") {
                -1
            } else {
                0
            };
            let name = self.ident()?;
            if !names.insert(name.clone()) {
                return Err("duplicate type parameter".into());
            }
            let bounds = if self.sym(":") { vec![self.ty()?] } else { vec![] };
            result.push(crate::model::GenericParam { name, variance, bounds });
            if self.sym(">") {
                break;
            }
            self.need(",")?;
        }
        Ok(result)
    }

    pub(super) fn constraints(&mut self, parameters: &mut [crate::model::GenericParam]) -> Result<(), String> {
        if !self.kw("where") {
            return Ok(());
        }
        loop {
            let name = self.ident()?;
            self.need(":")?;
            let bound = self.ty()?;
            parameters
                .iter_mut()
                .find(|p| p.name == name)
                .ok_or("where names an undeclared type parameter")?
                .bounds
                .push(bound);
            if !self.sym(",") {
                break;
            }
        }
        Ok(())
    }
}

impl Cursor {
    pub(super) fn at_symbol(&self, offset: usize, expected: &str) -> bool {
        self.tokens
            .get(self.position + offset)
            .is_some_and(|token| symbol(token, expected))
    }

    pub(super) fn at_keyword(&self, offset: usize, expected: &str) -> bool {
        self.tokens
            .get(self.position + offset)
            .is_some_and(|token| name(token, expected))
    }

    pub(super) fn starts_named_argument(&self) -> bool {
        matches!(self.tokens.get(self.position), Some(Token::Name(_))) && self.at_symbol(1, "=")
    }

    pub(super) fn starts_lambda(&self) -> bool {
        if self.at_keyword(0, "suspend") {
            return true;
        }
        if !self.at_symbol(0, "(") {
            return false;
        }
        let typed_parameter =
            matches!(self.tokens.get(self.position + 1), Some(Token::Name(_))) && self.at_symbol(2, ":");
        let parameter_modifier = self.at_keyword(1, "var") || self.at_keyword(1, "vararg");
        let empty_parameters = self.at_symbol(1, ")") && (self.at_symbol(2, "=>") || self.at_symbol(2, "->"));
        typed_parameter || parameter_modifier || empty_parameters
    }

    pub(super) fn starts_loop_label(&self) -> bool {
        let Some(Token::Name(identifier)) = self.tokens.get(self.position) else {
            return false;
        };
        !["break", "continue"].contains(&identifier.as_str()) && self.at_symbol(1, "@")
    }
}
