//! Conservative overlap proofs for checked casts, without value conversions.
//!
//! A cast is rejected only when these static facts establish disjoint types.
//! Unknown generic arguments and inhabited common subtypes require runtime checks.

use crate::model::Type;
use crate::types::graph::RetainedNode;
use std::collections::BTreeMap;

const MAX_PROOF_DEPTH: usize = 64;
const MAX_QUERY_RELATIONSHIPS: usize = 4096;

impl Type {
    pub(crate) fn may_overlap(&self, target: &Self) -> bool {
        OverlapQuery::default().check(Proof::ValueOverlap, self, target, 0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Proof {
    ValueOverlap,
    ArgumentIdentity,
}

#[derive(Default)]
struct OverlapQuery {
    checked: BTreeMap<(Proof, usize, RetainedNode, RetainedNode), bool>,
}
impl OverlapQuery {
    fn check(&mut self, proof: Proof, left: &Type, right: &Type, depth: usize) -> bool {
        // Argument identity is stricter than value overlap. Depth also matters:
        // a conservative answer near the limit cannot replace a shallower proof.
        let key = RetainedNode::of(left)
            .zip(RetainedNode::of(right))
            .map(|(left, right)| (proof, depth, left, right));
        if let Some(result) = key.as_ref().and_then(|key| self.checked.get(key)) {
            return *result;
        }
        let result = match proof {
            Proof::ValueOverlap => overlap(self, left, right, depth),
            Proof::ArgumentIdentity => identical_argument(self, left, right, depth),
        };
        if let Some(key) = key
            && self.checked.len() < MAX_QUERY_RELATIONSHIPS
        {
            self.checked.insert(key, result);
        }
        result
    }
}

fn overlap(query: &mut OverlapQuery, left: &Type, right: &Type, depth: usize) -> bool {
    if depth >= MAX_PROOF_DEPTH || matches!(left, Type::Nothing | Type::Pending) || *right == Type::Pending {
        // A bottom-valued operand never reaches the cast. An exhausted proof
        // budget cannot establish that a reachable runtime check must fail.
        return true;
    }
    if left.permits_null() && right.permits_null() {
        return true;
    }
    if left == right || left.subtype_of(right) {
        return true;
    }
    if *right == Type::Nothing {
        return false;
    }
    if right.subtype_of(left) {
        return true;
    }
    if *left == Type::Null || *right == Type::Null {
        return false;
    }
    let left = left.present();
    let right = right.present();
    if matches!(left, Type::Any | Type::Star) || matches!(right, Type::Any | Type::Star) {
        return true;
    }
    // Any shared null path has already been handled. Member views describe
    // the remaining non-null portion even when a parameter's bound is nullable.
    if matches!(left, Type::Parameter { .. }) {
        let view = crate::generics::member_view(left);
        return view == *left || query.check(Proof::ValueOverlap, &view, right, depth + 1);
    }
    if matches!(right, Type::Parameter { .. }) {
        let view = crate::generics::member_view(right);
        return view == *right || query.check(Proof::ValueOverlap, left, &view, depth + 1);
    }
    match (left, right) {
        (Type::Named { name: a, args: aa }, Type::Named { name: b, args: ba }) => {
            if a == b {
                return aa.len() == ba.len()
                    && aa
                        .iter()
                        .zip(ba.iter())
                        .enumerate()
                        .all(|(index, (left_arg, right_arg))| {
                            // Covariant arguments may share a bottom subtype;
                            // contravariant arguments may share a wider input type.
                            // Bounds/inhabitation proofs remain conservative here.
                            crate::declarations::argument_variance(a, index) != 0
                                || query.check(Proof::ArgumentIdentity, left_arg, right_arg, depth + 1)
                        });
            }
            // Closed variants can overlap their own enclosing sum. Sharing a
            // parent does not make two distinct variant declarations overlap.
            if let Some(parent) = crate::declarations::parent(left)
                && nominal_name(&parent) == Some(b.as_str())
            {
                return query.check(Proof::ValueOverlap, &parent, right, depth + 1);
            }
            if let Some(parent) = crate::declarations::parent(right)
                && nominal_name(&parent) == Some(a.as_str())
            {
                return query.check(Proof::ValueOverlap, left, &parent, depth + 1);
            }
            false
        }
        (
            Type::Function {
                params: a, vararg: av, ..
            },
            Type::Function {
                params: b, vararg: bv, ..
            },
        ) => {
            // A broad-input, bottom-result function may satisfy both signatures.
            // Suspension widening permits a common ordinary function as well.
            a.len() == b.len() && av == bv
        }
        (Type::Scene { params: a }, Type::Scene { params: b }) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(a, b)| query.check(Proof::ArgumentIdentity, a, b, depth + 1))
        }
        _ => false,
    }
}

fn nominal_name(ty: &Type) -> Option<&str> {
    if let Type::Named { name, .. } = ty {
        Some(name)
    } else {
        None
    }
}

/// An invariant slot needs a possible identical reified argument, not merely
/// two value types with a common subtype. Concrete List<Int> and List<String>
/// are distinct arguments even though their covariant values can overlap.
fn identical_argument(query: &mut OverlapQuery, left: &Type, right: &Type, depth: usize) -> bool {
    if depth >= MAX_PROOF_DEPTH || left == right || *left == Type::Star || *right == Type::Star {
        return true;
    }
    if matches!(left, Type::Parameter { .. }) || matches!(right, Type::Parameter { .. }) {
        return query.check(Proof::ValueOverlap, left, right, depth + 1);
    }
    match (left, right) {
        (Type::Nullable { inner: a }, Type::Nullable { inner: b }) => {
            query.check(Proof::ArgumentIdentity, a, b, depth + 1)
        }
        (Type::Named { name: a, args: aa }, Type::Named { name: b, args: ba }) => {
            a == b
                && aa.len() == ba.len()
                && aa
                    .iter()
                    .zip(ba.iter())
                    .all(|(a, b)| query.check(Proof::ArgumentIdentity, a, b, depth + 1))
        }
        (
            Type::Function {
                params: a,
                result: ar,
                suspend: as_,
                vararg: av,
            },
            Type::Function {
                params: b,
                result: br,
                suspend: bs,
                vararg: bv,
            },
        ) => {
            a.len() == b.len()
                && as_ == bs
                && av == bv
                && query.check(Proof::ArgumentIdentity, ar, br, depth + 1)
                && a.iter()
                    .zip(b.iter())
                    .all(|(a, b)| query.check(Proof::ArgumentIdentity, a, b, depth + 1))
        }
        (Type::Scene { params: a }, Type::Scene { params: b }) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(a, b)| query.check(Proof::ArgumentIdentity, a, b, depth + 1))
        }
        _ => false,
    }
}
