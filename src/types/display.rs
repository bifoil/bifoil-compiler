//! Source-oriented, bounded type names for diagnostics; serialized identities stay unchanged.

use crate::model::Type;
use std::fmt::{self, Display, Formatter};

const MAX_DIAGNOSTIC_BYTES: usize = 4096;
const MAX_DIAGNOSTIC_DEPTH: usize = 64;

impl Display for Type {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        TypeName {
            out,
            remaining: MAX_DIAGNOSTIC_BYTES,
            truncated: false,
        }
        .render(self, 0)
    }
}

/// One budget covers the complete type, including repeated shared subgraphs.
struct TypeName<'a, 'b> {
    out: &'a mut Formatter<'b>,
    remaining: usize,
    truncated: bool,
}
impl TypeName<'_, '_> {
    fn elide(&mut self) -> fmt::Result {
        if !self.truncated {
            self.out.write_str("…")?;
            self.truncated = true;
        }
        Ok(())
    }

    fn text(&mut self, text: &str) -> fmt::Result {
        if self.truncated {
            return Ok(());
        }
        if text.len() <= self.remaining {
            self.remaining -= text.len();
            return self.out.write_str(text);
        }
        let mut end = self.remaining;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.out.write_str(&text[..end])?;
        self.remaining = 0;
        self.elide()
    }

    fn render(&mut self, ty: &Type, depth: usize) -> fmt::Result {
        if self.truncated {
            return Ok(());
        }
        if self.remaining == 0 || depth > MAX_DIAGNOSTIC_DEPTH {
            return self.elide();
        }
        match ty {
            Type::Int => self.text("Int"),
            Type::Float => self.text("Float"),
            Type::Boolean => self.text("Boolean"),
            Type::String => self.text("String"),
            Type::Char => self.text("Char"),
            Type::Unit => self.text("Unit"),
            Type::Nothing => self.text("Nothing"),
            Type::Any => self.text("Any"),
            Type::Null => self.text("null"),
            Type::Star => self.text("*"),
            Type::Pending => self.text("unresolved type"),
            Type::Parameter { name, nonnull } => {
                if *nonnull {
                    self.text("non-null ")?;
                }
                self.text(name.rsplit('#').next().unwrap_or(name))
            }
            Type::Nullable { inner } => {
                let grouped = matches!(
                    inner.as_ref(),
                    Type::Function { .. } | Type::Parameter { nonnull: true, .. }
                );
                if grouped {
                    self.text("(")?;
                }
                self.render(inner, depth + 1)?;
                self.text(if grouped { ")?" } else { "?" })
            }
            Type::Named { name, args } => {
                self.text(name)?;
                self.arguments(args, depth)
            }
            Type::Scene { params } => {
                self.text("Scene")?;
                self.arguments(params, depth)
            }
            Type::Function {
                params,
                result,
                suspend,
                vararg,
            } => {
                if *suspend {
                    self.text("suspend ")?;
                }
                self.text("(")?;
                for (index, parameter) in params.iter().enumerate() {
                    if self.truncated {
                        return Ok(());
                    }
                    if index > 0 {
                        self.text(", ")?;
                    }
                    if *vararg && index + 1 == params.len() {
                        self.text("vararg ")?;
                    }
                    self.render(parameter, depth + 1)?;
                }
                self.text(") -> ")?;
                self.render(result, depth + 1)
            }
        }
    }

    fn arguments(&mut self, types: &[Type], depth: usize) -> fmt::Result {
        if !types.is_empty() {
            self.text("<")?;
            for (index, ty) in types.iter().enumerate() {
                if self.truncated {
                    return Ok(());
                }
                if index > 0 {
                    self.text(", ")?;
                }
                self.render(ty, depth + 1)?;
            }
            self.text(">")?;
        }
        Ok(())
    }
}
