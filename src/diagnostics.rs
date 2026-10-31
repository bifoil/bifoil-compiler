//! Attach a source location once, retaining a more precise nested diagnostic.

pub(crate) fn at_source(file: &str, line: Option<usize>, message: String) -> String {
    if message.strip_prefix(file).is_some_and(|suffix| suffix.starts_with(':')) {
        return message;
    }
    if let Some(line) = line {
        format!("{file}:{line}: {message}")
    } else {
        format!("{file}: {message}")
    }
}
