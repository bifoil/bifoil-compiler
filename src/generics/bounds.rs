//! Safe member views from generic upper bounds, without value conversion.
//!
//! Member access has already checked or guarded a null receiver. Capabilities
//! constrain operations but do not describe a runtime receiver shape.

use crate::model::Type;
use std::collections::BTreeSet;

pub(crate) fn member_view(receiver: &Type) -> Type {
    fn resolve(receiver: &Type, active: &mut BTreeSet<String>) -> Type {
        let receiver = receiver.present();
        let Type::Parameter { name, .. } = receiver else {
            return receiver.clone();
        };
        if !active.insert(name.clone()) {
            return Type::Any;
        }
        let mut best = Type::Any;
        if let Some(parameter) = super::parameter(name) {
            for bound in parameter.bounds {
                if matches!(&bound, Type::Named { name, .. } if name == "Equatable" || name == "StableKey") {
                    continue;
                }
                let candidate = resolve(&bound, active);
                if candidate.subtype_of(&best) {
                    best = candidate;
                } else if !best.subtype_of(&candidate) {
                    // Unrelated closed nominal bounds have no common runtime
                    // receiver. Do not invent an intersection member interface.
                    best = Type::Any;
                    break;
                }
            }
        }
        active.remove(name);
        best
    }
    resolve(receiver, &mut BTreeSet::new())
}

/// Primitive operations require a non-null operand before using its upper view.
pub(crate) fn operation_view(operand: &Type) -> Type {
    if matches!(operand, Type::Parameter { .. }) && !operand.permits_null() {
        member_view(operand)
    } else {
        operand.clone()
    }
}
