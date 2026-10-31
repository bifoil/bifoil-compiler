//! Function, variable, nominal-type, and alias declaration grammar.

use super::{Cursor, Parser};
use crate::lexer::{indentation, name};
use crate::model::Type;
use crate::syntax::{AliasAst, FieldAst, FunctionAst, Parameter, Statement, StatementKind, TypeAst};

impl Parser<'_> {
    pub(super) fn parse_function(&mut self, mut cursor: Cursor, exported: bool) -> Result<FunctionAst, String> {
        let line = if cursor.source_line > 0 {
            cursor.source_line
        } else {
            self.line_index + 1
        };
        let legacy = cursor.kw("legacy");
        let suspend = cursor.kw("suspend");
        let legacy = legacy || cursor.kw("legacy");
        let transfer = legacy && cursor.kw("transfer");
        let scene = cursor.kw("scene");
        if !scene && !cursor.kw("fun") {
            return Err(self.error("expected fun or scene"));
        }
        let mut generics = cursor.generics()?;
        let fname = cursor.ident().map_err(|e| self.error(e))?;
        cursor.need("(").map_err(|e| self.error(e))?;
        let mut params = Vec::new();
        if !cursor.sym(")") {
            loop {
                let mutable = cursor.kw("var");
                let vararg = cursor.kw("vararg");
                let pname = cursor.ident().map_err(|e| self.error(e))?;
                cursor.need(":").map_err(|e| self.error(e))?;
                let ty = cursor.ty().map_err(|e| self.error(e))?;
                let default = if cursor.sym("=") {
                    Some(cursor.expr(0, 0).map_err(|e| self.error(e))?)
                } else {
                    None
                };
                params.push(Parameter {
                    name: pname,
                    ty,
                    mutable,
                    vararg,
                    default,
                });
                if cursor.sym(")") {
                    break;
                }
                cursor.need(",").map_err(|e| self.error(e))?;
            }
        }
        if params
            .iter()
            .enumerate()
            .any(|(i, p)| p.vararg && (i + 1 != params.len() || p.default.is_some()))
        {
            return Err(self.error("vararg must be last and cannot declare a default"));
        }
        let result = if cursor.sym("->") {
            Some(cursor.ty().map_err(|e| self.error(e))?)
        } else {
            None
        };
        cursor.constraints(&mut generics)?;
        if legacy {
            if scene {
                return Err(self.error("legacy scene declarations are not supported; use a suspend function"));
            }
            let mut target = super::legacy::target(&mut cursor).map_err(|error| self.error(error))?;
            target.transfer = transfer;
            self.line_index += 1;
            return Ok(FunctionAst {
                legacy: Some(target),
                generics,
                name: fname,
                params: params.into(),
                result: Some(result.unwrap_or(if transfer { Type::Nothing } else { Type::Unit })),
                suspend: suspend || transfer,
                scene: false,
                exported,
                body: vec![],
                expression: false,
                line,
            });
        }
        let expression = cursor.sym("=");
        let body = if expression {
            if scene {
                return Err(self.error("scene requires a block body"));
            }
            let value = cursor.expr(0, 0).map_err(|e| self.error(e))?;
            cursor.end().map_err(|e| self.error(e))?;
            self.line_index += 1;
            vec![Statement {
                kind: StatementKind::Return(Some(value)),
                line,
            }]
        } else {
            cursor.need(":").map_err(|e| self.error(e))?;
            cursor.end().map_err(|e| self.error(e))?;
            let level = cursor.indent_level;
            self.line_index += 1;
            self.parse_block(level)?
        };
        if scene && result.as_ref().is_some_and(|t| *t != Type::Unit) {
            return Err(self.error("scene result is Unit"));
        }
        Ok(FunctionAst {
            legacy: None,
            generics,
            name: fname,
            params: params.into(),
            result,
            suspend: suspend || scene,
            scene,
            exported,
            body,
            expression,
            line,
        })
    }

    pub(super) fn parse_variable(&mut self, mut cursor: Cursor) -> Result<StatementKind, String> {
        let mutable = cursor.kw("var");
        if !mutable && !cursor.kw("val") {
            return Err(self.error("expected val or var"));
        }
        let name = cursor.ident().map_err(|e| self.error(e))?;
        let ty = if cursor.sym(":") {
            Some(cursor.ty().map_err(|e| self.error(e))?)
        } else {
            None
        };
        if !cursor.sym("=") {
            cursor.end().map_err(|e| self.error(e))?;
            return Ok(StatementKind::Uninitialized {
                name,
                ty: ty.ok_or_else(|| self.error("uninitialized binding requires a type"))?,
                mutable,
            });
        }
        let value = cursor.expr(0, 0).map_err(|e| self.error(e))?;
        cursor.end().map_err(|e| self.error(e))?;
        Ok(StatementKind::Variable {
            name,
            ty,
            mutable,
            value,
        })
    }

    pub(super) fn parse_type_declaration(&mut self, mut cursor: Cursor, exported: bool) -> Result<TypeAst, String> {
        let kind = if cursor.kw("data") {
            if !cursor.kw("class") {
                return Err(self.error("expected class"));
            }
            "data"
        } else if cursor.kw("sealed") {
            if !cursor.kw("class") {
                return Err(self.error("expected class"));
            }
            "sum"
        } else if cursor.kw("enum") {
            "enum"
        } else {
            return Err(self.error("expected data class, sealed class, or enum"));
        };
        let type_name = cursor.ident()?;
        let mut generics = cursor.generics()?;
        cursor.constraints(&mut generics)?;
        cursor.need(":")?;
        cursor.end()?;
        let level = cursor.indent_level;
        let line = cursor.source_line;
        self.line_index += 1;
        let mut result = TypeAst {
            generics,
            name: type_name,
            kind: kind.into(),
            exported,
            fields: vec![],
            methods: vec![],
            variants: vec![],
            line,
        };
        self.skip_blank();
        while self.line_index < self.source_lines.len() && indentation(&self.source_lines[self.line_index]).0 > level {
            if indentation(&self.source_lines[self.line_index]).0 != level + 1 {
                return Err(self.error("invalid type member indentation"));
            }
            let mut cursor = self.statement_cursor()?;
            let private = cursor.kw("private");
            if !private {
                cursor.kw("public");
            }
            if kind == "data" {
                if cursor.tokens[cursor.position..].iter().take(2).any(|t| name(t, "fun")) {
                    let mut method = self.parse_function(cursor, false)?;
                    method.name = format!("{}{}", if private { "$private$" } else { "" }, method.name);
                    result.methods.push(method);
                } else {
                    let mutable = cursor.kw("var");
                    if !mutable && !cursor.kw("val") {
                        return Err(self.error("expected field or method"));
                    }
                    let name = cursor.ident()?;
                    cursor.need(":")?;
                    let ty = cursor.ty()?;
                    let default = if cursor.sym("=") {
                        Some(cursor.expr(0, 0)?)
                    } else {
                        None
                    };
                    cursor.end()?;
                    if result.fields.iter().any(|f| f.name == name) {
                        return Err(self.error("duplicate field"));
                    }
                    result.fields.push(FieldAst {
                        name,
                        ty,
                        mutable,
                        private,
                        default,
                    });
                    self.line_index += 1;
                }
            } else {
                cursor.kw("case");
                let name = cursor.ident()?;
                let mut fields = vec![];
                if cursor.sym("(") && !cursor.sym(")") {
                    loop {
                        let name = cursor.ident()?;
                        cursor.need(":")?;
                        let ty = cursor.ty()?;
                        let default = if cursor.sym("=") {
                            Some(cursor.expr(0, 0)?)
                        } else {
                            None
                        };
                        fields.push(FieldAst {
                            name,
                            ty,
                            mutable: false,
                            private: false,
                            default,
                        });
                        if cursor.sym(")") {
                            break;
                        }
                        cursor.need(",")?;
                    }
                }
                if kind == "enum" && !fields.is_empty() {
                    return Err(self.error("plain enum cases cannot carry payloads"));
                }
                cursor.end()?;
                if result.variants.iter().any(|v| v.name == name) {
                    return Err(self.error("duplicate case"));
                }
                result.variants.push(TypeAst {
                    generics: vec![],
                    name,
                    kind: "variant".into(),
                    fields,
                    methods: vec![],
                    variants: vec![],
                    exported: false,
                    line: cursor.source_line,
                });
                self.line_index += 1;
            }
            self.skip_blank();
        }
        if kind != "data" && result.variants.is_empty() {
            return Err(self.error("closed type requires cases"));
        }
        Ok(result)
    }

    pub(super) fn parse_alias(&mut self, mut cursor: Cursor, exported: bool) -> Result<AliasAst, String> {
        let name = cursor.ident()?;
        let mut generics = cursor.generics()?;
        cursor.need("=")?;
        let ty = cursor.ty()?;
        cursor.constraints(&mut generics)?;
        cursor.end()?;
        let line = cursor.source_line;
        self.line_index += 1;
        Ok(AliasAst {
            name,
            generics,
            ty,
            exported,
            line,
        })
    }
}
