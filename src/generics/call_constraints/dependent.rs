//! Carry observed values across declared subtype bounds before rejecting them.
//!
//! Only the bound pattern contains inference variables. Observed types retain
//! caller identities, even when recursive calls reuse the same parameter IDs.

use super::{Constraints, Direction, Failure, Mode, Origin};
use crate::model::Type;
use std::collections::BTreeMap;

impl Constraints<'_> {
    pub(super) fn depends_on_parameters(&self, bound: &Type) -> bool {
        bound.any_node(|node| {
            matches!(node, Type::Parameter { name, .. }
                if self.parameters.iter().any(|parameter| parameter.name == *name))
        })
    }

    pub(super) fn with_dependent_bounds(&self, types: &BTreeMap<String, Type>) -> Result<Self, Failure> {
        let mut propagated = self.clone();
        for parameter in self.parameters {
            let Some(actual) = types.get(&parameter.name) else {
                continue;
            };
            for bound in &parameter.bounds {
                if self.depends_on_parameters(bound) {
                    // The bounded value may itself use approved scalar widening
                    // to satisfy this bound. Structural collection, function and
                    // nominal positions retain their ordinary variance rules.
                    propagated.collect(bound, actual, Direction::Lower, Mode::Value, Origin::Argument)?;
                }
            }
        }
        Ok(propagated)
    }
}
