//! Case coverage, payload bindings, and when-statement lowering.

use super::flow::FlowJoin;
use super::{Binding, Compiler, FunctionContext, binary_type, insert, merge_env, narrow_case, pure_expression};
use crate::model::{Op, Type};
use crate::syntax::{Expr, Pattern, WhenCase};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Resolve case aliases to declaration identities before proving coverage.
/// Source spelling is insufficient after an import or expected-type shorthand.
fn constant_variant(value: &Expr, expected: &Type, env: &super::ScopeStack) -> Result<Option<String>, String> {
    if let Expr::Group(inner) = value {
        return constant_variant(inner, expected, env);
    }
    let binding = if let Some(binding) = super::static_symbol(value, env) {
        Some(binding)
    } else if let Expr::Name(name) = value {
        super::enum_shorthand(name, env, Some(expected))?
    } else {
        None
    };
    Ok(binding.filter(super::constant_case).map(|binding| binding.runtime))
}

pub(super) fn when_exhaustive(
    subject: Option<&Type>,
    cases: &[WhenCase],
    env: &super::ScopeStack,
) -> Result<bool, String> {
    if cases.iter().any(|context| context.fallback) {
        return Ok(true);
    }
    let view = subject.map(|ty| crate::generics::member_view(ty.present()));
    if let Some(Type::Named { name, args }) = view.as_ref()
        && let Some(def) = crate::declarations::definition(name)
    {
        let covered = cases
            .iter()
            .filter(|context| {
                context
                    .guard
                    .as_ref()
                    .is_none_or(|guard| super::constants::constant_boolean(guard) == Some(true))
            })
            .flat_map(|context| &context.patterns)
            .collect::<Vec<_>>();
        let mut constant_variants = BTreeSet::new();
        if def.kind == "enum" || args.is_empty() {
            for pattern in &covered {
                if let Pattern::Value(value) = pattern
                    && let Some(id) = constant_variant(value, subject.unwrap(), env)?
                {
                    constant_variants.insert(id);
                }
            }
        }
        let all = def.variants.iter().all(|variant| {
            let possible = if def.kind == "enum" {
                view.as_ref().unwrap().clone()
            } else {
                Type::Named {
                    name: variant.clone(),
                    args: args.clone(),
                }
            };
            constant_variants.contains(variant)
                || covered.iter().any(
                    |pattern| matches!(pattern, Pattern::Type { ty, negate: false, .. } if possible.subtype_of(ty)),
                )
        });
        let null = subject.is_none_or(|s| !s.permits_null())
            || covered.iter().any(|p| matches!(p, Pattern::Value(Expr::Null)));
        if !def.variants.is_empty() && all && null {
            return Ok(true);
        }
    }
    let mut yes = false;
    let mut no = false;
    let mut null = false;
    let mut whole = false;
    for case in cases {
        if case
            .guard
            .as_ref()
            .is_some_and(|guard| super::constants::constant_boolean(guard) != Some(true))
        {
            continue;
        }
        for pattern in &case.patterns {
            match pattern {
                Pattern::Value(Expr::Boolean(value)) => {
                    if subject.is_none() && *value {
                        whole = true
                    }
                    if *value { yes = true } else { no = true }
                }
                Pattern::Value(Expr::Null) => null = true,
                Pattern::Type { ty, negate: false, .. } if subject.is_some_and(|s| s.subtype_of(ty)) => whole = true,
                _ => {}
            }
        }
    }
    Ok(whole || subject.is_some_and(|s| view == Some(Type::Boolean) && yes && no && (!s.permits_null() || null)))
}

pub(super) fn pattern_bindings(pattern: &Pattern) -> Result<Vec<(String, String, Type)>, String> {
    let Pattern::Type {
        ty,
        bindings: Some(bindings),
        negate,
    } = pattern
    else {
        return Ok(vec![]);
    };
    if *negate {
        return Err("negative type patterns cannot bind payloads".into());
    }
    let Type::Named { name, .. } = ty else {
        return Err("payload pattern requires a variant type".into());
    };
    let d = crate::declarations::definition(name).ok_or("unknown variant type")?;
    if d.kind != "variant" || bindings.len() != d.field_order.len() {
        return Err("payload pattern requires every variant field or _".into());
    }
    let mut seen = BTreeSet::new();
    let mut result = vec![];
    for (binding, field) in bindings.iter().zip(&d.field_order) {
        if binding == "_" {
            continue;
        }
        if !seen.insert(binding) {
            return Err("duplicate pattern binding".into());
        }
        result.push((
            binding.clone(),
            field.clone(),
            crate::declarations::field(ty, field).unwrap().ty,
        ));
    }
    Ok(result)
}

pub(super) fn case_bindings(case: &WhenCase, subject: Option<&Type>) -> Result<Vec<(String, Type)>, String> {
    let mut joined: Option<BTreeMap<String, Type>> = None;
    for pattern in &case.patterns {
        if let Pattern::Type { ty, .. } = pattern {
            let subject = subject.ok_or("type pattern requires a subject")?;
            let view = crate::generics::member_view(subject.present());
            if !view.subtype_of(ty) && !ty.subtype_of(&view) && view != Type::Any {
                return Err("impossible type pattern".into());
            }
        }
        let bindings = pattern_bindings(pattern)?
            .into_iter()
            .map(|(name, _, ty)| (name, ty))
            .collect::<BTreeMap<_, _>>();
        if let Some(old) = &mut joined {
            if old.keys().ne(bindings.keys()) {
                return Err("case alternatives must bind the same names".into());
            }
            for (name, ty) in bindings {
                let prev = old[&name].clone();
                old.insert(
                    name,
                    prev.join(&ty)
                        .ok_or("pattern alternatives have incompatible bindings")?,
                );
            }
        } else {
            joined = Some(bindings)
        }
    }
    Ok(joined.unwrap_or_default().into_iter().collect())
}

impl Compiler {
    pub(super) fn when_statement(
        &mut self,
        subject: &Option<Expr>,
        binding: &Option<String>,
        annotation: &Option<Type>,
        cases: &[WhenCase],
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        context.emit(Op::Enter, line);
        context.scopes.push(Arc::default());
        let temporary = format!("$when:{}", context.instructions.len());
        let mut subject_requirements = Default::default();
        let mut subject_source = None;
        let subject_type = if let Some(subject) = subject {
            let actual = self.expression_as(subject, context, line, annotation.as_ref())?;
            subject_requirements = context.value_requirements.clone();
            let target = annotation.clone().unwrap_or(actual.clone());
            subject_source = super::flow::SubjectSource::capture(subject, &actual, &target, &context.scopes);
            context.convert(&actual, &target, line)?;
            if let Some(name) = binding {
                insert(
                    &mut context.scopes,
                    name,
                    Binding {
                        identity: crate::compiler::bindings::Identity::new(),
                        index_key: false,
                        global_aliases: None,
                        volatile: false,
                        implicit_receiver: false,
                        escaped: false,
                        runtime: name.clone(),
                        ty: target.clone(),
                        mutable: false,
                        initialized: true,
                        may_be_initialized: true,
                        owner: context.function_id.clone(),
                        narrow: None,
                        field_narrows: None,
                        callables: vec![],
                        requires: subject_requirements.clone(),
                        callable_requirements: Default::default(),
                    },
                )?;
                context.emit(Op::Dup, line);
                context.emit(
                    Op::Declare {
                        name: name.clone(),
                        ty: target.clone(),
                        mutable: false,
                    },
                    line,
                );
            }
            context.emit(
                Op::Declare {
                    name: temporary.clone(),
                    ty: target.clone(),
                    mutable: false,
                },
                line,
            );
            Some(target)
        } else {
            None
        };
        let base = context.scopes.clone();
        let incoming = context.is_reachable;
        let mut remaining = incoming.then(|| base.clone());
        let mut ends = vec![];
        let mut paths = FlowJoin::default();
        let mut stable = true;
        let mut null_covered = false;
        for case in cases {
            context.scopes = remaining.clone().unwrap_or_else(|| base.clone());
            if !stable {
                super::invalidate_narrowing(&mut context.scopes);
            }
            context.is_reachable = remaining.is_some();
            let mut matches = vec![];
            let bindings = case_bindings(case, subject_type.as_ref())?;
            let mut matched = vec![];
            if !case.fallback {
                for pattern in &case.patterns {
                    match pattern {
                        Pattern::Value(value) => {
                            if let Some(subject) = &subject_type {
                                context.emit(
                                    Op::Load {
                                        name: temporary.clone(),
                                    },
                                    case.line,
                                );
                                let actual = self.expression_as(value, context, case.line, Some(subject))?;
                                binary_type("==", subject, &actual)?;
                                context.emit(Op::Binary { operator: "==".into() }, case.line);
                            } else if !Type::Boolean.accepts(&self.expression(value, context, case.line)?) {
                                return Err("subjectless case requires Boolean".into());
                            }
                            stable &= pure_expression(value);
                        }
                        Pattern::Membership { value, negate } => {
                            let subject = subject_type.as_ref().ok_or("membership case requires a subject")?;
                            context.emit(
                                Op::Load {
                                    name: temporary.clone(),
                                },
                                case.line,
                            );
                            let actual = self.expression(value, context, case.line)?;
                            let operator = if *negate { "!in" } else { "in" };
                            binary_type(operator, subject, &actual)?;
                            context.emit(
                                Op::Binary {
                                    operator: operator.into(),
                                },
                                case.line,
                            );
                            stable &= pure_expression(value);
                        }
                        Pattern::Type { ty, negate, .. } => {
                            if subject_type.is_none() {
                                return Err("type pattern requires a subject".into());
                            }
                            context.emit(
                                Op::Load {
                                    name: temporary.clone(),
                                },
                                case.line,
                            );
                            context.emit(
                                Op::TypeTest {
                                    ty: ty.clone(),
                                    negate: *negate,
                                },
                                case.line,
                            );
                        }
                    }
                    // Each successful alternative skips the remaining tests.
                    // The failed path retains effects before trying the next one.
                    let known = match pattern {
                        Pattern::Value(value) => {
                            if let Some(subject) = subject {
                                super::constants::constant_comparison("==", subject, value)
                            } else {
                                super::constants::constant_boolean(value)
                            }
                        }
                        _ => None,
                    };
                    if context.is_reachable && known != Some(false) {
                        let mut matched = context.scopes.clone();
                        if let Pattern::Value(value) = pattern
                            && subject.is_none()
                        {
                            super::narrow_condition(value, &mut matched, true);
                        }
                        matches.push(matched);
                    }
                    context.is_reachable &= known != Some(true);
                    if let Pattern::Value(value) = pattern
                        && subject.is_none()
                    {
                        super::narrow_condition(value, &mut context.scopes, false);
                    }
                    let fail = context.emit(Op::JumpFalse { target: 0 }, case.line);
                    context.emit(Op::Enter, case.line);
                    for (name, field, actual) in pattern_bindings(pattern)? {
                        let expected = bindings.iter().find(|(n, _)| n == &name).unwrap().1.clone();
                        context.emit(
                            Op::Load {
                                name: temporary.clone(),
                            },
                            case.line,
                        );
                        context.emit(
                            Op::Member {
                                name: field,
                                ty: actual.clone(),
                                function: None,
                            },
                            case.line,
                        );
                        context.convert(&actual, &expected, case.line)?;
                        context.emit(
                            Op::Declare {
                                name,
                                ty: expected,
                                mutable: false,
                            },
                            case.line,
                        );
                    }
                    matched.push(context.emit(Op::Jump { target: 0 }, case.line));
                    context.patch(fail, context.instructions.len());
                }
            }
            if case.fallback && context.is_reachable {
                matches.push(context.scopes.clone());
            }
            let mut unmatched = vec![];
            if !case.fallback && context.is_reachable {
                unmatched.push(context.scopes.clone());
            }
            context.scopes = merge_env(&base, &matches);
            context.is_reachable = !matches.is_empty();
            let next = if case.fallback {
                None
            } else {
                Some(context.emit(Op::Jump { target: 0 }, case.line))
            };
            let start = context.instructions.len();
            for jump in matched {
                context.patch(jump, start)
            }
            if case.fallback {
                context.emit(Op::Enter, case.line);
            }
            context.scopes.push(Arc::default());
            for (name, ty) in bindings {
                insert(
                    &mut context.scopes,
                    &name,
                    Binding {
                        identity: crate::compiler::bindings::Identity::new(),
                        index_key: false,
                        global_aliases: None,
                        volatile: false,
                        implicit_receiver: false,
                        escaped: false,
                        runtime: name.clone(),
                        ty,
                        mutable: false,
                        initialized: true,
                        may_be_initialized: true,
                        owner: context.function_id.clone(),
                        narrow: None,
                        field_narrows: None,
                        callables: vec![],
                        requires: subject_requirements.clone(),
                        callable_requirements: Default::default(),
                    },
                )?;
            }
            let original = subject_source.as_ref().and_then(|source| source.original(stable));
            narrow_case(
                case,
                subject_type.as_ref(),
                binding.as_deref(),
                original,
                &mut context.scopes,
            );
            if case.fallback && null_covered {
                super::flow::narrow_when_remainder(binding.as_deref(), original, &mut context.scopes);
            }
            let guarded = if let Some(guard) = &case.guard {
                if !Type::Boolean.accepts(&self.expression(guard, context, case.line)?) {
                    return Err("case guard requires Boolean".into());
                }
                stable &= pure_expression(guard);
                let known = super::constants::constant_boolean(guard);
                if context.is_reachable && known != Some(true) {
                    let mut failed = super::captures::leaving_scopes(&context.scopes, base.len())?;
                    super::narrow_condition(guard, &mut failed, false);
                    unmatched.push(failed);
                }
                context.is_reachable &= known != Some(false);
                super::narrow_condition(guard, &mut context.scopes, true);
                Some(context.emit(Op::JumpFalse { target: 0 }, case.line))
            } else {
                None
            };
            remaining = (!unmatched.is_empty()).then(|| merge_env(&base, &unmatched));
            self.body(&case.body, context)?;
            context.exit_scope()?;
            context.emit(Op::Exit, case.line);
            if context.is_reachable {
                paths.add(&base, context.scopes.clone())
            }
            ends.push(context.emit(Op::Jump { target: 0 }, case.line));
            if let Some(jump) = guarded {
                context.patch(jump, context.instructions.len());
                context.emit(Op::Exit, case.line);
            }
            if let Some(jump) = next {
                context.patch(jump, context.instructions.len())
            }
            if case
                .guard
                .as_ref()
                .is_none_or(|guard| super::constants::constant_boolean(guard) == Some(true))
                && case.patterns.iter().any(|p| matches!(p, Pattern::Value(Expr::Null)))
            {
                null_covered = true
            }
        }
        let exhaustive = when_exhaustive(subject_type.as_ref(), cases, &base)?;
        if !exhaustive {
            if let Some(mut unmatched) = remaining {
                if !stable {
                    super::invalidate_narrowing(&mut unmatched);
                }
                if null_covered {
                    let original = subject_source.as_ref().and_then(|source| source.original(stable));
                    super::flow::narrow_when_remainder(binding.as_deref(), original, &mut unmatched);
                }
                paths.add(&base, unmatched);
            }
        } else if !cases.iter().any(|case| case.fallback) {
            // Typed coverage makes this path impossible. A terminator keeps the
            // lowered CFG sound for analyses that do not evaluate case predicates.
            context.emit(Op::Unreachable, line);
        }
        let end = context.instructions.len();
        for jump in ends {
            context.patch(jump, end)
        }
        context.is_reachable = incoming && !paths.is_empty();
        context.scopes = paths.finish(&base);
        context.exit_scope()?;
        context.emit(Op::Exit, line);
        Ok(())
    }
}
