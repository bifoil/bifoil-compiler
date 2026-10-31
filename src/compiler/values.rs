//! Result-producing blocks and anonymous function return conventions.

use super::result_types::{InferredValue, infer_value};
use super::{ScopeStack, infer_returns};
use crate::model::Type;
use crate::syntax::{Expr, FunctionAst, Statement, StatementKind};

pub(super) fn infer_block_value(body: &[Statement], env: &ScopeStack, expected: Option<&Type>) -> Result<Type, String> {
    Ok(infer_block_result(body, env, expected)?.ty)
}

pub(super) fn infer_block_result(
    body: &[Statement],
    env: &ScopeStack,
    expected: Option<&Type>,
) -> Result<InferredValue, String> {
    let (scope, reaches) = super::return_inference::value_prefix(body, env)?;
    if !reaches {
        return Ok(Type::Nothing.into());
    }
    let Some(last) = body.last() else {
        return Ok(Type::Unit.into());
    };
    match &last.kind {
        StatementKind::Expression(e) => infer_value(e, &scope, expected),
        StatementKind::Return(_) => Ok(Type::Nothing.into()),
        StatementKind::If { branches, no } => {
            if no.is_empty() {
                return Err("value-producing if requires else".into());
            }
            super::return_inference::if_value_result(branches, no, &scope, expected)
        }
        StatementKind::When {
            subject,
            binding,
            annotation,
            cases,
        } => super::return_inference::when_value_result(subject, binding, annotation, cases, &scope, expected),
        _ => Ok(Type::Unit.into()),
    }
}

pub(super) fn yieldize(body: &[Statement], expected: &Type) -> Result<Vec<Statement>, String> {
    let mut body = body.to_vec();
    if let Some(last) = body.last_mut() {
        last.kind = match &last.kind {
            StatementKind::Expression(value) => StatementKind::Yield {
                value: value.clone(),
                expected: expected.clone(),
            },
            StatementKind::If { branches, no } => {
                if no.is_empty() {
                    return Err("value-producing if requires else".into());
                }
                let mut branches = branches.clone();
                for branch in &mut branches {
                    branch.body = yieldize(&branch.body, expected)?;
                }
                StatementKind::If {
                    branches,
                    no: yieldize(no, expected)?,
                }
            }
            StatementKind::When {
                subject,
                binding,
                annotation,
                cases,
            } => {
                let mut cases = cases.clone();
                for case in &mut cases {
                    case.body = yieldize(&case.body, expected)?
                }
                StatementKind::When {
                    subject: subject.clone(),
                    binding: binding.clone(),
                    annotation: annotation.clone(),
                    cases,
                }
            }
            StatementKind::Return(_) => last.kind.clone(),
            _ => {
                let line = last.line;
                body.push(Statement {
                    line,
                    kind: StatementKind::Yield {
                        value: Expr::Name("Unit".into()),
                        expected: expected.clone(),
                    },
                });
                return Ok(body);
            }
        };
    }
    Ok(body)
}

pub(super) fn returnize(body: &[Statement], discard: bool) -> Vec<Statement> {
    let mut body = body.to_vec();
    if discard {
        body.push(Statement {
            line: body.last().map_or(1, |s| s.line),
            kind: StatementKind::Return(None),
        });
        return body;
    }
    if let Some(last) = body.last_mut() {
        match &mut last.kind {
            StatementKind::Expression(e) => last.kind = StatementKind::Return(Some(e.clone())),
            StatementKind::If { branches, no } if !no.is_empty() => {
                for branch in branches {
                    branch.body = returnize(&branch.body, false);
                }
                *no = returnize(no, false);
            }
            StatementKind::When { cases, .. } => {
                for case in cases {
                    case.body = returnize(&case.body, false)
                }
            }
            StatementKind::Return(_) => {}
            _ => {
                let line = last.line;
                body.push(Statement {
                    line,
                    kind: StatementKind::Return(None),
                })
            }
        }
    } else {
        body.push(Statement {
            line: 1,
            kind: StatementKind::Return(None),
        })
    }
    body
}

pub(super) fn lambda_definition(
    fun: &FunctionAst,
    env: &ScopeStack,
    expected: Option<&Type>,
) -> Result<FunctionAst, String> {
    let expected = expected.and_then(|t| {
        if let Type::Function { result, .. } = t.present() {
            Some(result.as_ref())
        } else {
            None
        }
    });
    let result = fun.result.as_ref().or(expected);
    let discard = result == Some(&Type::Unit);
    let mut function = fun.clone();
    function.body = returnize(&fun.body, discard);
    function.result = Some(if let Some(result) = result {
        result.clone()
    } else {
        infer_returns(&function, env)?
    });
    Ok(function)
}
