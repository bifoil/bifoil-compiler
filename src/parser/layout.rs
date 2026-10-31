//! Indentation, continued statements, logical source lines, and block boundaries.

use super::{Cursor, Parser};
use crate::lexer::{LiteralState, Token, indentation, lex_lines, name, symbol};
use crate::syntax::{Expr, Statement};
use std::borrow::Cow;

/// Retain only layout facts for already scanned tokens. Rebuild after a block
/// has been replaced by an embedded expression, rather than on every source row.
#[derive(Default)]
struct StatementScan {
    scanned: usize,
    balance: i32,
    header: Option<usize>,
    lambda: bool,
}
impl StatementScan {
    fn advance(&mut self, tokens: &[Token]) {
        let branch_header = matches!(tokens.first(), Some(Token::Name(keyword))
            if ["else", "case", "option"].contains(&keyword.as_str()));
        for (index, token) in tokens.iter().enumerate().skip(self.scanned) {
            if self.header.is_none()
                && index > 0
                && !(branch_header && self.balance == 0)
                && matches!(token, Token::Name(keyword) if keyword == "if" || keyword == "when")
            {
                self.header = Some(index);
            }
            self.lambda |= symbol(token, "=>");
            if ["(", "[", "?[", "{"].iter().any(|value| symbol(token, value)) {
                self.balance += 1;
            } else if [")", "]", "}"].iter().any(|value| symbol(token, value)) {
                self.balance -= 1;
            }
        }
        self.scanned = tokens.len();
    }
}

impl Parser<'_> {
    pub(super) fn error(&self, msg: impl Into<String>) -> String {
        format!("{}:{}: {}", self.file, self.line_index + 1, msg.into())
    }

    pub(super) fn statement_cursor(&mut self) -> Result<Cursor, String> {
        let first = self.line_index;
        let level = indentation(&self.source_lines[first]).0;
        let mut tokens = Vec::new();
        let mut token_lines = Vec::new();
        let mut embedded = Vec::new();
        let mut scan = StatementScan::default();
        let mut row = first;
        loop {
            let raw = self
                .source_lines
                .get(row)
                .ok_or_else(|| self.error("unfinished continuation"))?;
            let trimmed = raw.trim_start();
            let text = if self.prose_mode {
                trimmed.strip_prefix('@').unwrap_or(trimmed)
            } else {
                trimmed
            };
            let (next, lines) = lex_lines(text).map_err(|e| self.error(e))?;
            tokens.extend(next);
            token_lines.extend(lines.into_iter().map(|offset| row + 1 + offset));
            scan.advance(&tokens);
            let colon = tokens.last().is_some_and(|t| symbol(t, ":"));
            if let Some(index) = scan.header.filter(|_| colon) {
                self.line_index = row;
                let header_line = token_lines[index];
                let header_level = indentation(&self.source_lines[header_line - 1]).0;
                let mut cursor = Cursor::new(tokens[index + 1..].to_vec());
                cursor.source_line = header_line;
                cursor.token_lines = token_lines[index + 1..].to_vec();
                cursor.embedded = embedded.clone();
                let kind = if name(&tokens[index], "if") {
                    self.parse_conditional(cursor, header_level, header_line)?.kind
                } else {
                    self.parse_when(cursor, header_level)?
                };
                let id = embedded.len();
                embedded.push(Expr::Block(vec![Statement {
                    line: header_line,
                    kind,
                }]));
                tokens.truncate(index);
                tokens.push(Token::Embedded(id));
                token_lines.truncate(index);
                token_lines.push(header_line);
                scan = StatementScan::default();
                row = self.line_index;
            } else if colon && scan.lambda {
                let mut found = None;
                for index in 0..tokens.len() {
                    if !symbol(&tokens[index], "(") && !name(&tokens[index], "suspend") {
                        continue;
                    }
                    let mut cursor = Cursor::new(tokens[index..].to_vec());
                    cursor.source_line = token_lines[index];
                    cursor.token_lines = token_lines[index..].to_vec();
                    cursor.embedded = embedded.clone();
                    if let Ok(Expr::Lambda(mut fun)) = cursor.expr(0, 0)
                        && cursor.position == cursor.tokens.len()
                        && !fun.expression
                    {
                        self.line_index = row + 1;
                        let header_level = indentation(&self.source_lines[token_lines[index] - 1]).0;
                        fun.body = self.parse_block(header_level)?;
                        found = Some((index, Expr::Lambda(fun)));
                        break;
                    }
                }
                if let Some((index, expression)) = found {
                    let id = embedded.len();
                    embedded.push(expression);
                    tokens.truncate(index);
                    tokens.push(Token::Embedded(id));
                    let header_line = token_lines[index];
                    token_lines.truncate(index);
                    token_lines.push(header_line);
                    scan = StatementScan::default();
                    row = self.line_index;
                } else {
                    self.line_index = row;
                    let mut cursor = Cursor::new(tokens);
                    cursor.token_lines = token_lines;
                    cursor.embedded = embedded;
                    cursor.indent_level = level;
                    cursor.source_line = first + 1;
                    return Ok(cursor);
                }
            } else {
                row += 1;
            }
            scan.advance(&tokens);
            let unfinished = super::operators::continues(&tokens, &embedded) && !nullable_body_colon(&tokens);
            if scan.balance <= 0 && !unfinished {
                self.line_index = row.saturating_sub(1);
                let mut cursor = Cursor::new(tokens);
                cursor.token_lines = token_lines;
                cursor.embedded = embedded;
                cursor.indent_level = level;
                cursor.source_line = first + 1;
                return Ok(cursor);
            }
            if row >= self.source_lines.len() {
                return Err(self.error("unfinished expression continuation"));
            }
        }
    }

    pub(super) fn skip_blank(&mut self) {
        while self.line_index < self.source_lines.len()
            && (self.source_lines[self.line_index].trim().is_empty()
                || self.source_lines[self.line_index].trim_start().starts_with('#')
                || self.source_lines[self.line_index].trim_start().starts_with("//"))
        {
            self.line_index += 1;
        }
    }

    pub(super) fn parse_block(&mut self, parent: usize) -> Result<Vec<Statement>, String> {
        if self.prose_mode {
            return self.parse_prose_block(parent);
        }
        self.block_depth += 1;
        if self.block_depth > 32 {
            return Err(self.error("block nesting budget exceeded"));
        }
        let mut result = Vec::new();
        self.skip_blank();
        while self.line_index < self.source_lines.len() {
            let level = indentation(&self.source_lines[self.line_index]).0;
            if level <= parent {
                break;
            }
            if level != parent + 1 {
                return Err(self.error("expected exactly one additional indentation level"));
            }
            result.push(self.parse_statement()?);
            if result.len() > 4096 {
                return Err(self.error("block statement budget exceeded"));
            }
            self.skip_blank();
        }
        self.block_depth -= 1;
        if result.is_empty() {
            return Err(self.error("block requires a statement"));
        }
        Ok(result)
    }
}

/// In a type-bearing block header, terminal ?: means a nullable type plus
/// the block colon. Other terminal Elvis operators still continue the expression.
fn nullable_body_colon(tokens: &[Token]) -> bool {
    if !tokens.last().is_some_and(|token| symbol(token, "?:")) {
        return false;
    }
    let first = tokens.iter().find(|token| {
        !["export", "suspend", "legacy", "transfer"]
            .iter()
            .any(|keyword| name(token, keyword))
    });
    if first.is_some_and(|token| {
        ["fun", "data", "sealed", "enum"]
            .iter()
            .any(|keyword| name(token, keyword))
    }) {
        let mut depth = 0;
        for token in tokens {
            if symbol(token, "(") || symbol(token, "[") || symbol(token, "{") {
                depth += 1;
            }
            if symbol(token, ")") || symbol(token, "]") || symbol(token, "}") {
                depth -= 1;
            }
            if depth == 0 && symbol(token, "=") {
                return false;
            }
        }
        return true;
    }
    if !first.is_some_and(|token| {
        ["if", "elif", "else", "while", "when", "case", "option", "for"]
            .iter()
            .any(|keyword| name(token, keyword))
    }) {
        return false;
    }
    let Some(index) = tokens.iter().rposition(|token| {
        ["is", "!is", "as", "as?"]
            .iter()
            .any(|keyword| name(token, keyword) || symbol(token, keyword))
    }) else {
        return false;
    };
    let mut suffix = Cursor::new(tokens[index + 1..].to_vec());
    suffix.ty().is_ok() && suffix.need(":").is_ok() && suffix.end().is_ok()
}

pub(super) fn logical_lines(source: &str) -> Result<Vec<Cow<'_, str>>, String> {
    scan_lines(source, |_| {})
}

/// Physical narrative and continued literal rows whose whitespace is content.
pub(crate) fn protected_lines(source: &str) -> Result<Vec<bool>, String> {
    let mut protected = vec![false; source.lines().count()];
    scan_lines(source, |row| protected[row] = true)?;
    Ok(protected)
}

fn scan_lines(source: &str, mut protect: impl FnMut(usize)) -> Result<Vec<Cow<'_, str>>, String> {
    // Only text literals are combined here. Code continuations are gathered by the parser so block layout survives.
    let mut lines = source.lines().map(Cow::Borrowed).collect::<Vec<_>>();
    let mut prose = None;
    let mut prose_literals = LiteralState::default();
    let mut row = 0;
    while row < lines.len() {
        let text = lines[row].trim();
        if text.is_empty() {
            row += 1;
            continue;
        }
        let level = indentation(&lines[row]).0;
        if prose.is_some_and(|p| level <= p) {
            prose = None;
            prose_literals = LiteralState::default();
        }
        if prose.is_some() && (!text.starts_with('@') || prose_literals.continues()) {
            protect(row);
            prose_literals
                .scan_template_line(&lines[row])
                .map_err(|error| format!("{}: {error}", row + 1))?;
            row += 1;
            continue;
        }
        if text == "prose:" || text == "@prose:" {
            prose = Some(level);
            row += 1;
            continue;
        }
        let start = row;
        let mut combined = lines[row].clone();
        let mut literals = LiteralState::default();
        loop {
            literals
                .scan_line(&lines[row])
                .map_err(|error| format!("{}: {error}", row + 1))?;
            if !literals.continues() {
                break;
            }
            row += 1;
            if row >= lines.len() {
                return Err(format!("{}: unterminated text literal or interpolation", start + 1));
            }
            combined.to_mut().push('\n');
            combined.to_mut().push_str(&lines[row]);
        }
        if row > start {
            lines[start] = combined;
            for (offset, line) in lines[start + 1..=row].iter_mut().enumerate() {
                protect(start + 1 + offset);
                *line = Cow::Borrowed("")
            }
        }
        row += 1;
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::logical_lines;

    #[test]
    fn triple_layout_preserves_prose_comments_quotes_and_physical_spans() {
        let source = r#"# """ ignored comment
fun text() = """first
# literal comment
\""" escaped delimiter
last""" + """second
end""" // """ ignored
scene start():
    prose:
        """ ordinary narrative
        @val quoted = """embedded
import literal.notAModule
        """
"#;
        let lines = logical_lines(source).unwrap();
        assert!(lines[1].contains("last\"\"\" + \"\"\"second\nend"));
        assert!(lines[2..6].iter().all(|line| line.is_empty()));
        assert_eq!(lines[8].trim(), "\"\"\" ordinary narrative");
        assert!(lines[9].contains("import literal.notAModule"));
        assert!(lines[10..].iter().all(|line| line.is_empty()));
    }

    #[test]
    fn long_triple_strings_keep_one_logical_line_and_original_line_count() {
        let source = format!("fun text() = \"\"\"\n{}\"\"\"\n", "original scalar 😀\n".repeat(12000));
        let lines = logical_lines(&source).unwrap();
        assert_eq!(lines.len(), 12002);
        assert_eq!(lines[0], source.trim_end());
        assert!(lines[1..].iter().all(|line| line.is_empty()));
    }
    #[test]
    fn unchanged_prose_and_code_rows_borrow_source_while_joined_literals_own_their_text() {
        use std::borrow::Cow;
        let source = format!(
            "fun message() = \"\"\"first\nlast\"\"\"\nscene start():\n    prose:\n{}",
            "        Unicode 😀 remains narrative\n".repeat(1_000),
        );
        let lines = logical_lines(&source).unwrap();
        assert!(matches!(lines[0], Cow::Owned(_)));
        assert!(lines[0].ends_with("last\"\"\""));
        assert!(lines[1].is_empty());
        assert_eq!(lines.len(), 1_004);
        for line in &lines[2..] {
            assert!(matches!(line, Cow::Borrowed(_)));
        }
        assert_eq!(lines.last().unwrap().trim(), "Unicode 😀 remains narrative");
    }
}
