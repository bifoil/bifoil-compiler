//! Branch-result inference uses the same scope joins and narrowing as lowering.
//! Failed tests and guards retain their effects before later alternatives run.

use super::super::constants::{constant_boolean, constant_comparison};
use super::super::flow::{FlowJoin, narrow_stable_condition};
use super::super::{
    Binding, ScopeStack, case_bindings, infer_as, insert, invalidate_narrowing, merge_env, narrow_case,
    narrow_condition, pure_expression, when_exhaustive,
};
use super::{Inference, ValueResults, block, expression_returns};
use crate::model::Type;
use crate::syntax::{ConditionalBranch, Expr, OptionAst, Pattern, Statement, WhenCase};
use std::sync::Arc;

/// Both result inference and value blocks observe condition effects before
/// deriving branch facts. Only normally completing paths reach later statements.
pub(super) fn conditional(
    branches: &[ConditionalBranch],
    no: &[Statement],
    env: &mut ScopeStack,
    analysis: &mut Inference,
    mut values: Option<&mut ValueResults<'_>>,
) -> Result<bool, String> {
    let mut remaining = env.clone();
    let mut base = None;
    let mut exits = FlowJoin::default();
    let mut reaches = true;
    for branch in branches {
        let condition = &branch.condition;
        if !expression_returns(condition, &mut remaining, analysis)? {
            if let Some(values) = values.as_deref_mut() {
                values.types.push(Type::Nothing.into());
            }
            reaches = false;
            break;
        }
        base.get_or_insert_with(|| remaining.clone());
        let known = constant_boolean(condition);
        // Value arms must agree even for constant conditions. Statement-mode
        // inference only follows normally reachable tests and bodies.
        if values.is_some() || known != Some(false) {
            let mut scope = remaining.clone();
            narrow_condition(condition, &mut scope, true);
            if let Some(values) = values.as_deref_mut() {
                values.types.push(super::super::values::infer_block_result(
                    &branch.body,
                    &scope,
                    values.expected,
                )?);
            } else {
                scope.push(Arc::default());
                let body_reaches = block(&branch.body, &mut scope, analysis)?;
                scope.pop();
                if body_reaches {
                    exits.add(base.as_ref().unwrap(), scope);
                }
            }
        }
        if values.is_none() && known == Some(true) {
            reaches = false;
            break;
        }
        narrow_condition(condition, &mut remaining, false);
    }
    if reaches {
        if let Some(values) = values {
            values.types.push(super::super::values::infer_block_result(
                no,
                &remaining,
                values.expected,
            )?);
        } else {
            remaining.push(Arc::default());
            let body_reaches = block(no, &mut remaining, analysis)?;
            remaining.pop();
            if body_reaches {
                exits.add(base.as_ref().unwrap_or(env), remaining);
            }
        }
    }
    let reaches = !exits.is_empty();
    *env = exits.finish(base.as_ref().unwrap_or(env));
    Ok(reaches)
}

pub(super) fn when_statement(
    subject: &Option<Expr>,
    binding: &Option<String>,
    annotation: &Option<Type>,
    cases: &[WhenCase],
    env: &mut ScopeStack,
    analysis: &mut Inference,
    mut values: Option<&mut ValueResults<'_>>,
) -> Result<bool, String> {
    let mut subject_source = None;
    let subject_type = if let Some(subject) = subject {
        let actual = infer_as(subject, env, annotation.as_ref())?;
        if !expression_returns(subject, env, analysis)? {
            return Ok(false);
        }
        let target = annotation.clone().unwrap_or(actual.clone());
        subject_source = super::super::flow::SubjectSource::capture(subject, &actual, &target, env);
        Some(target)
    } else {
        None
    };
    let outer = env.len();
    env.push(Arc::default());
    if let (Some(name), Some(ty)) = (binding, &subject_type) {
        bind(env, name, ty.clone())?;
    }
    let base = env.clone();
    let mut remaining = Some(base.clone());
    let mut paths = FlowJoin::default();
    let mut stable = true;
    let mut null_covered = false;
    for case in cases {
        let mut scope = remaining.clone().unwrap_or_else(|| base.clone());
        if !stable {
            invalidate_narrowing(&mut scope);
        }
        let mut reaches = remaining.is_some();
        let mut matches = Vec::new();
        let bindings = case_bindings(case, subject_type.as_ref())?;
        if !case.fallback {
            for pattern in &case.patterns {
                if let Pattern::Value(value) | Pattern::Membership { value, .. } = pattern {
                    if reaches {
                        reaches = expression_returns(value, &mut scope, analysis)?;
                    }
                    stable &= pure_expression(value);
                }
                let known = match pattern {
                    Pattern::Value(value) => {
                        if let Some(subject) = subject {
                            constant_comparison("==", subject, value)
                        } else {
                            constant_boolean(value)
                        }
                    }
                    _ => None,
                };
                if reaches && known != Some(false) {
                    let mut success = scope.clone();
                    if let Pattern::Value(value) = pattern
                        && subject.is_none()
                    {
                        narrow_condition(value, &mut success, true);
                    }
                    matches.push(success);
                }
                reaches &= known != Some(true);
                if let Pattern::Value(value) = pattern
                    && subject.is_none()
                {
                    narrow_condition(value, &mut scope, false);
                }
            }
        } else if reaches {
            matches.push(scope.clone());
        }
        let mut unmatched = Vec::new();
        if !case.fallback && reaches {
            unmatched.push(scope);
        }
        if !matches.is_empty() {
            let mut branch = merge_env(&base, &matches);
            branch.push(Arc::default());
            for (name, ty) in bindings {
                bind(&mut branch, &name, ty)?;
            }
            let original = subject_source.as_ref().and_then(|source| source.original(stable));
            narrow_case(case, subject_type.as_ref(), binding.as_deref(), original, &mut branch);
            if case.fallback && null_covered {
                super::super::flow::narrow_when_remainder(binding.as_deref(), original, &mut branch);
            }
            let mut body_reaches = true;
            if let Some(guard) = &case.guard {
                body_reaches = expression_returns(guard, &mut branch, analysis)?;
                stable &= pure_expression(guard);
                let known = constant_boolean(guard);
                if body_reaches && known != Some(true) {
                    let mut failed = branch.clone();
                    failed.truncate(base.len());
                    narrow_condition(guard, &mut failed, false);
                    unmatched.push(failed);
                }
                body_reaches &= known != Some(false);
                narrow_condition(guard, &mut branch, true);
            }
            if body_reaches {
                if let Some(values) = values.as_deref_mut() {
                    values.types.push(super::super::values::infer_block_result(
                        &case.body,
                        &branch,
                        values.expected,
                    )?);
                } else if block(&case.body, &mut branch, analysis)? {
                    branch.pop();
                    paths.add(&base, branch);
                }
            }
        }
        remaining = (!unmatched.is_empty()).then(|| merge_env(&base, &unmatched));
        if case
            .guard
            .as_ref()
            .is_none_or(|guard| constant_boolean(guard) == Some(true))
            && case
                .patterns
                .iter()
                .any(|pattern| matches!(pattern, Pattern::Value(Expr::Null)))
        {
            null_covered = true;
        }
    }
    if !when_exhaustive(subject_type.as_ref(), cases, &base)?
        && let Some(mut unmatched) = remaining
    {
        if !stable {
            invalidate_narrowing(&mut unmatched);
        }
        if null_covered {
            let original = subject_source.as_ref().and_then(|source| source.original(stable));
            super::super::flow::narrow_when_remainder(binding.as_deref(), original, &mut unmatched);
        }
        paths.add(&base, unmatched);
    }
    let reaches = !paths.is_empty();
    *env = paths.finish(&base);
    env.truncate(outer);
    Ok(reaches)
}

pub(super) fn choice(
    options: &[OptionAst],
    fallback: &Option<Vec<Statement>>,
    env: &mut ScopeStack,
    analysis: &mut Inference,
) -> Result<bool, String> {
    for option in options {
        let before = env.clone();
        let mut prepared = before.clone();
        let mut exits = Vec::new();
        if option.once {
            exits.push(before.clone());
        }
        if expression_returns(&option.visible, &mut prepared, analysis)? {
            let visibility = constant_boolean(&option.visible);
            if visibility != Some(true) {
                exits.push(prepared.clone());
            }
            if visibility != Some(false) {
                narrow_condition(&option.visible, &mut prepared, true);
                if expression_returns(&option.enabled, &mut prepared, analysis)?
                    && expression_returns(&option.label, &mut prepared, analysis)?
                {
                    exits.push(prepared);
                }
            }
        }
        *env = merge_env(&before, &exits);
        if exits.is_empty() {
            return Ok(false);
        }
    }
    let base = env.clone();
    let mut paths = FlowJoin::default();
    for option in options {
        if constant_boolean(&option.visible) == Some(false) || constant_boolean(&option.enabled) == Some(false) {
            continue;
        }
        let mut branch = base.clone();
        narrow_stable_condition(&option.visible, &mut branch);
        narrow_stable_condition(&option.enabled, &mut branch);
        branch.push(Arc::default());
        if block(&option.body, &mut branch, analysis)? {
            branch.pop();
            paths.add(&base, branch);
        }
    }
    let always_selectable = options.iter().any(|option| {
        !option.once
            && constant_boolean(&option.visible) == Some(true)
            && constant_boolean(&option.enabled) == Some(true)
    });
    if let Some(fallback) = fallback
        && !always_selectable
    {
        let mut branch = base.clone();
        branch.push(Arc::default());
        if block(fallback, &mut branch, analysis)? {
            branch.pop();
            paths.add(&base, branch);
        }
    }
    let reaches = !paths.is_empty();
    *env = paths.finish(&base);
    Ok(reaches)
}

fn bind(env: &mut ScopeStack, name: &str, ty: Type) -> Result<(), String> {
    insert(
        env,
        name,
        Binding {
            identity: crate::compiler::bindings::Identity::new(),
            index_key: false,
            global_aliases: None,
            volatile: false,
            implicit_receiver: false,
            escaped: false,
            runtime: name.into(),
            ty,
            mutable: false,
            initialized: true,
            may_be_initialized: true,
            owner: String::new(),
            narrow: None,
            field_narrows: None,
            callables: vec![],
            requires: Default::default(),
            callable_requirements: Default::default(),
        },
    )
}
