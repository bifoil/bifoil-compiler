//! Keep overloaded references as alternatives until the whole call is checked.

use super::super::super::{ScopeStack, infer, infer_as};
use super::super::{callbacks, reference_needs_resolution, visible_candidates};
use crate::generics::call_constraints::{Constraints, Failure};
use crate::model::{Signature, Type};
use crate::syntax::Expr;
use std::collections::BTreeMap;

const MAX_CONSTRAINT_BRANCHES: usize = 256;

pub(super) fn alternatives<'a>(
    signature: &'a Signature,
    supplied: &[callbacks::Deferred<'_>],
    expected: Option<&Type>,
    known: &BTreeMap<String, Type>,
) -> Result<Vec<Constraints<'a>>, String> {
    let mut initial = Constraints::new(&signature.generics);
    if let Some(expected) = expected {
        initial
            .context(&signature.result, expected)
            .map_err(|e| e.to_string())?;
    }
    let mut branches = vec![initial];
    let unresolved = signature
        .generics
        .iter()
        .filter(|parameter| !known.contains_key(&parameter.name))
        .map(|parameter| parameter.name.clone())
        .collect::<Vec<_>>();
    let query = ArgumentQuery { known, unresolved };
    for argument in supplied {
        branches = query.expand(argument.value, &argument.pattern, &argument.scope, branches, 0)?;
    }
    Ok(branches)
}

struct ArgumentQuery<'a> {
    known: &'a BTreeMap<String, Type>,
    unresolved: Vec<String>,
}
impl ArgumentQuery<'_> {
    fn expand<'a>(
        &self,
        expression: &Expr,
        pattern: &Type,
        env: &ScopeStack,
        mut branches: Vec<Constraints<'a>>,
        depth: usize,
    ) -> Result<Vec<Constraints<'a>>, String> {
        if depth > 32 {
            return Err(Failure::Budget.to_string());
        }
        let hint = crate::generics::substitute(pattern, self.known);
        let actual = if super::expression_options::requires_context(expression, env) {
            None
        } else {
            infer(expression, env).ok()
        };
        let actual = actual.or_else(|| {
            let incomplete = hint.any_node(|node| {
                matches!(node, Type::Star | Type::Pending)
                    || matches!(node, Type::Parameter { name, .. } if self.unresolved.contains(name))
            });
            (!incomplete)
                .then(|| infer_as(expression, env, Some(&hint)).ok())
                .flatten()
        });
        if let Some(actual) = actual {
            let constructible = if actual.collection().is_some() {
                matches!(super::super::ungroup(expression), Expr::Collection { .. })
            } else {
                matches!(
                    super::super::ungroup(expression),
                    Expr::Lambda(_) | Expr::Construct { .. } | Expr::Call(..)
                )
            };
            for branch in &mut branches {
                branch
                    .argument(pattern, &actual, constructible)
                    .map_err(|error| error.to_string())?;
            }
            return Ok(branches);
        }
        if let Expr::Group(inner) = expression {
            return self.expand(inner, pattern, env, branches, depth + 1);
        }
        if matches!(expression, Expr::Lambda(_) | Expr::Block(_))
            || matches!(expression, Expr::Collection { .. }) && hint.present().collection().is_none()
        {
            let options = super::expression_options::types(expression, env, &self.unresolved, Some(&hint), depth + 1)?;
            let mut alternatives = vec![];
            for actual in options {
                for previous in &branches {
                    if alternatives.len() == MAX_CONSTRAINT_BRANCHES {
                        return Err(Failure::Budget.to_string());
                    }
                    let mut next = previous.clone();
                    next.argument(pattern, &actual, matches!(expression, Expr::Lambda(_) | Expr::Block(_)))
                        .map_err(|error| error.to_string())?;
                    alternatives.push(next);
                }
            }
            if !alternatives.is_empty() {
                return Ok(alternatives);
            }
        }
        if let Expr::Collection { entries, .. } = expression
            && let Some((family, hinted, _)) = hint.present().collection()
        {
            let arguments = pattern
                .present()
                .collection()
                .map(|(_, args, _)| args)
                .unwrap_or(hinted);
            let mut scope = env.clone();
            for (key, value) in entries {
                if let Some(key) = key {
                    if family != "Map" {
                        return Ok(branches);
                    }
                    branches = self.expand(key, &arguments[0], &scope, branches, depth + 1)?;
                    super::super::super::return_inference::normal_completion(key, &mut scope)?;
                }
                branches = self.expand(value, arguments.last().unwrap(), &scope, branches, depth + 1)?;
                super::super::super::return_inference::normal_completion(value, &mut scope)?;
            }
            return Ok(branches);
        }
        if reference_needs_resolution(expression, env) {
            super::super::super::members::check_static_receiver(expression, env)?;
            let mut alternatives = vec![];
            for declaration in visible_candidates(expression, env)? {
                for actual in
                    callbacks::reference_types(&declaration, expression, hint.present(), &self.unresolved, env)
                {
                    for previous in &branches {
                        if alternatives.len() == MAX_CONSTRAINT_BRANCHES {
                            return Err(Failure::Budget.to_string());
                        }
                        let mut next = previous.clone();
                        next.argument(pattern, &actual, false)
                            .map_err(|error| error.to_string())?;
                        alternatives.push(next);
                    }
                }
            }
            if !alternatives.is_empty() {
                return Ok(alternatives);
            }
        }
        Ok(branches)
    }
}
