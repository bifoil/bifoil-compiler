//! Type annotation grammar.

use super::Cursor;
use crate::lexer::Token;
use crate::model::Type;

impl Cursor {
    pub(super) fn ty(&mut self) -> Result<Type, String> {
        self.ty_inner(0)
    }

    pub(super) fn ty_inner(&mut self, depth: usize) -> Result<Type, String> {
        if depth > 32 {
            return Err("type nesting budget exceeded".into());
        }
        let suspend = self.kw("suspend");
        let mut result = if self.sym("(") {
            let mut params = vec![];
            let mut vararg = false;
            if !self.sym(")") {
                loop {
                    if vararg {
                        return Err("variadic parameter must be last".into());
                    }
                    vararg = self.kw("vararg");
                    params.push(self.ty_inner(depth + 1)?);
                    if self.sym(")") {
                        break;
                    }
                    self.need(",")?;
                }
            }
            if self.sym("->") {
                Type::Function {
                    params: params.into(),
                    result: std::sync::Arc::new(self.ty_inner(depth + 1)?),
                    suspend,
                    vararg,
                }
            } else if params.len() == 1 && !vararg && !suspend {
                params.remove(0)
            } else {
                return Err("function type requires -> result".into());
            }
        } else {
            if suspend {
                return Err("suspend requires a function type".into());
            }
            if self.sym("*") {
                Type::Star
            } else {
                let mut name = self.ident()?;
                while self.sym(".") {
                    name.push('.');
                    name.push_str(&self.ident()?)
                }
                let mut args = vec![];
                if self.sym("<") {
                    loop {
                        args.push(self.ty_inner(depth + 1)?);
                        if self.sym(">") {
                            break;
                        }
                        self.need(",")?;
                    }
                }
                if !args.is_empty()
                    && ["Int", "Float", "Boolean", "String", "Char", "Unit", "Nothing", "Any"].contains(&name.as_str())
                {
                    return Err(format!("type {name} does not accept type arguments"));
                }
                match name.as_str() {
                    "Int" => Type::Int,
                    "Float" => Type::Float,
                    "Boolean" => Type::Boolean,
                    "String" => Type::String,
                    "Char" => Type::Char,
                    "Unit" => Type::Unit,
                    "Nothing" => Type::Nothing,
                    "Any" => Type::Any,
                    "Scene" => Type::Scene { params: args.into() },
                    _ => Type::named(&name, args),
                }
            }
        };
        let nullable = self.sym("?");
        // A nullable return type or final bound may directly precede the body
        // colon. The lexer sees ?:; leave its colon for the declaration parser.
        // A real continued Elvis expression has its fallback tokens by now.
        let nullable_header = !nullable && self.at_symbol(0, "?:") && self.position + 1 == self.tokens.len();
        if nullable_header {
            self.tokens[self.position] = Token::Symbol(":".into());
        }
        if nullable || nullable_header {
            result = result.nullable();
            if self.sym("?") || self.at_symbol(0, "?:") {
                return Err("repeated nullable marker".into());
            }
        }
        Ok(result)
    }
}
