//! Quoted text escapes and interpolation.

use super::Cursor;
use crate::lexer::interpolation_end;
use crate::syntax::{Expr, TemplatePart};

pub(super) fn template(raw: &str, depth: usize, source_line: usize) -> Result<Expr, String> {
    parse_template(raw, depth, source_line, false)
}

pub(super) fn prose_template(raw: &str, depth: usize, source_line: usize) -> Result<Expr, String> {
    parse_template(raw, depth, source_line, true)
}

fn parse_template(raw: &str, depth: usize, source_line: usize, directive_escape: bool) -> Result<Expr, String> {
    if depth > 64 {
        return Err("string nesting budget exceeded".into());
    }
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut index = 0;
    let mut line = source_line;
    while index < raw.len() {
        let rest = &raw[index..];
        if rest.starts_with('\\') {
            index += 1;
            let escaped = raw[index..].chars().next().ok_or("incomplete escape")?;
            index += escaped.len_utf8();
            match escaped {
                'n' => text.push('\n'),
                'r' => text.push('\r'),
                't' => text.push('\t'),
                '\\' | '"' | '\'' => text.push(escaped),
                '@' if directive_escape => text.push('@'),
                '$' if raw[index..].starts_with('{') => text.push('$'),
                'u' | 'U' => text.push(unicode_escape(raw, &mut index, escaped)?),
                _ => return Err(format!("unknown escape '\\{escaped}'")),
            }
        } else if rest.starts_with("${") {
            parts.push(TemplatePart {
                value: Expr::String(std::mem::take(&mut text)),
                line,
            });
            let start = index + 2;
            let end = interpolation_end(raw, start)?;
            let mut cursor = Cursor::located(&raw[start..end], line)?;
            let expression_line = cursor.line();
            parts.push(TemplatePart {
                value: cursor.expr(0, depth + 1)?,
                line: expression_line,
            });
            cursor.end()?;
            line += raw[start..end].bytes().filter(|byte| *byte == b'\n').count();
            index = end + 1;
        } else {
            let scalar = rest.chars().next().unwrap();
            text.push(if scalar == '\n' && directive_escape {
                ' '
            } else {
                scalar
            });
            if scalar == '\n' {
                line += 1;
            }
            index += scalar.len_utf8();
        }
    }
    parts.push(TemplatePart {
        value: Expr::String(text),
        line,
    });
    if parts.len() == 1 {
        Ok(parts.remove(0).value)
    } else {
        Ok(Expr::Format(parts))
    }
}

fn unicode_escape(raw: &str, index: &mut usize, kind: char) -> Result<char, String> {
    let start;
    let end;
    if kind == 'u' && raw[*index..].starts_with('{') {
        start = *index + 1;
        end = start + raw[start..].find('}').ok_or("invalid braced Unicode escape")?;
        if end == start || end - start > 6 {
            return Err("invalid braced Unicode escape".into());
        }
        *index = end + 1;
    } else {
        start = *index;
        end = start + if kind == 'u' { 4 } else { 8 };
        *index = end;
    }
    let hex = raw.get(start..end).ok_or("incomplete Unicode escape")?;
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid Unicode escape".into());
    }
    let scalar = u32::from_str_radix(hex, 16).map_err(|_| "invalid Unicode escape")?;
    char::from_u32(scalar).ok_or_else(|| "invalid Unicode scalar".into())
}
