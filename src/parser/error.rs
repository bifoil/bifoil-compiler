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

#[cfg(test)]
mod tests {
    use super::ParseError;

    #[test]
    fn missing_locations_preserve_the_file_boundary_until_a_physical_line_is_known() {
        let error = ParseError::from("missing operand").at(0).offset(8);
        assert_eq!(error.to_string(), "missing operand");
        assert_eq!(error.render("story.bfl"), "story.bfl: missing operand");
        let error = error.at(2).at(40).offset(3);
        assert_eq!(error.to_string(), "5: missing operand");
        assert_eq!(error.render("story.bfl"), "story.bfl:5: missing operand");
    }

    #[test]
    fn formatting_propagates_a_writer_failure() {
        struct FailedWriter;
        impl std::fmt::Write for FailedWriter {
            fn write_str(&mut self, _: &str) -> std::fmt::Result {
                Err(std::fmt::Error)
            }
        }
        for error in [
            ParseError::from("message"),
            ParseError::from(String::from("message")).at(3),
        ] {
            assert!(std::fmt::write(&mut FailedWriter, format_args!("{error}")).is_err());
        }
    }
}
