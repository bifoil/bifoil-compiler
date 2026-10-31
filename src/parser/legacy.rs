//! Explicit ChoiceScript interface declarations, separate from legacy source text.

use super::{Cursor, Parser};
use crate::model::LegacyTarget;
use crate::syntax::{Expr, LegacyGlobalAst};

fn literal(cursor: &mut Cursor) -> Result<String, String> {
    match cursor.expr(0, 0)? {
        Expr::String(value) => Ok(value),
        _ => Err("legacy targets require a non-interpolated String literal".into()),
    }
}

impl Parser<'_> {
    pub(super) fn parse_legacy_global(
        &mut self,
        mut cursor: Cursor,
        exported: bool,
    ) -> Result<LegacyGlobalAst, String> {
        cursor.kw("legacy");
        let mutable = cursor.kw("var");
        if !mutable && !cursor.kw("val") {
            return Err(self.error("expected legacy val or var"));
        }
        let name = cursor.ident()?;
        cursor.need(":")?;
        let ty = cursor.ty()?;
        cursor.need("=")?;
        let target = literal(&mut cursor)?;
        cursor.end()?;
        let line = cursor.source_line;
        self.line_index += 1;
        Ok(LegacyGlobalAst {
            name,
            ty,
            mutable,
            target,
            exported,
            line,
        })
    }
}

pub(super) fn target(cursor: &mut Cursor) -> Result<LegacyTarget, String> {
    cursor.need("=")?;
    let scene = literal(cursor)?;
    let label = if cursor.kw("label") {
        Some(literal(cursor)?)
    } else {
        None
    };
    let result = if cursor.kw("result") {
        Some(literal(cursor)?)
    } else {
        None
    };
    cursor.end()?;
    Ok(LegacyTarget {
        transfer: false,
        scene,
        label,
        result,
    })
}
