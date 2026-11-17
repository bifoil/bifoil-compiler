//! Contextual candidates for anonymous functions and result-producing blocks.
//! Flow, parameter scopes and return joins remain owned by return inference.

use super::super::super::ScopeStack;
use super::super::super::result_types::{InferredValue, infer_value};
use super::super::super::return_inference::options::{self, ResultOptions};
use super::super::{callbacks, reference_needs_resolution, ungroup, visible_candidates};
use crate::generics::call_constraints::Failure;
use crate::model::Type;
use crate::syntax::{Expr, FunctionAst, Statement};

pub(super) fn types(
    expression: &Expr,
    env: &ScopeStack,
    unresolved: &[String],
    hint: Option<&Type>,
    depth: usize,
) -> Result<Vec<Type>, String> {
    let mut query = Query {
        unresolved,
        hint: hint.cloned(),
        depth,
    };
    let mut types = vec![];
    for value in query.expression(expression, env)? {
        if !types.contains(&value.ty) {
            types.push(value.ty);
        }
    }
    Ok(types)
}

pub(super) fn complete(ty: &Type, unresolved: &[String]) -> bool {
    !ty.any_node(|node| {
        matches!(node, Type::Pending | Type::Star)
            || matches!(node, Type::Parameter { name, .. } if unresolved.contains(name))
    })
}

pub(super) fn requires_context(expression: &Expr, env: &ScopeStack) -> bool {
    match ungroup(expression) {
        Expr::Lambda(_) | Expr::Block(_) => true,
        Expr::Collection { entries, .. } => entries.iter().any(|(key, value)| {
            key.as_ref().is_some_and(|key| requires_context(key, env)) || requires_context(value, env)
        }),
        _ => callbacks::requires_context(expression, env),
    }
}

struct Query<'a> {
    unresolved: &'a [String],
    hint: Option<Type>,
    depth: usize,
}
impl Query<'_> {
    fn lambda(&mut self, fun: &FunctionAst, env: &ScopeStack) -> Result<Vec<InferredValue>, String> {
        let expected = self.hint.as_ref().filter(|ty| complete(ty, self.unresolved));
        if (expected.is_some() || fun.result.is_some())
            && let Ok(value) = infer_value(&Expr::Lambda(Box::new(fun.clone())), env, expected)
        {
            return Ok(vec![value]);
        }
        // A source annotation fixes the result; alternatives cannot replace it.
        if fun.result.is_some() {
            return Ok(vec![]);
        }
        let return_hint = match self.hint.as_ref().map(Type::present) {
            Some(Type::Function { result, .. }) => Some(result.as_ref().clone()),
            _ => None,
        };
        let discard = return_hint == Some(Type::Unit);
        let mut function = fun.clone();
        function.body = super::super::super::values::returnize(&fun.body, discard);
        let mut returned = Query {
            hint: return_hint,
            unresolved: self.unresolved,
            depth: self.depth + 1,
        };
        let mut signatures = vec![];
        for value in options::function(&function, env, &mut returned)? {
            function.result = Some(value.ty);
            let ty = super::super::super::functions::signature(&function, "$candidate".into()).ty();
            if !ty.within_depth(32) {
                return Err(Failure::Budget.to_string());
            }
            let inferred = InferredValue::from(ty);
            if !signatures.contains(&inferred) {
                signatures.push(inferred);
            }
        }
        Ok(signatures)
    }
}

impl ResultOptions for Query<'_> {
    fn expression(&mut self, expression: &Expr, env: &ScopeStack) -> Result<Vec<InferredValue>, String> {
        if self.depth > 32 {
            return Err(Failure::Budget.to_string());
        }
        let expression = ungroup(expression);
        match expression {
            Expr::Lambda(fun) => return self.lambda(fun, env),
            Expr::Block(body) => return self.block_value(body, env),
            Expr::Collection { .. } => {
                return super::literal_types::options(expression, env, self.unresolved, self.hint.as_ref(), self.depth)
                    .map(|types| types.into_iter().map(InferredValue::from).collect());
            }
            _ => {}
        }
        let expected = self.hint.as_ref().filter(|ty| complete(ty, self.unresolved));
        if !callbacks::requires_context(expression, env)
            && let Ok(value) = infer_value(expression, env, expected)
        {
            return Ok(vec![value]);
        }
        if reference_needs_resolution(expression, env) {
            super::super::super::members::check_static_receiver(expression, env)?;
            let mut values = vec![];
            let hint = self.hint.as_ref().map(Type::present).unwrap_or(&Type::Pending);
            for declaration in visible_candidates(expression, env)? {
                for ty in callbacks::reference_types(&declaration, expression, hint, self.unresolved, env) {
                    let value = InferredValue::from(ty);
                    if !values.contains(&value) {
                        if values.len() == 256 {
                            return Err(Failure::Budget.to_string());
                        }
                        values.push(value);
                    }
                }
            }
            return Ok(values);
        }
        Ok(vec![])
    }

    fn block_value(&mut self, body: &[Statement], env: &ScopeStack) -> Result<Vec<InferredValue>, String> {
        if self.depth > 32 {
            return Err(Failure::Budget.to_string());
        }
        let mut nested = Query {
            unresolved: self.unresolved,
            hint: self.hint.clone(),
            depth: self.depth + 1,
        };
        options::value(body, env, &mut nested)
    }
}
