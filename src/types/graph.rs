//! Query-local identities for immutable shared type children.
//! Addresses are compared only while their allocations are borrowed or retained.
//! They are never dereferenced, serialized, or used as language-level type identities.

use crate::model::Type;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SharedNode<'a> {
    Named {
        name: &'a str,
        arguments: *const Type,
        length: usize,
    },
    Scene {
        parameters: *const Type,
        length: usize,
    },
    Function {
        parameters: *const Type,
        length: usize,
        result: *const Type,
        suspend: bool,
        vararg: bool,
    },
}
impl<'a> SharedNode<'a> {
    pub(crate) fn of(ty: &'a Type) -> Option<Self> {
        match ty {
            Type::Named { name, args } if !args.is_empty() => Some(Self::Named {
                name,
                arguments: args.as_ptr(),
                length: args.len(),
            }),
            Type::Scene { params } if !params.is_empty() => Some(Self::Scene {
                parameters: params.as_ptr(),
                length: params.len(),
            }),
            Type::Function {
                params,
                result,
                suspend,
                vararg,
            } => Some(Self::Function {
                parameters: params.as_ptr(),
                length: params.len(),
                result: Arc::as_ptr(result),
                suspend: *suspend,
                vararg: *vararg,
            }),
            _ => None,
        }
    }
}

impl Type {
    /// Pure shape predicates visit each distinct shared child graph once.
    pub(crate) fn any_node(&self, mut matches: impl FnMut(&Type) -> bool) -> bool {
        let mut pending = vec![self];
        let mut checked = BTreeSet::new();
        while let Some(ty) = pending.pop() {
            if matches(ty) {
                return true;
            }
            if let Some(node) = SharedNode::of(ty)
                && !checked.insert(node)
            {
                continue;
            }
            match ty {
                Type::Named { args, .. } => pending.extend(args.iter()),
                Type::Scene { params } => pending.extend(params.iter()),
                Type::Nullable { inner } => pending.push(inner),
                Type::Function { params, result, .. } => {
                    pending.push(result);
                    pending.extend(params.iter());
                }
                _ => {}
            }
        }
        false
    }

    /// Inspect nodes without expanding a shared child graph more than once.
    pub(crate) fn for_each_node(&self, mut visit: impl FnMut(&Type)) {
        self.any_node(|ty| {
            visit(ty);
            false
        });
    }

    pub fn within_depth(&self, maximum: usize) -> bool {
        let mut pending = vec![(self, 0)];
        let mut checked = BTreeMap::new();
        while let Some((ty, depth)) = pending.pop() {
            if depth > maximum {
                return false;
            }
            if let Some(node) = SharedNode::of(ty) {
                // A node first reached by a short path must be checked again
                // when a longer path could put its children over the limit.
                if checked.get(&node).is_some_and(|earlier| *earlier >= depth) {
                    continue;
                }
                checked.insert(node, depth);
            }
            match ty {
                Type::Scene { params } => pending.extend(params.iter().map(|ty| (ty, depth + 1))),
                Type::Named { args, .. } => pending.extend(args.iter().map(|ty| (ty, depth + 1))),
                Type::Nullable { inner } => pending.push((inner, depth + 1)),
                Type::Function { params, result, .. } => {
                    pending.push((result, depth + 1));
                    pending.extend(params.iter().map(|ty| (ty, depth + 1)));
                }
                _ => {}
            }
        }
        true
    }
}

/// Preserve structural equality without unfolding immutable shared arguments.
impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        if let Some(equal) = shallow_equal(self, other) {
            return equal;
        }
        let mut pending = vec![(self, other)];
        let mut checked = BTreeSet::new();
        while let Some((left, right)) = pending.pop() {
            if let Some(equal) = shallow_equal(left, right) {
                if !equal {
                    return false;
                }
                continue;
            }
            if let (Some(left), Some(right)) = (SharedNode::of(left), SharedNode::of(right))
                && !checked.insert((left, right))
            {
                continue;
            }
            match (left, right) {
                (Type::Named { args: a, .. }, Type::Named { args: b, .. }) => {
                    pending.extend(a.iter().zip(b.iter()));
                }
                (Type::Scene { params: a }, Type::Scene { params: b }) => pending.extend(a.iter().zip(b.iter())),
                (Type::Nullable { inner: a }, Type::Nullable { inner: b }) => pending.push((a, b)),
                (
                    Type::Function {
                        params: a, result: ar, ..
                    },
                    Type::Function {
                        params: b, result: br, ..
                    },
                ) => {
                    pending.push((ar, br));
                    pending.extend(a.iter().zip(b.iter()));
                }
                _ => unreachable!("shallow equality handles scalar and incompatible shapes"),
            }
        }
        true
    }
}
impl Eq for Type {}

/// None means that compatible shapes still require child comparisons.
fn shallow_equal(left: &Type, right: &Type) -> Option<bool> {
    if std::ptr::eq(left, right) {
        return Some(true);
    }
    if std::mem::discriminant(left) != std::mem::discriminant(right) {
        return Some(false);
    }
    match left {
        Type::Int
        | Type::Float
        | Type::Boolean
        | Type::String
        | Type::Unit
        | Type::Char
        | Type::Null
        | Type::Nothing
        | Type::Any
        | Type::Star
        | Type::Pending => Some(true),
        Type::Parameter { name: a, nonnull: an } => {
            let Type::Parameter { name: b, nonnull: bn } = right else {
                unreachable!()
            };
            Some(a == b && an == bn)
        }
        Type::Named { name: a, args: aa } => {
            let Type::Named { name: b, args: ba } = right else {
                unreachable!()
            };
            if a != b || aa.len() != ba.len() {
                Some(false)
            } else if aa.is_empty() || Arc::ptr_eq(aa, ba) {
                Some(true)
            } else {
                None
            }
        }
        Type::Nullable { .. } => None,
        Type::Scene { params: a } => {
            let Type::Scene { params: b } = right else {
                unreachable!()
            };
            if a.len() != b.len() {
                Some(false)
            } else if Arc::ptr_eq(a, b) {
                Some(true)
            } else {
                None
            }
        }
        Type::Function {
            params: a,
            result: ar,
            suspend: as_,
            vararg: av,
            ..
        } => {
            let Type::Function {
                params: b,
                result: br,
                suspend: bs,
                vararg: bv,
                ..
            } = right
            else {
                unreachable!()
            };
            if a.len() != b.len() || as_ != bs || av != bv {
                Some(false)
            } else if Arc::ptr_eq(a, b) && Arc::ptr_eq(ar, br) {
                Some(true)
            } else {
                None
            }
        }
    }
}

/// Retain shared allocations while a relationship query caches their identities.
/// Derived bounds and parents may otherwise be dropped before a query finishes.
#[derive(Clone)]
pub(crate) struct RetainedNode(Type);
impl RetainedNode {
    pub(crate) fn of(ty: &Type) -> Option<Self> {
        SharedNode::of(ty).map(|_| Self(ty.clone()))
    }
    fn identity(&self) -> SharedNode<'_> {
        SharedNode::of(&self.0).expect("retained nodes have shared children")
    }
}
impl PartialEq for RetainedNode {
    fn eq(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }
}
impl Eq for RetainedNode {}
impl PartialOrd for RetainedNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for RetainedNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.identity().cmp(&other.identity())
    }
}
