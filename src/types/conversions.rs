//! Lossless scalar and container conversions; these are distinct from subtyping.
//! Completed shared graph pairs are reused within this one relationship query.

use super::graph::RetainedNode;
use crate::model::Type;
use std::collections::BTreeMap;

const MAX_QUERY_RELATIONSHIPS: usize = 4096;

pub(super) fn rank(target: &Type, source: &Type) -> Option<u8> {
    ConversionQuery::default().check(target, source)
}

#[derive(Default)]
struct ConversionQuery {
    checked: BTreeMap<(RetainedNode, RetainedNode), Option<u8>>,
}
impl ConversionQuery {
    fn check(&mut self, target: &Type, source: &Type) -> Option<u8> {
        let key = RetainedNode::of(target).zip(RetainedNode::of(source));
        if let Some(result) = key.as_ref().and_then(|key| self.checked.get(key)) {
            return *result;
        }
        let result = self.relationship(target, source);
        if let Some(key) = key
            && self.checked.len() < MAX_QUERY_RELATIONSHIPS
        {
            self.checked.insert(key, result);
        }
        result
    }
    fn relationship(&mut self, target: &Type, source: &Type) -> Option<u8> {
        if target == source || *source == Type::Nothing {
            return Some(0);
        }
        if source.subtype_of(target) {
            return Some(1);
        }
        if let Type::Nullable { inner } = target {
            return self.check(inner, &source.non_null());
        }
        if source.permits_null() {
            return None;
        }
        let upper = matches!(source, Type::Parameter { .. }).then(|| crate::generics::operation_view(source));
        let source_type = upper.as_ref().unwrap_or(source);
        if (*target == Type::Float && *source_type == Type::Int)
            || (*target == Type::String && *source_type == Type::Char)
        {
            return Some(1);
        }
        if let (
            Some((target_family, target_arguments, target_mutable)),
            Some((source_family, source_arguments, source_mutable)),
        ) = (target.collection(), source_type.collection())
        {
            if target_family != source_family || target_arguments.len() != source_arguments.len() {
                return None;
            }
            if target_mutable && source_mutable {
                return None;
            }
            let mut widening = false;
            for (target_argument, source_argument) in target_arguments.iter().zip(source_arguments.iter()) {
                if *target_argument == Type::Star {
                    // Copying preserves the source argument behind the projected
                    // view. Its static widening still participates in overload rank.
                    widening |= target_argument != source_argument;
                    continue;
                }
                let rank = self.check(target_argument, source_argument)?;
                if rank > 1 {
                    return None;
                }
                widening |= target_argument != source_argument;
            }
            return if target_mutable != source_mutable {
                Some(if widening { 3 } else { 2 })
            } else {
                Some(1)
            };
        }
        None
    }
}
