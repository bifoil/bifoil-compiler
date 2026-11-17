//! Parser diagnostics retain a physical line until the public file boundary.

use std::fmt;

#[derive(Debug)]
pub(crate) struct ParseError {
    line: Option<usize>,
    message: String,
}

impl ParseError {
    pub(super) fn at(mut self, line: usize) -> Self {
        if line > 0 && self.line.is_none() {
            self.line = Some(line);
        }
        self
    }

    pub(super) fn offset(mut self, offset: usize) -> Self {
        if let Some(line) = &mut self.line {
            *line += offset;
        }
        self
    }

    pub(super) fn render(&self, file: &str) -> String {
        if self.line.is_some() {
            format!("{file}:{self}")
        } else {
            format!("{file}: {self}")
        }
    }
}

impl From<String> for ParseError {
    fn from(message: String) -> Self {
        Self { line: None, message }
    }
}

impl From<&str> for ParseError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(line) = self.line {
            write!(output, "{line}: ")?;
        }
        output.write_str(&self.message)
    }
}
