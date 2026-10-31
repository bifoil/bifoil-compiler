mod literals;
mod numbers;

use literals::quoted_literal;
pub(crate) use literals::{LiteralState, interpolation_end};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Embedded(usize),
    Name(String),
    Number(String),
    Text(String),
    Character(String),
    Symbol(String),
}
impl std::fmt::Display for Token {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Embedded(_) => out.write_str("block expression"),
            Self::Name(name) => write!(out, "identifier '{name}'"),
            Self::Number(number) => write!(out, "number {number}"),
            Self::Text(_) => out.write_str("String literal"),
            Self::Character(_) => out.write_str("Char literal"),
            Self::Symbol(symbol) => write!(out, "'{symbol}'"),
        }
    }
}

pub fn lex(text: &str) -> Result<Vec<Token>, String> {
    tokenize(text, false).map(|(tokens, _)| tokens)
}

/// Token start lines relative to the supplied text; unlocated callers allocate no line array.
pub(crate) fn lex_lines(text: &str) -> Result<(Vec<Token>, Vec<usize>), String> {
    tokenize(text, true)
}

fn tokenize(text: &str, located: bool) -> Result<(Vec<Token>, Vec<usize>), String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut lines = Vec::new();
    let mut line = 0;
    let mut offset = 0;
    while offset < bytes.len() {
        // Normal scanning always starts on a scalar boundary. ASCII token
        // scanners and the literal balancer return another such boundary.
        let character = text[offset..].chars().next().unwrap();
        if character.is_whitespace() {
            line += usize::from(character == '\n');
            offset += character.len_utf8();
            continue;
        }
        if bytes[offset] == b'#' || bytes[offset..].starts_with(b"//") {
            while offset < bytes.len() && bytes[offset] != b'\n' {
                offset += 1;
            }
            continue;
        }
        let token_line = line;
        if character.is_ascii_alphabetic() || character == '_' {
            let start = offset;
            offset += 1;
            while bytes
                .get(offset)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                offset += 1;
            }
            if &text[start..offset] == "as" && bytes.get(offset) == Some(&b'?') {
                offset += 1;
            }
            out.push(Token::Name(text[start..offset].into()));
        } else if character.is_ascii_digit() {
            let start = offset;
            offset = numbers::end(bytes, offset);
            out.push(Token::Number(text[start..offset].replace('_', "")));
        } else if character == '"' || character == '\'' {
            let literal = quoted_literal(text, offset)?;
            let raw = &text[literal.content_start..literal.content_end];
            out.push(if character == '\'' {
                Token::Character(raw.into())
            } else {
                let triple = literal.content_start - offset == 3;
                Token::Text(if triple { dedent(raw) } else { raw.into() })
            });
            if located {
                line += bytes[offset..literal.next]
                    .iter()
                    .filter(|byte| **byte == b'\n')
                    .count();
            }
            offset = literal.next;
        } else {
            let mut matched = false;
            for operator in [
                "..<", "===", "!==", "%+=", "%-=", "::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=",
                "*=", "/=", "%=", "%+", "%-", "?.", "?[", "?:", "!!", "..", "!in", "!is",
            ] {
                let follows_identifier = bytes
                    .get(offset + operator.len())
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
                let keyword_boundary = !["!in", "!is"].contains(&operator) || !follows_identifier;
                if keyword_boundary && text[offset..].starts_with(operator) {
                    out.push(Token::Symbol(operator.into()));
                    offset += operator.len();
                    matched = true;
                    break;
                }
            }
            if !matched {
                if !".()[]{}:,=+-*/%<>!?@".contains(character) {
                    return Err(format!("unexpected character '{character}'"));
                }
                out.push(Token::Symbol(character.to_string()));
                offset += character.len_utf8();
            }
        }
        if located {
            lines.push(token_line);
        }
        if out.len() > 4096 {
            return Err("line token budget exceeded".into());
        }
    }
    Ok((out, lines))
}

fn dedent(raw: &str) -> String {
    let width = raw
        .split('\n')
        .filter(|s| !s.trim().is_empty())
        .map(|line| {
            line.chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .map(|c| if c == '\t' { 4 } else { 1 })
                .sum::<usize>()
        })
        .min()
        .unwrap_or(0);
    raw.split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                return line.to_string();
            }
            let mut remove = width;
            let mut offset = 0;
            let mut remainder = 0;
            for c in line.chars() {
                if remove == 0 {
                    break;
                }
                let w = if c == '\t' {
                    4
                } else if c == ' ' {
                    1
                } else {
                    break;
                };
                offset += c.len_utf8();
                if w > remove {
                    remainder = w - remove;
                    break;
                }
                remove -= w;
            }
            " ".repeat(remainder) + &line[offset..]
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn name(token: &Token, value: &str) -> bool {
    matches!(token, Token::Name(name) if name == value)
}
pub fn symbol(token: &Token, value: &str) -> bool {
    matches!(token, Token::Symbol(symbol) if symbol == value)
}
pub fn indentation(line: &str) -> (usize, usize) {
    let (mut spaces, mut tabs, mut bytes) = (0, 0, 0);
    for c in line.chars() {
        match c {
            ' ' => spaces += 1,
            '\t' => tabs += 1,
            _ => break,
        }
        bytes += c.len_utf8();
    }
    (tabs + spaces / 4, bytes)
}

#[cfg(test)]
mod tests;
