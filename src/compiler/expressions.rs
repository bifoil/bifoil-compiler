//! Expression and assignment-address lowering.

use super::overloads::{reference_needs_resolution, reference_type};
use super::{
    Compiler, FunctionContext, access_member, binary_type, constant_case, constant_number, enum_shorthand, index_types,
    infer, infer_as, lambda_definition, lookup, member_type, merge_env, narrow_condition, pending_type, signature,
    specialize_signature, static_symbol, yieldize,
};
use crate::model::{Op, Type};
use crate::syntax::Expr;
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Arc;

impl Compiler {
    pub(super) fn expression(&mut self, e: &Expr, context: &mut FunctionContext, line: usize) -> Result<Type, String> {
        self.expression_as(e, context, line, None)
    }

    pub(super) fn expression_as(
        &mut self,
        e: &Expr,
        context: &mut FunctionContext,
        line: usize,
        expected: Option<&Type>,
    ) -> Result<Type, String> {
        self.check_budget(context)?;
        super::members::check_static_receiver(e, &context.scopes)?;
        constant_number(e)?;
        if reference_needs_resolution(e, &context.scopes)
            && let Some(resolved) = specialize_signature(e, None, expected, &context.scopes)?
        {
            let ty = reference_type(e, resolved.signature.ty());
            let receiver_requirements = self.declared_callable(e, &resolved, context, line)?;
            context.value_requirements =
                super::captures::callable_requirements(e, &resolved.signature, &context.scopes);
            context.value_requirements.extend(receiver_requirements);
            return Ok(ty);
        }
        let special = static_symbol(e, &context.scopes).or(if let Expr::Name(n) = e {
            enum_shorthand(n, &context.scopes, expected)?
        } else {
            None
        });
        if let Some(sym) = special {
            context.value_requirements = sym.requires.clone();
            let ty = if constant_case(&sym) {
                let sig = sym.signature().unwrap();
                context.emit(
                    Op::Construct {
                        ty: sig.result.clone(),
                        fields: vec![],
                        tag: sig.id.rsplit('.').next().map(str::to_string),
                    },
                    line,
                );
                sig.result.clone()
            } else {
                context.emit(Op::Load { name: sym.runtime }, line);
                sym.ty
            };
            return Ok(ty);
        }
        let mut ty = infer_as(e, &context.scopes, expected)?;
        if pending_type(&ty) {
            return Err("return type is unresolved; add an annotation".into());
        }
        let mut requirements = std::collections::BTreeSet::new();
        match e {
            Expr::TypeApply { .. } => return Err("type arguments require generic callable".into()),
            Expr::Construct { ty, fields, tag } => {
                for (name, e) in fields {
                    let expected = crate::declarations::field(ty, name)
                        .ok_or("unknown constructed field")?
                        .ty;
                    let actual = self.expression_as(e, context, line, Some(&expected))?;
                    context.convert(&actual, &expected, line)?;
                }
                context.emit(
                    Op::Construct {
                        ty: ty.clone(),
                        fields: fields.iter().map(|(n, _)| n.clone()).collect(),
                        tag: tag.clone(),
                    },
                    line,
                );
            }
            Expr::Block(body) => {
                let body = yieldize(body, &ty)?;
                context.value_scopes.push((context.scopes.len(), Default::default()));
                context.emit(Op::Enter, line);
                context.scopes.push(Arc::default());
                self.body(&body, context)?;
                requirements = context.value_scopes.pop().unwrap().1;
                context.exit_scope()?;
                context.emit(Op::Exit, line);
            }
            Expr::Lambda(fun) => {
                let capture = super::captures::callable_captures(fun);
                let capture_names = capture.all_names();
                requirements = capture
                    .body
                    .into_iter()
                    .filter(|required| super::captures::capture_binding(&context.scopes, required.name()).is_some())
                    .collect();
                let fun = lambda_definition(fun, &context.scopes, expected)?;
                let id = format!("{}$lambda:{}:{}", context.function_id, line, context.instructions.len());
                let sig = signature(&fun, id.clone());
                self.function(&fun, super::functions::FunctionEnvironment::new(&context.scopes), sig)?;
                let captures = super::captures::runtime_capture_names(&capture_names, &context.scopes);
                context.emit(
                    Op::FunctionValue {
                        function: id,
                        captures: Some(captures),
                    },
                    line,
                );
            }
            Expr::Int(n) => {
                context.emit(
                    Op::Constant {
                        ty: Type::Int,
                        value: json!(*n),
                    },
                    line,
                );
            }
            Expr::Float(n) => {
                context.emit(
                    Op::Constant {
                        ty: Type::Float,
                        value: json!(*n),
                    },
                    line,
                );
            }
            Expr::Boolean(n) => {
                context.emit(
                    Op::Constant {
                        ty: Type::Boolean,
                        value: json!(*n),
                    },
                    line,
                );
            }
            Expr::String(s) | Expr::Char(s) => {
                context.emit(
                    Op::Constant {
                        ty: if matches!(e, Expr::Char(_)) {
                            Type::Char
                        } else {
                            Type::String
                        },
                        value: json!(s),
                    },
                    line,
                );
            }
            Expr::Null => {
                context.emit(
                    Op::Constant {
                        ty: Type::Null,
                        value: json!(null),
                    },
                    line,
                );
            }
            Expr::Format(parts) => {
                for part in parts {
                    let t = self.expression(&part.value, context, part.line)?;
                    if !t.renderable() {
                        return Err("interpolation requires a renderable non-null primitive".into());
                    }
                }
                context.emit(Op::Format { count: parts.len() }, line);
            }
            Expr::Name(n) if n == "Unit" => {
                context.emit(unit(), line);
            }
            Expr::Name(n) | Expr::Reference(n) => {
                let sym = lookup(&context.scopes, n).ok_or_else(|| format!("unknown binding '{n}'"))?;
                if context.is_reachable && sym.owner == context.function_id && !sym.initialized {
                    return Err(format!("binding '{n}' is not definitely initialized"));
                }
                if matches!(e, Expr::Reference(_)) && sym.signature().is_none() {
                    return Err(":: requires a declared function or scene".into());
                }
                requirements = sym.requires;
                context.emit(Op::Load { name: sym.runtime }, line);
            }
            Expr::Group(e) => return self.expression_as(e, context, line, expected),
            Expr::Unary(op, e) => {
                if op == "-" && matches!(e.as_ref(), Expr::Int(2147483648)) {
                    context.emit(
                        Op::Constant {
                            ty: Type::Int,
                            value: json!(i32::MIN),
                        },
                        line,
                    );
                } else {
                    self.expression(e, context, line)?;
                    context.emit(Op::Unary { operator: op.clone() }, line);
                }
            }
            Expr::Binary(op, a, b) => {
                if op == "?:" {
                    let left = self.expression(a, context, line)?;
                    requirements.extend(context.value_requirements.iter().cloned());
                    let base = context.scopes.clone();
                    let incoming = context.is_reachable;
                    context.emit(Op::Dup, line);
                    let right = context.emit(
                        Op::JumpNull {
                            target: 0,
                            when_null: true,
                        },
                        line,
                    );
                    if left != Type::Null {
                        context.convert(&left.non_null(), &ty, line)?;
                    }
                    let done = context.emit(Op::Jump { target: 0 }, line);
                    context.patch(right, context.instructions.len());
                    context.emit(Op::Pop, line);
                    context.is_reachable = incoming && left.permits_null();
                    let actual = self.expression_as(b, context, line, Some(&ty))?;
                    requirements.extend(context.value_requirements.iter().cloned());
                    context.convert(&actual, &ty, line)?;
                    let rhs = context.scopes.clone();
                    let rhs_reachable = context.is_reachable;
                    context.patch(done, context.instructions.len());
                    let mut exits = vec![];
                    if left != Type::Null && incoming {
                        exits.push(base.clone())
                    }
                    if rhs_reachable {
                        exits.push(rhs)
                    }
                    context.scopes = merge_env(&base, &exits);
                    context.is_reachable = !exits.is_empty();
                } else {
                    // Keep the value type captured before later operand effects.
                    let left = self.expression(a, context, line)?;
                    if op == "&&" || op == "||" {
                        context.emit(Op::Dup, line);
                        let jump = context.emit(
                            if op == "&&" {
                                Op::JumpFalse { target: 0 }
                            } else {
                                Op::JumpTrue { target: 0 }
                            },
                            line,
                        );
                        context.emit(Op::Pop, line);
                        let skipped = super::flow::SkippedPath::new(context);
                        let known = super::constants::constant_boolean(a);
                        let forced = known == Some(op == "&&");
                        let never = known == Some(op != "&&");
                        context.is_reachable &= !never;
                        narrow_condition(a, &mut context.scopes, op == "&&");
                        let right = self.expression(b, context, line)?;
                        ty = binary_type(op, &left, &right)?;
                        if !forced {
                            skipped.merge(context);
                        }
                        context.patch(jump, context.instructions.len());
                    } else {
                        let right = self.expression(b, context, line)?;
                        ty = binary_type(op, &left, &right)?;
                        if ["..", "..<", "downTo"].contains(&op.as_str()) {
                            if let (Some((_, a)), Some((_, b))) = (constant_number(a)?, constant_number(b)?)
                                && ((op == ".." && a > b) || (op == "..<" && a >= b))
                            {
                                return Err("constant ascending range is empty or reversed".into());
                            }
                            context.emit(
                                Op::Range {
                                    descending: op == "downTo",
                                    exclusive: op == "..<",
                                },
                                line,
                            );
                        } else if op == "step" {
                            if constant_number(b)?.is_some_and(|(_, n)| n <= 0.0) {
                                return Err("constant range step must be positive".into());
                            }
                            context.emit(Op::RangeStep, line);
                        } else {
                            context.emit(Op::Binary { operator: op.clone() }, line);
                        }
                    }
                }
            }
            Expr::Call(callee, args) | Expr::SafeCall(callee, args) => {
                self.call(
                    callee,
                    args,
                    context,
                    line,
                    CallOptions {
                        safe: matches!(e, Expr::SafeCall(..)),
                        kind: CallKind::Function,
                        expected_result: Some(ty.present()),
                    },
                )?;
                requirements = context.value_requirements.clone();
            }
            Expr::Collection { entries, .. } => {
                let (family, args, _) = ty.collection().ok_or("invalid collection type")?;
                if family == "Map" {
                    let mut keys = BTreeSet::new();
                    for (key, _) in entries {
                        if let Some(key) = key
                            .as_ref()
                            .and_then(|key| super::field_proofs::constant_key(key, &context.scopes, &args[0]))
                            && !keys.insert(key)
                        {
                            return Err("duplicate constant map key".into());
                        }
                    }
                }
                context.emit(Op::BeginCollection { ty: ty.clone() }, line);
                for (key, value) in entries {
                    if let Some(key) = key {
                        let actual = self.expression_as(key, context, line, Some(&args[0]))?;
                        context.convert(&actual, &args[0], line)?;
                        context.emit(Op::CheckKey, line);
                    }
                    let target = &args[if family == "Map" { 1 } else { 0 }];
                    let actual = self.expression_as(value, context, line, Some(target))?;
                    requirements.extend(context.value_requirements.iter().cloned());
                    context.convert(&actual, target, line)?;
                    context.emit(
                        if family == "Map" {
                            Op::CollectionPut
                        } else {
                            Op::CollectionAdd
                        },
                        line,
                    );
                }
            }
            Expr::Member { receiver, name, safe } => {
                self.expression(receiver, context, line)?;
                requirements = context.value_requirements.clone();
                let jump = if *safe {
                    context.emit(Op::Dup, line);
                    Some(context.emit(
                        Op::JumpNull {
                            target: 0,
                            when_null: true,
                        },
                        line,
                    ))
                } else {
                    None
                };
                let receiver_type = infer(receiver, &context.scopes)?;
                let member = member_type(receiver_type.present(), name)?;
                let function = super::members::resolved_member_target(receiver_type.present(), name);
                context.emit(
                    Op::Member {
                        name: name.clone(),
                        ty: member,
                        function,
                    },
                    line,
                );
                if let Some(jump) = jump {
                    context.patch(jump, context.instructions.len())
                }
            }
            Expr::BoundReference(receiver, name) => {
                let receiver_type = infer(receiver, &context.scopes)?;
                let member = member_type(&receiver_type, name)?;
                let function = super::members::resolved_member_target(&receiver_type, name);
                if !matches!(member, Type::Function { .. }) {
                    return Err("bound reference requires a method".into());
                }
                self.expression(receiver, context, line)?;
                requirements = context.value_requirements.clone();
                context.emit(
                    Op::Member {
                        name: name.clone(),
                        ty: member,
                        function,
                    },
                    line,
                );
            }
            Expr::Index { receiver, index, safe } => {
                let base = infer(receiver, &context.scopes)?;
                let (key, _) = index_types(base.present())?;
                super::constants::check_index(receiver, index, base.present())?;
                self.expression(receiver, context, line)?;
                requirements = context.value_requirements.clone();
                let jump = if *safe {
                    context.emit(Op::Dup, line);
                    Some(context.emit(
                        Op::JumpNull {
                            target: 0,
                            when_null: true,
                        },
                        line,
                    ))
                } else {
                    None
                };
                let skipped = (*safe && base.permits_null()).then(|| super::flow::SkippedPath::new(context));
                let actual = self.expression_as(index, context, line, Some(&key))?;
                context.convert(&actual, &key, line)?;
                context.emit(Op::Index, line);
                if let Some(jump) = jump {
                    context.patch(jump, context.instructions.len())
                }
                if let Some(skipped) = skipped {
                    skipped.merge(context);
                }
            }
            Expr::Unwrap(e) => {
                let source_expected = expected.map(Type::nullable);
                self.expression_as(e, context, line, source_expected.as_ref())?;
                requirements = context.value_requirements.clone();
                context.emit(Op::AssertPresent, line);
                super::flow::narrow_present(e, &mut context.scopes);
            }
            Expr::Spread(_) => return Err("spread is only valid in a variadic call".into()),
            Expr::TypeOp { value, operator, ty } => {
                self.expression(value, context, line)?;
                requirements = context.value_requirements.clone();
                context.emit(
                    if operator == "is" || operator == "!is" {
                        Op::TypeTest {
                            ty: ty.clone(),
                            negate: operator == "!is",
                        }
                    } else {
                        Op::Cast {
                            ty: ty.clone(),
                            safe: operator == "as?",
                        }
                    },
                    line,
                );
            }
            Expr::Transfer { kind, label, value } => match kind.as_str() {
                "return" => {
                    self.return_value(&value.as_deref().cloned(), context, line)?;
                }
                "break" | "continue" => self.loop_transfer(kind, label, context, line)?,
                "goto" | "visit" => {
                    let Some(value) = value else {
                        return Err("scene operation requires a destination".into());
                    };
                    let Expr::Call(callee, args) = value.as_ref() else {
                        return Err("scene operation requires destination(arguments)".into());
                    };
                    if !context.may_suspend {
                        return Err("scene operation requires a suspension-capable context".into());
                    }
                    self.call(
                        callee,
                        args,
                        context,
                        line,
                        CallOptions {
                            safe: false,
                            kind: if kind == "goto" {
                                CallKind::SceneTransfer
                            } else {
                                CallKind::SceneVisit
                            },
                            expected_result: None,
                        },
                    )?;
                    if kind == "goto" {
                        context.is_reachable = false
                    }
                }
                _ => return Err("invalid control transfer".into()),
            },
        }
        // Primitive results cannot contain deferred closures, even when their
        // receiver does (for example a list's size or a runtime type test).
        if matches!(
            ty.present(),
            Type::Int
                | Type::Float
                | Type::Boolean
                | Type::String
                | Type::Char
                | Type::Unit
                | Type::Null
                | Type::Nothing
        ) {
            requirements.clear();
        }
        context.value_requirements = requirements;
        if ty == Type::Nothing {
            context.is_reachable = false;
        }
        Ok(ty)
    }

    pub(super) fn address(
        &mut self,
        target: &Expr,
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<Type, String> {
        match target {
            Expr::Index {
                receiver,
                index,
                safe: false,
            } => {
                let actual = infer(receiver, &context.scopes)?;
                if actual.permits_null() {
                    return Err("nullable assignment receiver requires a null check".into());
                }
                let base = crate::generics::member_view(&actual);
                let Some((family, args, true)) = base.collection() else {
                    return Err("indexed assignment requires a mutable list or map".into());
                };
                if family == "Set" {
                    return Err("Set cannot be indexed".into());
                }
                let (key, _) = index_types(&base)?;
                super::constants::check_index(receiver, index, &base)?;
                self.expression(receiver, context, line)?;
                let actual = self.expression_as(index, context, line, Some(&key))?;
                context.convert(&actual, &key, line)?;
                context.emit(Op::AddressIndex, line);
                Ok(args[if family == "Map" { 1 } else { 0 }].clone())
            }
            Expr::Member {
                receiver,
                name,
                safe: false,
            } => {
                let ty = infer(receiver, &context.scopes)?;
                if ty.permits_null() {
                    return Err("nullable assignment receiver requires a null check".into());
                }
                access_member(&ty, name, &context.scopes)?;
                let field = crate::declarations::field(&ty, name).ok_or("property is not a mutable data field")?;
                if !field.mutable {
                    return Err("cannot assign to val field".into());
                }
                self.expression(receiver, context, line)?;
                context.emit(Op::AddressMember { name: name.clone() }, line);
                Ok(field.ty)
            }
            _ => Err("invalid assignment target".into()),
        }
    }
}

use crate::model::unit;

use super::calls::{CallKind, CallOptions};
