//! Common inference types without inventing Any or projection arguments.
//! Function and nominal views must remain genuine subtypes, never adapters.

use super::Failure;
use crate::model::Type;
use crate::types::graph::RetainedNode;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Relationship {
    Join,
    StrictJoin,
    Meet,
}

#[derive(Default)]
pub(super) struct Common {
    checked: BTreeMap<(Relationship, usize, RetainedNode, RetainedNode), Option<Type>>,
}
impl Common {
    pub(super) fn join(&mut self, left: &Type, right: &Type) -> Result<Option<Type>, Failure> {
        self.check(Relationship::Join, left, right, 0)
    }
    pub(super) fn meet(&mut self, left: &Type, right: &Type) -> Result<Option<Type>, Failure> {
        self.check(Relationship::Meet, left, right, 0)
    }
    fn check(
        &mut self,
        relation: Relationship,
        left: &Type,
        right: &Type,
        depth: usize,
    ) -> Result<Option<Type>, Failure> {
        if depth > 32 {
            return Err(Failure::Budget);
        }
        let key = RetainedNode::of(left)
            .zip(RetainedNode::of(right))
            .map(|(left, right)| (relation, depth, left, right));
        if let Some(result) = key.as_ref().and_then(|key| self.checked.get(key)) {
            return Ok(result.clone());
        }
        let result = self.relationship(relation, left, right, depth)?;
        if result.as_ref().is_some_and(|ty| !ty.within_depth(32)) {
            return Err(Failure::Budget);
        }
        if let Some(key) = key
            && self.checked.len() < 4096
        {
            self.checked.insert(key, result.clone());
        }
        Ok(result)
    }
    fn relationship(
        &mut self,
        relation: Relationship,
        left: &Type,
        right: &Type,
        depth: usize,
    ) -> Result<Option<Type>, Failure> {
        if let Some(direct) = direct_relationship(relation, left, right) {
            return Ok(Some(direct));
        }
        let nullable = if relation == Relationship::Meet {
            left.permits_null() && right.permits_null()
        } else {
            left.permits_null() || right.permits_null()
        };
        if matches!(left, Type::Nullable { .. } | Type::Null) || matches!(right, Type::Nullable { .. } | Type::Null) {
            let inner = self.check(relation, &left.non_null(), &right.non_null(), depth + 1)?;
            return Ok(inner.map(|ty| if nullable { ty.nullable() } else { ty }));
        }
        let shaped = match (left, right) {
            (Type::Named { name, args: aa }, Type::Named { name: other, args: ba })
                if name == other && aa.len() == ba.len() =>
            {
                self.nominal_relationship(relation, name, aa, ba, depth)?
            }
            (Type::Function { .. }, Type::Function { .. }) => {
                self.function_relationship(relation, left, right, depth)?
            }
            _ => None,
        };
        if let Some(candidate) = shaped {
            let valid = if relation == Relationship::Meet {
                candidate.subtype_of(left) && candidate.subtype_of(right)
            } else {
                left.subtype_of(&candidate) && right.subtype_of(&candidate)
            };
            if valid {
                return Ok(Some(candidate));
            }
        }
        Ok(if relation == Relationship::Meet && !left.may_overlap(right) {
            Some(Type::Nothing)
        } else {
            None
        })
    }
    fn nominal_relationship(
        &mut self,
        relation: Relationship,
        name: &str,
        aa: &[Type],
        ba: &[Type],
        depth: usize,
    ) -> Result<Option<Type>, Failure> {
        let mut arguments = vec![];
        for (index, (a, b)) in aa.iter().zip(ba.iter()).enumerate() {
            let variance = crate::declarations::argument_variance(name, index);
            let item = self.argument_relationship(relation, variance, a, b, depth)?;
            let Some(item) = item else {
                return Ok((relation == Relationship::Meet).then_some(Type::Nothing));
            };
            arguments.push(item);
        }
        Ok(Some(Type::named(name, arguments)))
    }

    fn argument_relationship(
        &mut self,
        relation: Relationship,
        variance: i8,
        a: &Type,
        b: &Type,
        depth: usize,
    ) -> Result<Option<Type>, Failure> {
        Ok(if variance == 0 {
            if a == b {
                Some(a.clone())
            } else if relation == Relationship::Meet && *a == Type::Star {
                Some(b.clone())
            } else if relation == Relationship::Meet && *b == Type::Star {
                Some(a.clone())
            } else {
                None
            }
        } else {
            let child = match (relation, variance) {
                (Relationship::Meet, 1) => Relationship::Meet,
                (Relationship::Meet, _) => Relationship::StrictJoin,
                (_, 1) => Relationship::StrictJoin,
                (_, _) => Relationship::Meet,
            };
            self.check(child, a, b, depth + 1)?
        })
    }

    fn function_relationship(
        &mut self,
        relation: Relationship,
        left: &Type,
        right: &Type,
        depth: usize,
    ) -> Result<Option<Type>, Failure> {
        let (
            Type::Function {
                params: a,
                result: ar,
                suspend: ae,
                vararg: av,
            },
            Type::Function {
                params: b,
                result: br,
                suspend: be,
                vararg: bv,
            },
        ) = (left, right)
        else {
            return Ok(None);
        };
        if a.len() != b.len() || av != bv {
            return Ok(None);
        }
        let (inputs, output) = if relation == Relationship::Meet {
            (Relationship::StrictJoin, Relationship::Meet)
        } else {
            (Relationship::Meet, Relationship::StrictJoin)
        };
        let mut parameters = vec![];
        for (a, b) in a.iter().zip(b.iter()) {
            let Some(parameter) = self.check(inputs, a, b, depth + 1)? else {
                return Ok(None);
            };
            parameters.push(parameter);
        }
        let Some(result) = self.check(output, ar, br, depth + 1)? else {
            return Ok(None);
        };
        Ok(Some(Type::Function {
            params: parameters.into(),
            result: Arc::new(result),
            suspend: if relation == Relationship::Meet {
                *ae && *be
            } else {
                *ae || *be
            },
            vararg: *av,
        }))
    }
}

fn direct_relationship(relation: Relationship, left: &Type, right: &Type) -> Option<Type> {
    if relation != Relationship::Meet {
        if let Some(joined) = left.join(right)
            && (relation == Relationship::Join || (left.subtype_of(&joined) && right.subtype_of(&joined)))
        {
            return Some(joined);
        }
    } else {
        if left.subtype_of(right) {
            return Some(left.clone());
        }
        if right.subtype_of(left) {
            return Some(right.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests;
