//! Normalize code indentation using the parser's physical literal/prose modes.
//!
//! Content rows remain byte-for-byte unchanged. No token spelling, newline style,
//! comment, or alignment space is rewritten, and source locations keep their lines.

use crate::lexer::indentation;
use crate::model::MAX_SOURCE;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub(crate) fn format(source: &str, file: &str) -> Result<String, String> {
    // Do not publish a transformation of malformed or unfinished source.
    crate::parser::parse(source, file)?;
    let protected = crate::parser::protected_lines(source).map_err(|error| format!("{file}:{error}"))?;
    let mut output = String::with_capacity(source.len());
    for (row, line) in source.split_inclusive('\n').enumerate() {
        let (width, prefix) = if protected[row] || line.trim().is_empty() {
            (0, 0)
        } else {
            let (level, prefix) = indentation(line);
            let alignment = line[..prefix].bytes().filter(|byte| *byte == b' ').count() % 4;
            (level * 4 + alignment, prefix)
        };
        let size = output
            .len()
            .checked_add(width)
            .and_then(|size| size.checked_add(line.len() - prefix));
        if size.is_none_or(|size| size > MAX_SOURCE) {
            return Err(format!("{file}: formatted source byte budget exceeded"));
        }
        output.extend(std::iter::repeat_n(' ', width));
        output.push_str(&line[prefix..]);
    }
    Ok(output)
}

pub(crate) fn command(arguments: &[String], started: Instant) -> Result<(), String> {
    let source = arguments.first().ok_or("format requires a source file")?;
    let mut output = None;
    let mut check = false;
    let mut position = 1;
    while position < arguments.len() {
        match arguments[position].as_str() {
            "--check" if !check => check = true,
            "--out" if output.is_none() && position + 1 < arguments.len() => {
                position += 1;
                output = Some(PathBuf::from(&arguments[position]));
            }
            _ => return Err("unknown, repeated, or incomplete format option".into()),
        }
        position += 1;
    }
    if check && output.is_some() {
        return Err("format --check cannot be combined with --out".into());
    }
    let original = crate::input::read(Path::new(source), MAX_SOURCE as u64, &mut 0)?;
    let formatted = format(&original, source)?;
    if check {
        if formatted != original {
            return Err(format!("formatting differs: {source}"));
        }
    } else if let Some(path) = output {
        crate::artifact::write_text(&path, &formatted, started)?;
    } else {
        std::io::stdout()
            .lock()
            .write_all(formatted.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
