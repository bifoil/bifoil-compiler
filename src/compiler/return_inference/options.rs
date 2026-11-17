//! Alternative result types share the ordinary flow walker and result joins.

use super::super::result_types::{InferredValue, result_join};
use super::{Inference, ScopeStack, ValueResults, block, branches, function_scope};
use crate::model::Type;
use crate::syntax::{ConditionalBranch, Expr, FunctionAst, Statement, StatementKind, WhenCase};
use std::sync::Arc;

const MAX_RESULT_ALTERNATIVES: usize = 256;

pub(in crate::compiler) trait ResultOptions {
    fn expression(&mut self, expression: &Expr, env: &ScopeStack) -> Result<Vec<InferredValue>, String>;
    fn block_value(&mut self, body: &[Statement], env: &ScopeStack) -> Result<Vec<InferredValue>, String>;
}

#[derive(Default)]
pub(super) struct Choices {
    values: Vec<InferredValue>,
    recorded: bool,
}
impl Choices {
    pub(super) fn add(&mut self, options: Vec<InferredValue>) -> Result<(), String> {
        let mut next = vec![];
        if !self.recorded {
            for value in options {
                retain(&mut next, value)?;
            }
        } else {
            for prefix in &self.values {
                for option in &options {
                    if let Ok(joined) = result_join(vec![prefix.clone(), option.clone()], None) {
                        retain(&mut next, joined)?;
                    }
                }
            }
        }
        self.values = next;
        self.recorded = true;
        Ok(())
    }
    pub(super) fn finish(self, otherwise: Type) -> Vec<InferredValue> {
        if self.recorded {
            self.values
        } else {
            vec![otherwise.into()]
        }
    }
}

fn retain(values: &mut Vec<InferredValue>, value: InferredValue) -> Result<(), String> {
    if values.contains(&value) {
        return Ok(());
    }
    if !value.ty.within_depth(32) || values.len() == MAX_RESULT_ALTERNATIVES {
        return Err("generic result inference type graph budget exceeded; supply type arguments explicitly".into());
    }
    values.push(value);
    Ok(())
}

pub(in crate::compiler) fn function(
    fun: &FunctionAst,
    env: &ScopeStack,
    options: &mut dyn ResultOptions,
) -> Result<Vec<InferredValue>, String> {
    let mut scope = function_scope(fun, super::super::functions::FunctionEnvironment::new(env))?;
    let mut analysis = Inference {
        alternatives: Some(options),
        ..Default::default()
    };
    block(&fun.body, &mut scope, &mut analysis)?;
    Ok(analysis.choices.finish(Type::Unit))
}

pub(in crate::compiler) fn value(
    body: &[Statement],
    env: &ScopeStack,
    options: &mut dyn ResultOptions,
) -> Result<Vec<InferredValue>, String> {
    let mut scope = env.clone();
    scope.push(Arc::default());
    super::prepare_functions(body, &mut scope)?;
    let mut analysis = Inference {
        alternatives: Some(options),
        external_loop_transfers: true,
        flow_only: true,
        ..Default::default()
    };
    analysis.flow_only = true;
    if !block(&body[..body.len().saturating_sub(1)], &mut scope, &mut analysis)? {
        return Ok(vec![Type::Nothing.into()]);
    }
    let Some(last) = body.last() else {
        return Ok(vec![Type::Unit.into()]);
    };
    match &last.kind {
        StatementKind::Expression(expression) => analysis.alternatives.as_mut().unwrap().expression(expression, &scope),
        StatementKind::Return(_) | StatementKind::ImplicitReturn => Ok(vec![Type::Nothing.into()]),
        StatementKind::If { branches, no } => {
            if no.is_empty() {
                return Err("value-producing if requires else".into());
            }
            conditional(branches, no, &scope, analysis.alternatives.take().unwrap())
        }
        StatementKind::When {
            subject,
            binding,
            annotation,
            cases,
        } => when(
            subject,
            binding,
            annotation,
            cases,
            &scope,
            analysis.alternatives.take().unwrap(),
        ),
        _ => Ok(vec![Type::Unit.into()]),
    }
}

fn conditional(
    arms: &[ConditionalBranch],
    no: &[Statement],
    env: &ScopeStack,
    options: &mut dyn ResultOptions,
) -> Result<Vec<InferredValue>, String> {
    let mut scope = env.clone();
    let mut analysis = Inference {
        alternatives: Some(options),
        external_loop_transfers: true,
        flow_only: true,
        ..Default::default()
    };
    let mut values = ValueResults {
        expected: None,
        types: vec![],
        choices: Choices::default(),
    };
    branches::conditional(arms, no, &mut scope, &mut analysis, Some(&mut values))?;
    Ok(values.choices.finish(Type::Nothing))
}

fn when(
    subject: &Option<Expr>,
    binding: &Option<String>,
    annotation: &Option<Type>,
    cases: &[WhenCase],
    env: &ScopeStack,
    options: &mut dyn ResultOptions,
) -> Result<Vec<InferredValue>, String> {
    let mut analysis = Inference {
        alternatives: Some(options),
        external_loop_transfers: true,
        flow_only: true,
        ..Default::default()
    };
    let mut values = ValueResults {
        expected: None,
        types: vec![],
        choices: Choices::default(),
    };
    super::when_values(subject, binding, annotation, cases, env, &mut analysis, &mut values)?;
    Ok(values.choices.finish(Type::Nothing))
}

#[cfg(test)]
mod tests;
