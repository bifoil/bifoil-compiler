//! Resolve source paths to one lexical binding and typed selectors.
//! Static qualified names win over field access, matching ordinary lookup.

use super::{ConstantKey, ScopeStack, Step, ValuePath, lookup_ref};
use crate::syntax::Expr;

fn source(expression: &Expr, env: &ScopeStack) -> Option<ValuePath> {
    match expression {
        Expr::Group(value) | Expr::Unwrap(value) => source(value, env),
        Expr::Name(name) => Some(ValuePath {
            root: name.clone(),
            steps: Vec::new(),
            safe_prefixes: Vec::new(),
        }),
        Expr::Member { receiver, name, safe } => {
            let mut path = source(receiver, env)?;
            if *safe {
                path.safe_prefixes.push(path.steps.len());
            }
            path.steps.push(Step::Field(name.clone()));
            Some(path)
        }
        Expr::Index { receiver, index, safe } => {
            let selector = selector(index, env)?;
            let mut path = source(receiver, env)?;
            if *safe {
                path.safe_prefixes.push(path.steps.len());
            }
            path.steps.push(selector);
            Some(path)
        }
        _ => None,
    }
}

fn selector(expression: &Expr, env: &ScopeStack) -> Option<Step> {
    if let Some(key) = ConstantKey::source(expression, env, None) {
        return Some(Step::Index { key, mutable: false });
    }
    if let Some(mut name) = super::super::expression_path(expression) {
        canonicalize(&mut name, env);
        if let Some(binding) = lookup_ref(env, &name) {
            if binding.volatile {
                return None;
            }
            return Some(Step::BindingIndex {
                identity: binding.identity,
                name,
                mutable: binding.mutable,
                container_mutable: false,
            });
        }
        if matches!(ungroup(expression), Expr::Name(_)) {
            return Some(Step::EnumIndex(name));
        }
    }
    Some(Step::ComputedIndex {
        key: super::computed::Key::source(expression, env)?,
        container_mutable: false,
    })
}

fn ungroup(mut value: &Expr) -> &Expr {
    while let Expr::Group(inner) = value {
        value = inner;
    }
    value
}
fn canonicalize(name: &mut String, env: &ScopeStack) {
    if let Some(aliases) = lookup_ref(env, name).and_then(|binding| binding.global_aliases.as_ref()) {
        *name = aliases[0].clone();
    }
}

pub(super) fn resolve(expression: &Expr, env: &ScopeStack) -> Option<ValuePath> {
    let mut path = source(expression, env)?;
    let mut prefix = path.root.clone();
    let mut found = lookup_ref(env, &prefix).map(|_| (prefix.len(), 0));
    for (index, step) in path.steps.iter().enumerate() {
        let Step::Field(name) = step else { break };
        prefix.push('.');
        prefix.push_str(name);
        if lookup_ref(env, &prefix).is_some() {
            found = Some((prefix.len(), index + 1));
        }
    }
    let (length, consumed) = found?;
    path.root = prefix[..length].to_owned();
    path.steps.drain(..consumed);
    path.safe_prefixes = path
        .safe_prefixes
        .into_iter()
        .filter_map(|prefix| prefix.checked_sub(consumed))
        .collect();
    canonicalize(&mut path.root, env);
    Some(path)
}
