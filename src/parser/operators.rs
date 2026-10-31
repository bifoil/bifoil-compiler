//! Shared operator binding and unfinished-statement recognition.
//!
//! A trailing `>` may instead close a complete type annotation or function
//! reference. Probe those existing grammars before joining the next source line.

use super::Cursor;
use crate::lexer::{Token, symbol};
use crate::syntax::Expr;

pub(super) fn precedence(operator: &str) -> Option<u8> {
    Some(match operator {
        "||" => 1,
        "&&" => 2,
        "==" | "!=" | "===" | "!==" => 3,
        "<" | "<=" | ">" | ">=" => 4,
        "in" | "!in" | "is" | "!is" => 5,
        "?:" => 6,
        "downTo" | "step" => 7,
        ".." | "..<" => 8,
        "+" | "-" | "%+" | "%-" => 9,
        "*" | "/" | "%" => 10,
        "as" | "as?" => 11,
        _ => return None,
    })
}

pub(super) fn continues(tokens: &[Token], embedded: &[Expr]) -> bool {
    let Some(Token::Symbol(operator) | Token::Name(operator)) = tokens.last() else {
        return false;
    };
    if precedence(operator).is_some() {
        return operator != ">" || !complete_tail(tokens, embedded);
    }
    matches!(
        operator.as_str(),
        "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "%+=" | "%-=" | "," | "=>" | "->" | "." | "?." | "::" | "!"
    )
}

fn complete_tail(tokens: &[Token], embedded: &[Expr]) -> bool {
    let mut cursor = Cursor::new(tokens.to_vec());
    cursor.embedded = embedded.to_vec();
    cursor.kw("export");
    cursor.kw("shared");
    if cursor.kw("typealias") {
        return (|| {
            cursor.ident()?;
            let mut parameters = cursor.generics()?;
            cursor.need("=")?;
            cursor.ty()?;
            cursor.constraints(&mut parameters)?;
            cursor.end()
        })()
        .is_ok();
    }
    if cursor.kw("val") || cursor.kw("var") {
        if cursor.ident().is_err() {
            return false;
        }
        if cursor.sym(":") && cursor.ty().is_err() {
            return false;
        }
        if cursor.end().is_ok() {
            return true;
        }
    }
    // Find an initializer or expression body, excluding parameter defaults and
    // named arguments. Its complete expression may end with ::function<Type>.
    let mut depth = 0i32;
    for (index, token) in tokens.iter().enumerate() {
        if ["(", "[", "?[", "{"].iter().any(|value| symbol(token, value)) {
            depth += 1;
        } else if [")", "]", "}"].iter().any(|value| symbol(token, value)) {
            depth -= 1;
        } else if depth == 0 && symbol(token, "=") {
            cursor.position = index + 1;
            return cursor.expr(0, 0).is_ok() && cursor.end().is_ok();
        }
    }
    // A do/while closing condition has no body colon. Ordinary expressions,
    // returns, casts, and type tests use the same expression parser.
    cursor.position = 0;
    cursor.kw("while");
    cursor.expr(0, 0).is_ok() && cursor.end().is_ok()
}

#[cfg(test)]
mod tests {
    use super::continues;
    use crate::lexer::lex;

    #[test]
    fn generic_closings_finish_types_and_references() {
        for source in [
            "val items: List<List<Int>>",
            "export typealias Items<T> = List<T>",
            "typealias Callback = () -> List<Int>",
            "val callback = ::identity<List<Int>>",
            "fun callback() = ::identity<Int>",
            "items[0] = ::identity<Int>",
            "return ::identity<Int>",
            "while value is List<Int>",
            "value as List<Int>",
        ] {
            assert!(!continues(&lex(source).unwrap(), &[]), "{source}");
        }
        for source in ["val result = value >", "if value >", "return value >", "value >"] {
            assert!(continues(&lex(source).unwrap(), &[]), "{source}");
        }
    }
}
