//! Expression grammar, operator binding, calls, and anonymous functions.

use super::{Cursor, template};
use crate::lexer::{Token, name, symbol};
use crate::syntax::{Expr, FunctionAst, Parameter, Statement, StatementKind};

impl Cursor {
    pub(super) fn arguments(&mut self, depth: usize) -> Result<Vec<(Option<String>, Expr)>, String> {
        let mut args = vec![];
        if self.sym(")") {
            return Ok(args);
        }
        loop {
            let name = if self.starts_named_argument() {
                let n = self.ident()?;
                self.need("=")?;
                Some(n)
            } else {
                None
            };
            let expr = if self.sym("*") {
                Expr::Spread(Box::new(self.expr(0, depth + 1)?))
            } else {
                self.expr(0, depth + 1)?
            };
            args.push((name, expr));
            if self.sym(")") {
                break;
            }
            self.need(",")?;
            if self.sym(")") {
                break;
            }
        }
        Ok(args)
    }

    pub(super) fn lambda(&mut self, suspend: bool, depth: usize) -> Result<Expr, String> {
        let line = self.line();
        self.need("(")?;
        let mut params = vec![];
        if !self.sym(")") {
            loop {
                let mutable = self.kw("var");
                let vararg = self.kw("vararg");
                let name = self.ident()?;
                self.need(":")?;
                let ty = self.ty()?;
                let default = if self.sym("=") {
                    Some(self.expr(0, depth + 1)?)
                } else {
                    None
                };
                params.push(Parameter {
                    name,
                    ty,
                    mutable,
                    vararg,
                    default,
                });
                if self.sym(")") {
                    break;
                }
                self.need(",")?;
            }
        }
        if params
            .iter()
            .enumerate()
            .any(|(i, p)| p.vararg && (i + 1 != params.len() || p.default.is_some()))
        {
            return Err("vararg must be last and cannot have a default".into());
        }
        let result = if self.sym("->") { Some(self.ty()?) } else { None };
        self.need("=>")?;
        let expression = !self.sym(":");
        let body = if expression {
            vec![Statement {
                line: self.line(),
                kind: StatementKind::Expression(self.expr(0, depth + 1)?),
            }]
        } else {
            vec![]
        };
        Ok(Expr::Lambda(Box::new(FunctionAst {
            legacy: None,
            generics: vec![],
            name: "$lambda".into(),
            params: params.into(),
            result,
            suspend,
            scene: false,
            exported: false,
            body,
            expression,
            line,
        })))
    }

    pub(super) fn expr(&mut self, min: u8, depth: usize) -> Result<Expr, String> {
        if depth > 64 {
            return Err("expression nesting budget exceeded".into());
        }
        let lambda_start = self.starts_lambda();
        let mut left = if lambda_start {
            let suspend = self.kw("suspend");
            self.lambda(suspend, depth)?
        } else if self.sym("-") {
            Expr::Unary("-".into(), Box::new(self.expr(11, depth + 1)?))
        } else if self.sym("!") {
            Expr::Unary("!".into(), Box::new(self.expr(11, depth + 1)?))
        } else if self.sym("::") {
            Expr::Reference(self.ident()?)
        } else if self.kw("return") {
            let value = if self.position == self.tokens.len()
                || self
                    .tokens
                    .get(self.position)
                    .is_some_and(|t| ["}", ")", ",", "]"].iter().any(|s| symbol(t, s)))
            {
                None
            } else {
                Some(Box::new(self.expr(0, depth + 1)?))
            };
            Expr::Transfer {
                kind: "return".into(),
                label: None,
                value,
            }
        } else if self.kw("break") || self.kw("continue") {
            let Token::Name(kind) = self.tokens[self.position - 1].clone() else {
                unreachable!()
            };
            let label = if self.sym("@") { Some(self.ident()?) } else { None };
            Expr::Transfer {
                kind,
                label,
                value: None,
            }
        } else if self.kw("goto") || self.kw("visit") {
            let Token::Name(kind) = self.tokens[self.position - 1].clone() else {
                unreachable!()
            };
            Expr::Transfer {
                kind,
                label: None,
                value: Some(Box::new(self.expr(0, depth + 1)?)),
            }
        } else if self.sym("(") {
            let e = self.expr(0, depth + 1)?;
            self.need(")")?;
            Expr::Group(Box::new(e))
        } else if self.sym("[") || self.sym("{") || self.kw("mutable") {
            let mut mutable = false;
            let mut list = self
                .tokens
                .get(self.position - 1)
                .is_some_and(|token| symbol(token, "["));
            if self
                .tokens
                .get(self.position - 1)
                .is_some_and(|token| name(token, "mutable"))
            {
                mutable = true;
                list = self.sym("[");
                if !list {
                    self.need("{")?
                }
            }
            let end = if list { "]" } else { "}" };
            let mut family = if list { Some("List".to_string()) } else { None };
            let mut entries = vec![];
            if !self.sym(end) {
                loop {
                    let first = self.expr(0, depth + 1)?;
                    let entry = if !list && self.sym(":") {
                        if family.as_deref() == Some("Set") {
                            return Err("cannot mix map and set entries".into());
                        }
                        family = Some("Map".into());
                        (Some(first), self.expr(0, depth + 1)?)
                    } else {
                        if family.as_deref() == Some("Map") {
                            return Err("map entry requires a value".into());
                        }
                        if !list {
                            family = Some("Set".into())
                        }
                        (None, first)
                    };
                    entries.push(entry);
                    if self.sym(end) {
                        break;
                    }
                    self.need(",")?;
                    if self.sym(end) {
                        break;
                    }
                }
            }
            Expr::Collection {
                family,
                mutable,
                entries,
            }
        } else {
            match self.tokens.get(self.position).cloned() {
                Some(Token::Embedded(id)) => {
                    self.position += 1;
                    self.embedded.get(id).cloned().ok_or("invalid embedded expression")?
                }
                Some(Token::Number(raw)) => {
                    self.position += 1;
                    if raw.starts_with("0x") || raw.starts_with("0X") {
                        Expr::Int(i64::from_str_radix(&raw[2..], 16).map_err(|_| "invalid hexadecimal Int")?)
                    } else if raw.starts_with("0b") || raw.starts_with("0B") {
                        Expr::Int(i64::from_str_radix(&raw[2..], 2).map_err(|_| "invalid binary Int")?)
                    } else if raw.contains(['.', 'e', 'E']) {
                        let n = raw.parse::<f64>().map_err(|_| "invalid Float")?;
                        if !n.is_finite() {
                            return Err("non-finite Float literal".into());
                        }
                        Expr::Float(n)
                    } else {
                        Expr::Int(raw.parse().map_err(|_| "Int literal too large")?)
                    }
                }
                Some(Token::Text(raw)) => {
                    let line = self.line();
                    self.position += 1;
                    template(&raw, depth + 1, line)?
                }
                Some(Token::Character(raw)) => {
                    let line = self.line();
                    self.position += 1;
                    let Expr::String(text) = template(&raw, depth + 1, line)? else {
                        return Err("Char cannot interpolate".into());
                    };
                    if text.chars().count() != 1 {
                        return Err("Char must contain exactly one Unicode scalar".into());
                    }
                    Expr::Char(text)
                }
                Some(Token::Name(n)) => {
                    self.position += 1;
                    match n.as_str() {
                        "true" => Expr::Boolean(true),
                        "false" => Expr::Boolean(false),
                        "null" => Expr::Null,
                        _ => Expr::Name(n),
                    }
                }
                _ => return Err("expected an expression".into()),
            }
        };
        loop {
            // Reject depth while the tree is still small enough to drop safely.
            // Waiting until the end could build an arbitrarily deep postfix or
            // left-associated tree even though recursive parsing itself is bounded.
            expression_depth(&left)?;
            if self.tokens.get(self.position).is_some_and(|t| symbol(t, "<")) {
                let mut probe = self.clone();
                probe.position += 1;
                let mut args = vec![];
                let mut valid = true;
                loop {
                    match probe.ty() {
                        Ok(t) => args.push(t),
                        Err(_) => {
                            valid = false;
                            break;
                        }
                    }
                    if probe.sym(">") {
                        break;
                    }
                    if !probe.sym(",") {
                        valid = false;
                        break;
                    }
                }
                let explicit_reference = matches!(left, Expr::Reference(_) | Expr::BoundReference(..));
                if valid && (explicit_reference || probe.tokens.get(probe.position).is_some_and(|t| symbol(t, "("))) {
                    self.position = probe.position;
                    left = Expr::TypeApply {
                        callee: Box::new(left),
                        args,
                    };
                    continue;
                }
            }
            if self.sym("(") {
                let safe = matches!(&left, Expr::Member { safe: true, .. });
                let args = self.arguments(depth)?;
                left = if safe {
                    Expr::SafeCall(Box::new(left), args)
                } else {
                    Expr::Call(Box::new(left), args)
                };
                continue;
            }
            if self.sym("?.") {
                if self.sym("(") {
                    left = Expr::SafeCall(Box::new(left), self.arguments(depth)?);
                } else {
                    left = Expr::Member {
                        receiver: Box::new(left),
                        name: self.ident()?,
                        safe: true,
                    };
                }
                continue;
            }
            if self.sym(".") {
                left = Expr::Member {
                    receiver: Box::new(left),
                    name: self.ident()?,
                    safe: false,
                };
                continue;
            }
            if self.sym("::") {
                left = Expr::BoundReference(Box::new(left), self.ident()?);
                continue;
            }
            if self.sym("[") || self.sym("?[") {
                let safe = self
                    .tokens
                    .get(self.position - 1)
                    .is_some_and(|token| symbol(token, "?["));
                let index = self.expr(0, depth + 1)?;
                self.need("]")?;
                left = Expr::Index {
                    receiver: Box::new(left),
                    index: Box::new(index),
                    safe,
                };
                continue;
            }
            if self.sym("!!") {
                left = Expr::Unwrap(Box::new(left));
                continue;
            }
            let op = match self.tokens.get(self.position) {
                Some(Token::Symbol(s)) => s.as_str(),
                Some(Token::Name(s)) if ["in", "downTo", "step", "is", "as", "as?"].contains(&s.as_str()) => s.as_str(),
                _ => break,
            }
            .to_string();
            let Some(precedence) = super::operators::precedence(&op) else {
                break;
            };
            if precedence < min {
                break;
            }
            self.position += 1;
            if ["is", "!is", "as", "as?"].contains(&op.as_str()) {
                left = Expr::TypeOp {
                    value: Box::new(left),
                    operator: op,
                    ty: self.ty()?,
                };
                continue;
            }
            let right = self.expr(if op == "?:" { precedence } else { precedence + 1 }, depth + 1)?;
            let fair = |e: &Expr| matches!(e,Expr::Binary(o,_,_) if o=="%+"||o=="%-");
            if ((op == "%+" || op == "%-") && (matches!(left, Expr::Binary(..)) || matches!(right, Expr::Binary(..))))
                || fair(&left)
                || fair(&right)
            {
                return Err("fairmath combinations require explicit parentheses".into());
            }
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        expression_depth(&left)?;
        Ok(left)
    }
}

pub(super) fn expression_depth(expr: &Expr) -> Result<(), String> {
    let mut pending = vec![(expr, 0)];
    while let Some((expr, depth)) = pending.pop() {
        if depth > 64 {
            return Err("expression tree budget exceeded".into());
        }
        match expr {
            Expr::Group(e) | Expr::Unary(_, e) | Expr::Unwrap(e) | Expr::Spread(e) | Expr::BoundReference(e, _) => {
                pending.push((e, depth + 1))
            }
            Expr::Member { receiver, .. } => pending.push((receiver, depth + 1)),
            Expr::Index { receiver, index, .. } => {
                pending.push((receiver, depth + 1));
                pending.push((index, depth + 1));
            }
            Expr::TypeOp { value, .. } => pending.push((value, depth + 1)),
            Expr::Collection { entries, .. } => {
                for (key, value) in entries {
                    if let Some(key) = key {
                        pending.push((key, depth + 1))
                    }
                    pending.push((value, depth + 1));
                }
            }
            Expr::Transfer { value: Some(e), .. } => pending.push((e, depth + 1)),
            Expr::Binary(_, a, b) => {
                pending.push((a, depth + 1));
                pending.push((b, depth + 1));
            }
            Expr::Call(e, args) | Expr::SafeCall(e, args) => {
                pending.push((e, depth + 1));
                pending.extend(args.iter().map(|(_, e)| (e, depth + 1)));
            }
            Expr::Format(parts) => pending.extend(parts.iter().map(|part| (&part.value, depth + 1))),
            _ => {}
        }
    }
    Ok(())
}
