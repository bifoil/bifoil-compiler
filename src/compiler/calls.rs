//! Callable specialization, argument checking, and ordered call lowering.

use super::overloads::{CallableSpecialization, specialize_signature};
use super::{Compiler, FunctionContext, infer, invalidate_narrowing, static_symbol};
use crate::model::{Op, Type};
use crate::syntax::Expr;

#[derive(Clone, Copy)]
pub(super) enum CallKind {
    Function,
    SceneVisit,
    SceneTransfer,
}

pub(super) struct CallOptions<'a> {
    pub(super) safe: bool,
    pub(super) kind: CallKind,
    pub(super) expected_result: Option<&'a Type>,
}

impl Compiler {
    pub(super) fn call(
        &mut self,
        callee: &Expr,
        args: &[(Option<String>, Expr)],
        context: &mut FunctionContext,
        line: usize,
        options: CallOptions<'_>,
    ) -> Result<(), String> {
        let CallOptions {
            mut safe,
            kind,
            expected_result,
        } = options;
        let scene = match kind {
            CallKind::Function => None,
            CallKind::SceneVisit => Some(false),
            CallKind::SceneTransfer => Some(true),
        };
        safe |= matches!(super::overloads::callable_base(callee), Expr::Member { safe: true, .. });
        let resolved = specialize_signature(callee, Some(args), expected_result, &context.scopes)?;
        let callee_type = if let Some(specialization) = &resolved {
            specialization.signature.ty()
        } else {
            infer(callee, &context.scopes)?
        };
        let callable_view = crate::generics::member_view(&callee_type);
        let (params, suspends, vararg) = match &callable_view {
            Type::Function {
                params,
                suspend,
                vararg,
                ..
            } if scene.is_none() => (params.clone(), *suspend, *vararg),
            Type::Scene { params } if scene.is_some() => (params.clone(), true, false),
            _ => return Err("call kind does not match its function/scene target".into()),
        };
        if callee_type.permits_null() && !safe {
            return Err("nullable call requires safe invocation".into());
        }
        if suspends && !context.may_suspend {
            return Err("suspending call requires a suspend context".into());
        }
        if let Some(resolved) = &resolved {
            super::constants::check_call_bounds(&resolved.signature, callee, args)?;
        }
        if scene.is_none()
            && !safe
            && super::single_use_initializers::plain_call_name(callee, args).is_some()
            && let Some(resolved) = &resolved
            && resolved.signature.result == Type::Unit
            && self.expand_initializer_value(&resolved.signature.id, context, line)?
        {
            return Ok(());
        }
        let direct = resolved.as_ref().map(|specialization| specialization.signature.clone());
        let nullable_target = match super::overloads::callable_base(callee) {
            Expr::Member {
                receiver, safe: true, ..
            } => infer(receiver, &context.scopes)?.permits_null(),
            _ => callee_type.permits_null(),
        };
        let mut used = vec![false; params.len()];
        let mut named = false;
        let mut names = vec![];
        let mut spreads = vec![];
        let mut escaping_arguments = super::captures::Requirements::new();
        let receiver_requirements = if let Some(specialization) = &resolved {
            self.declared_callable(callee, specialization, context, line)?
        } else {
            self.expression(callee, context, line)?;
            Default::default()
        };
        let mut capture = resolved
            .as_ref()
            .and_then(|resolved| super::captures::callable_metadata(callee, &resolved.signature, &context.scopes));
        let mut requirements = if let Some(resolved) = &resolved {
            super::captures::callable_requirements(callee, &resolved.signature, &context.scopes)
        } else {
            context.value_requirements.clone()
        };
        let builtin = direct
            .as_ref()
            .is_some_and(|signature| signature.id.starts_with("$builtin."));
        if !builtin {
            requirements.extend(receiver_requirements.iter().cloned());
            escaping_arguments.extend(receiver_requirements.iter().cloned());
        }
        let result_requirements = if direct.as_ref().is_some_and(builtin_returns_contents) {
            receiver_requirements
        } else {
            Default::default()
        };
        let skipped = (safe && nullable_target).then(|| super::flow::SkippedPath::new(context));
        let jump = if safe {
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
        for (position, (argname, value)) in args.iter().enumerate() {
            let index = if let Some(name) = argname {
                named = true;
                direct
                    .as_ref()
                    .ok_or("function values do not expose parameter names")?
                    .params
                    .iter()
                    .position(|p| p.name == *name)
                    .ok_or_else(|| format!("unknown argument '{name}'"))?
            } else {
                if named {
                    return Err("positional arguments must precede named arguments".into());
                }
                if vararg && !params.is_empty() {
                    position.min(params.len() - 1)
                } else {
                    position
                }
            };
            if index >= params.len() {
                return Err("too many arguments".into());
            }
            let variadic = vararg && index + 1 == params.len();
            if used[index] && (!variadic || argname.is_some() || named) {
                return Err("duplicate argument".into());
            }
            used[index] = true;
            let spread = matches!(value, Expr::Spread(_));
            if spread && !variadic {
                return Err("spread arguments supply only the variadic parameter".into());
            }
            let actual_value = if let Expr::Spread(e) = value { e.as_ref() } else { value };
            let expected = if variadic && (spread || argname.is_some()) {
                Type::named("List", vec![params[index].clone()])
            } else {
                params[index].clone()
            };
            let actual = self.expression_as(actual_value, context, line, Some(&expected))?;
            context.convert(&actual, &expected, line)?;
            requirements.extend(context.value_requirements.iter().cloned());
            escaping_arguments.extend(context.value_requirements.iter().cloned());
            names.push(argname.clone());
            spreads.push(spread);
        }
        for (index, provided) in used.iter().enumerate() {
            if !provided
                && !(vararg && index + 1 == params.len())
                && !direct.as_ref().is_some_and(|s| s.params[index].default)
            {
                return Err(format!("missing required argument at position {}", index + 1));
            }
        }
        // Only an invoked forward helper needs an early body check. Supplied
        // arguments may initialize dependencies or transfer control before it.
        // These plans select immutable named declarations, so refresh their
        // refined metadata while retaining the already captured argument values.
        if context.is_reachable
            && let Some(resolved) = &resolved
            && self.precheck_forward_initializer(&resolved.signature.id, context)?
        {
            capture = super::captures::callable_metadata(callee, &resolved.signature, &context.scopes);
            requirements = super::captures::callable_requirements(callee, &resolved.signature, &context.scopes);
            requirements.extend(escaping_arguments.iter().cloned());
        }
        if let Some(capture) = &capture {
            for (index, names) in &capture.defaults {
                if !used.get(*index).copied().unwrap_or(false) {
                    requirements.extend(names.iter().cloned());
                }
            }
        }
        if context.is_reachable {
            if let Some(signature) = &direct {
                self.check_initializer_call(&signature.id, context)?;
            }
            super::captures::check_requirements(
                &requirements,
                &context.scopes,
                &context.function_id,
                &mut self.empty_dependencies,
            )?;
            super::captures::mark_escaped(&escaping_arguments, &mut context.scopes);
            if let Some(capture) = &capture {
                let mut publications = capture.publications.clone();
                for (index, effects) in &capture.default_publications {
                    if !used.get(*index).copied().unwrap_or(false) {
                        publications.extend(effects.iter().cloned());
                    }
                }
                super::captures::mark_published(&publications, &mut context.scopes, &mut self.empty_publications);
            } else if direct.is_none() {
                // Calling an opaque value can retain its existing payload,
                // without retaining the variable from which it was loaded.
                super::captures::mark_escaped(&requirements, &mut context.scopes);
            }
        }
        if let Some(transfer) = scene {
            context.emit(
                Op::SceneCall {
                    names,
                    direct: direct.is_some(),
                    transfer,
                },
                line,
            );
        } else {
            context.emit(
                Op::Call {
                    names,
                    direct: direct.is_some(),
                    spreads,
                },
                line,
            );
        }
        if matches!(&callable_view, Type::Function { result, .. } if **result == Type::Nothing) {
            context.is_reachable = false;
        }
        if !builtin {
            invalidate_narrowing(&mut context.scopes);
        } else if let Some(signature) = &direct {
            super::field_proofs::invalidate_builtin(
                super::members::builtin_heap_effect(&signature.id),
                &mut context.scopes,
            );
        }
        if context.is_reachable
            && scene.is_none()
            && let Some(signature) = &direct
            && let Some(initialized) = self.initialized_captures.get(&signature.id)
        {
            super::capture_initialization::apply(initialized, context);
        }
        if let Some(jump) = jump {
            context.patch(jump, context.instructions.len())
        }
        if let Some(skipped) = skipped {
            skipped.merge(context);
        }
        context.value_requirements = result_requirements;
        Ok(())
    }
}

impl Compiler {
    /// Capture a selected callable before evaluating any supplied arguments.
    pub(super) fn declared_callable(
        &mut self,
        expression: &Expr,
        resolved: &CallableSpecialization,
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<super::captures::Requirements, String> {
        let mut requirements = Default::default();
        let base = super::overloads::callable_base(expression);
        let signature = &resolved.signature;
        let safe = matches!(base, Expr::Member { safe: true, .. });
        if let Expr::Member { receiver, name, .. } | Expr::BoundReference(receiver, name) = base
            && static_symbol(base, &context.scopes).is_none()
        {
            let receiver_type = infer(receiver, &context.scopes)?;
            if receiver_type.permits_null() && !safe {
                return Err("nullable member receiver requires ?. or a null check".into());
            }
            super::access_member(receiver_type.present(), name, &context.scopes)?;
            crate::declarations::check_method_access(
                receiver_type.present(),
                &signature.id,
                super::bindings::current_receiver(&context.scopes),
            )?;
            self.expression(receiver, context, line)?;
            requirements = context.value_requirements.clone();
            let skip = if safe {
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
            context.emit(
                Op::Member {
                    name: name.clone(),
                    ty: signature.ty(),
                    function: Some(signature.id.clone()),
                },
                line,
            );
            if let Some(skip) = skip {
                context.patch(skip, context.instructions.len());
            }
        } else {
            context.emit(
                Op::Load {
                    name: signature.id.clone(),
                },
                line,
            );
        }
        if !resolved.type_arguments.is_empty() {
            let skip = if safe {
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
            context.emit(
                Op::Specialize {
                    args: resolved.type_arguments.clone(),
                },
                line,
            );
            if let Some(skip) = skip {
                context.patch(skip, context.instructions.len());
            }
        }
        Ok(requirements)
    }
}

/// Copies and extraction methods retain latent reads in stored callbacks. The
/// other built-ins inspect or mutate containers without invoking their contents.
fn builtin_returns_contents(signature: &crate::model::Signature) -> bool {
    match signature.id.as_str() {
        "$builtin.toList"
        | "$builtin.toMutableList"
        | "$builtin.toSet"
        | "$builtin.toMutableSet"
        | "$builtin.toMap"
        | "$builtin.toMutableMap"
        | "$builtin.removeAt"
        | "$builtin.put" => true,
        "$builtin.remove" => signature.result != Type::Boolean,
        _ => false,
    }
}
