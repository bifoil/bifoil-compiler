//! Subtyping reuses completed named-node pairs within one relationship query.
//! Retained argument allocations keep query identities valid across temporary
//! bound refinements and substituted nominal parents.

use crate::model::Type;
use crate::types::graph::RetainedNode;
use std::collections::BTreeMap;

const MAX_QUERY_RELATIONSHIPS: usize = 4096;

impl Type {
    pub fn subtype_of(&self, target: &Self) -> bool {
        SubtypeQuery::default().check(self, target)
    }
}

#[derive(Default)]
struct SubtypeQuery {
    checked: BTreeMap<(RetainedNode, RetainedNode), bool>,
}
impl SubtypeQuery {
    fn check(&mut self, source: &Type, target: &Type) -> bool {
        if source == target || *source == Type::Nothing {
            return true;
        }
        let key = RetainedNode::of(source).zip(RetainedNode::of(target));
        if let Some(result) = key.as_ref().and_then(|key| self.checked.get(key)) {
            return *result;
        }
        let result = self.relationship(source, target);
        if let Some(key) = key
            && self.checked.len() < MAX_QUERY_RELATIONSHIPS
        {
            self.checked.insert(key, result);
        }
        result
    }

    fn relationship(&mut self, source: &Type, target: &Type) -> bool {
        if let (Type::Parameter { name: a, nonnull: an }, Type::Parameter { name: b, nonnull: bn }) = (source, target)
            && a == b
        {
            return !bn || *an || crate::generics::nonnull(a);
        }
        if *target == Type::Any {
            return !source.permits_null();
        }
        if let Type::Nullable { inner } = target {
            if **inner == Type::Any {
                return true;
            }
            // A nullable target accepts null independently. Compare the source's
            // present-value refinement, including nullable-capable parameters.
            return *source == Type::Null || self.check(&source.non_null(), inner);
        }
        if let Type::Parameter { name, nonnull } = source
            && crate::generics::parameter(name).is_some_and(|p| {
                p.bounds.iter().any(|bound| {
                    if *nonnull {
                        self.check(&bound.non_null(), target)
                    } else {
                        self.check(bound, target)
                    }
                })
            })
        {
            return true;
        }
        if let Some(parent) = crate::declarations::parent(source)
            && self.check(&parent, target)
        {
            return true;
        }
        match (source, target) {
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
                    && av == bv
                    && (!as_ || *bs)
                    && a.iter().zip(b.iter()).all(|(a, b)| self.check(b, a))
                    && self.check(ar, br)
            }
            (Type::Named { name: a, args: aa }, Type::Named { name: b, args: ba }) => {
                if a == "IntRange" && b == "IntProgression" {
                    return true;
                }
                if a != b || aa.len() != ba.len() {
                    return false;
                }
                if ["List", "Set", "Entry"].contains(&a.as_str()) {
                    aa.iter()
                        .zip(ba.iter())
                        .all(|(a, b)| *b == Type::Star || self.check(a, b))
                } else if a == "Map" {
                    (ba[0] == Type::Star || aa[0] == ba[0]) && (ba[1] == Type::Star || self.check(&aa[1], &ba[1]))
                } else {
                    let parameters = crate::declarations::definition(a)
                        .map(|d| d.parameters.clone())
                        .unwrap_or_default();
                    aa.iter().zip(ba.iter()).enumerate().all(|(i, (a, b))| {
                        *b == Type::Star
                            || match parameters.get(i).map_or(0, |p| p.variance) {
                                1 => self.check(a, b),
                                -1 => self.check(b, a),
                                _ => a == b,
                            }
                    })
                }
            }
            _ => false,
        }
    }
}
