//! Substitute shared type arguments without expanding repeated subgraphs.
//! Unchanged branches keep their original argument allocations. Nullable
//! application still normalizes even when no parameter is replaced.

use crate::model::{Signature, Type};
use crate::types::graph::SharedNode;
use std::collections::BTreeMap;
use std::sync::Arc;

type Rewritten = (Type, bool);

/// Reuse an unchanged declaration header. Each specialization owns its rewritten
/// parameters, and all header types share one substitution query's memo table.
pub fn signature(declaration: &Signature, arguments: &BTreeMap<String, Type>) -> Signature {
    let mut specialized = declaration.clone();
    let mut checked = BTreeMap::new();
    for (index, parameter) in declaration.params.iter().enumerate() {
        let (ty, changed) = rewrite(&parameter.ty, arguments, &mut checked);
        if changed {
            Arc::make_mut(&mut specialized).params_mut()[index].ty = ty;
        }
    }
    let (result, changed) = rewrite(&declaration.result, arguments, &mut checked);
    if changed {
        Arc::make_mut(&mut specialized).result = result;
    }
    specialized
}

pub fn substitute(ty: &Type, arguments: &BTreeMap<String, Type>) -> Type {
    rewrite(ty, arguments, &mut BTreeMap::new()).0
}

fn memo_node(ty: &Type) -> Option<SharedNode<'_>> {
    let args = match ty {
        Type::Function { .. } | Type::Scene { .. } => return SharedNode::of(ty),
        Type::Named { args, .. } => args,
        _ => return None,
    };
    // Leaf-only arguments have no repeated descendants worth retaining.
    args.iter()
        .any(|ty| {
            matches!(
                ty,
                Type::Named { .. } | Type::Nullable { .. } | Type::Function { .. } | Type::Scene { .. }
            )
        })
        .then(|| SharedNode::of(ty))
        .flatten()
}

fn rewrite<'a>(
    ty: &'a Type,
    arguments: &BTreeMap<String, Type>,
    checked: &mut BTreeMap<SharedNode<'a>, Rewritten>,
) -> Rewritten {
    let node = memo_node(ty);
    if let Some(previous) = node.and_then(|node| checked.get(&node)) {
        return previous.clone();
    }
    let result = match ty {
        Type::Parameter { name, nonnull } => match arguments.get(name) {
            Some(actual) => (if *nonnull { actual.non_null() } else { actual.clone() }, true),
            None => (ty.clone(), false),
        },
        Type::Nullable { inner } => {
            let (inner, changed) = rewrite(inner, arguments, checked);
            if changed
                || matches!(
                    inner,
                    Type::Nullable { .. } | Type::Null | Type::Parameter { nonnull: true, .. }
                )
            {
                (inner.nullable(), true)
            } else {
                (ty.clone(), false)
            }
        }
        Type::Named { name, args } => {
            let (args, changed) = rewrite_all(args, arguments, checked);
            if changed {
                (Type::named(name, args), true)
            } else {
                (ty.clone(), false)
            }
        }
        Type::Function {
            params,
            result,
            suspend,
            vararg,
        } => {
            let (params, changed) = rewrite_all(params, arguments, checked);
            let (result, result_changed) = rewrite(result, arguments, checked);
            if changed || result_changed {
                (
                    Type::Function {
                        params: params.into(),
                        result: std::sync::Arc::new(result),
                        suspend: *suspend,
                        vararg: *vararg,
                    },
                    true,
                )
            } else {
                (ty.clone(), false)
            }
        }
        Type::Scene { params } => {
            let (params, changed) = rewrite_all(params, arguments, checked);
            if changed {
                (Type::Scene { params: params.into() }, true)
            } else {
                (ty.clone(), false)
            }
        }
        _ => (ty.clone(), false),
    };
    if let Some(node) = node {
        checked.insert(node, result.clone());
    }
    result
}
fn rewrite_all<'a>(
    types: &'a [Type],
    arguments: &BTreeMap<String, Type>,
    checked: &mut BTreeMap<SharedNode<'a>, Rewritten>,
) -> (Vec<Type>, bool) {
    let mut changed = false;
    let values = types
        .iter()
        .map(|ty| {
            let (value, rewritten) = rewrite(ty, arguments, checked);
            changed |= rewritten;
            value
        })
        .collect();
    (values, changed)
}
