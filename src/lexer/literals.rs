//! Shared balancing for quoted literals, interpolation expressions, and layout.
//!
//! Delimiters are ASCII, so scanning bytes cannot mistake a UTF-8 continuation
//! byte for a quote or brace. An explicit stack bounds nesting without recursion.

const MAX_CONTEXTS: usize = 128;

#[derive(Clone, Copy)]
enum Context {
    Quoted { quote: u8, width: usize },
    Interpolation { braces: usize },
}

#[derive(Clone, Copy)]
enum Root {
    Code,
    Template,
    Delimited,
}

#[derive(Default)]
pub(crate) struct LiteralState {
    contexts: Vec<Context>,
}

struct Closing {
    start: usize,
    next: usize,
}

pub(super) struct LiteralBounds {
    pub content_start: usize,
    pub content_end: usize,
    pub next: usize,
}

pub(super) fn quoted_literal(text: &str, opening: usize) -> Result<LiteralBounds, String> {
    let bytes = text.as_bytes();
    let (context, width) = quoted_context(bytes, opening);
    let mut state = LiteralState::default();
    state.push(context)?;
    let closing = state
        .scan(bytes, opening + width, Root::Delimited)?
        .ok_or("unterminated text literal or interpolation")?;
    Ok(LiteralBounds {
        content_start: opening + width,
        content_end: closing.start,
        next: closing.next,
    })
}

/// The caller has consumed `${`; return the byte offset of its matching `}`.
pub(crate) fn interpolation_end(text: &str, start: usize) -> Result<usize, String> {
    let mut state = LiteralState::default();
    state.push(Context::Interpolation { braces: 1 })?;
    state
        .scan(text.as_bytes(), start, Root::Delimited)?
        .map(|closing| closing.start)
        .ok_or_else(|| "unclosed interpolation".into())
}

impl LiteralState {
    /// Continue layout scanning without rescanning previous physical lines.
    pub(crate) fn scan_line(&mut self, line: &str) -> Result<(), String> {
        self.scan_physical_line(line, Root::Code)
    }

    /// Narrative quotes and comments are literal outside `${...}`.
    pub(crate) fn scan_template_line(&mut self, line: &str) -> Result<(), String> {
        self.scan_physical_line(line, Root::Template)
    }

    fn scan_physical_line(&mut self, line: &str, root: Root) -> Result<(), String> {
        self.scan(line.as_bytes(), 0, root)?;
        if matches!(self.contexts.last(), Some(Context::Quoted { width: 1, .. })) {
            return Err("unterminated text literal or interpolation".into());
        }
        Ok(())
    }

    pub(crate) fn continues(&self) -> bool {
        !self.contexts.is_empty()
    }

    fn push(&mut self, context: Context) -> Result<(), String> {
        if self.contexts.len() >= MAX_CONTEXTS {
            return Err("string nesting budget exceeded".into());
        }
        self.contexts.push(context);
        Ok(())
    }

    fn scan(&mut self, bytes: &[u8], mut index: usize, root: Root) -> Result<Option<Closing>, String> {
        while index < bytes.len() {
            let byte = bytes[index];
            if self.contexts.is_empty() && matches!(root, Root::Template) {
                if byte == b'\\' {
                    index += 2;
                } else if bytes[index..].starts_with(b"${") {
                    self.push(Context::Interpolation { braces: 1 })?;
                    index += 2;
                } else {
                    index += 1;
                }
                continue;
            }
            match self.contexts.last().copied() {
                Some(Context::Quoted { quote, width }) => {
                    if byte == b'\\' {
                        index += 2;
                        continue;
                    }
                    if byte == quote && (width == 1 || bytes[index..].starts_with(b"\"\"\"")) {
                        self.contexts.pop();
                        if matches!(root, Root::Delimited) && self.contexts.is_empty() {
                            return Ok(Some(Closing {
                                start: index,
                                next: index + width,
                            }));
                        }
                        index += width;
                        continue;
                    }
                    if quote == b'"' && bytes[index..].starts_with(b"${") {
                        self.push(Context::Interpolation { braces: 1 })?;
                        index += 2;
                        continue;
                    }
                    if byte == b'\n' && width == 1 {
                        return Err("newline in ordinary text literal".into());
                    }
                }
                context => {
                    if byte == b'"' || byte == b'\'' {
                        let (quoted, width) = quoted_context(bytes, index);
                        self.push(quoted)?;
                        index += width;
                        continue;
                    }
                    if byte == b'#' || bytes[index..].starts_with(b"//") {
                        index += bytes[index..]
                            .iter()
                            .position(|byte| *byte == b'\n')
                            .unwrap_or(bytes.len() - index);
                        continue;
                    }
                    if let Some(Context::Interpolation { braces }) = context {
                        if byte == b'{' {
                            *self.contexts.last_mut().unwrap() = Context::Interpolation { braces: braces + 1 };
                        } else if byte == b'}' {
                            if braces == 1 {
                                self.contexts.pop();
                                if matches!(root, Root::Delimited) && self.contexts.is_empty() {
                                    return Ok(Some(Closing {
                                        start: index,
                                        next: index + 1,
                                    }));
                                }
                            } else {
                                *self.contexts.last_mut().unwrap() = Context::Interpolation { braces: braces - 1 };
                            }
                        }
                    }
                }
            }
            index += 1;
        }
        Ok(None)
    }
}

fn quoted_context(bytes: &[u8], opening: usize) -> (Context, usize) {
    let quote = bytes[opening];
    let width = if quote == b'"' && bytes[opening..].starts_with(b"\"\"\"") {
        3
    } else {
        1
    };
    (Context::Quoted { quote, width }, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deeply_nested_literals_report_a_budget_error() {
        let source = format!("{}0{}", "\"${".repeat(MAX_CONTEXTS), "}\"".repeat(MAX_CONTEXTS));
        assert_eq!(
            quoted_literal(&source, 0).err().unwrap(),
            "string nesting budget exceeded"
        );
    }

    #[test]
    fn comments_and_quoted_braces_do_not_end_interpolation() {
        let source = "{\"}呀\": \"\"\"a\" } b\"\"\"} // } \"\"\"\n}";
        assert_eq!(interpolation_end(source, 0).unwrap(), source.len() - 1);
    }
}
