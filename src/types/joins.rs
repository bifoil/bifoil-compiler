//! Compatible common types for inference, without implicit container-family copies.
//! Completed shared graph pairs are reused within this one relationship query.

use super::graph::RetainedNode;
use crate::model::Type;
use std::collections::BTreeMap;

const MAX_QUERY_RELATIONSHIPS: usize = 4096;

pub(super) fn common(first: &Type, second: &Type) -> Option<Type> {
    JoinQuery::default().check(first, second)
}

#[derive(Default)]
struct JoinQuery {
    checked: BTreeMap<(RetainedNode, RetainedNode), Option<Type>>,
}
impl JoinQuery {
    fn check(&mut self, first: &Type, second: &Type) -> Option<Type> {
        let key = RetainedNode::of(first).zip(RetainedNode::of(second));
        if let Some(result) = key.as_ref().and_then(|key| self.checked.get(key)) {
            return result.clone();
        }
        let result = self.relationship(first, second);
        if let Some(key) = key
            && self.checked.len() < MAX_QUERY_RELATIONSHIPS
        {
            self.checked.insert(key, result.clone());
        }
        result
    }
    fn relationship(&mut self, first: &Type, second: &Type) -> Option<Type> {
        if first == second {
            return Some(first.clone());
        }
        if *first == Type::Nothing {
            return Some(second.clone());
        }
        if *second == Type::Nothing {
            return Some(first.clone());
        }
        if *first == Type::Null {
            return Some(second.nullable());
        }
        if *second == Type::Null {
            return Some(first.nullable());
        }
        if first.subtype_of(second) {
            return Some(second.clone());
        }
        if second.subtype_of(first) {
            return Some(first.clone());
        }
        // A nullable-capable parameter is not a Nullable node. Preserve an
        // already-proven upper type before decomposing explicit nullable forms.
        if matches!(first, Type::Nullable { .. }) || matches!(second, Type::Nullable { .. }) {
            return self.check(first.present(), second.present()).map(|t| t.nullable());
        }
        if matches!((first, second), (Type::Int, Type::Float) | (Type::Float, Type::Int)) {
            return Some(Type::Float);
        }
        if matches!((first, second), (Type::Char, Type::String) | (Type::String, Type::Char)) {
            return Some(Type::String);
        }
        if let (Some(left), Some(right)) = (crate::declarations::parent(first), crate::declarations::parent(second))
            && let Some(common) = self.check(&left, &right)
            && first.subtype_of(&common)
            && second.subtype_of(&common)
        {
            // Closed sibling variants can share their declared sum. This is a
            // genuine supertype of both values, with no payload conversion.
            return Some(common);
        }
        if let (Some((first_family, first_arguments, false)), Some((second_family, second_arguments, false))) =
            (first.collection(), second.collection())
            && first_family == second_family
            && first_arguments.len() == second_arguments.len()
        {
            let arguments = first_arguments
                .iter()
                .zip(second_arguments.iter())
                .map(|(left, right)| self.check(left, right))
                .collect::<Option<Vec<_>>>()?;
            return Some(Type::named(first_family, arguments));
        }
        None
    }
}
