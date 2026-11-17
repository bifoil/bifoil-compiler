//! Shared generic reference candidates for complete and partial type contexts.
//!
//! These maps establish type arguments only. The caller must still prove the
//! resulting function subtype, including input variance, shape, and effects.

use crate::model::{Signature, Type};
use std::collections::BTreeMap;

pub(super) struct Options {
    pub(super) arguments: Vec<BTreeMap<String, Type>>,
    pub(super) initial_error: Option<String>,
}

pub(super) fn options(signature: &Signature, expected: &Type, unresolved: &[String]) -> Result<Options, String> {
    let mut joined = BTreeMap::new();
    let inferred =
        crate::generics::infer_partial(&signature.ty(), expected, &signature.generics, &mut joined, unresolved);
    let original_error = inferred
        .and_then(|_| crate::generics::check(&signature.generics, &joined))
        .err();
    let mut choices = vec![];
    retain(signature, joined.clone(), &mut choices);
    crate::generics::refine_nonnull_arguments(&signature.generics, &mut joined);
    retain(signature, joined, &mut choices);

    if let Type::Function { params, result, .. } = expected {
        let mut inputs = BTreeMap::new();
        let mut compatible = true;
        for (parameter, target) in signature.params.iter().zip(params.iter()) {
            compatible &=
                crate::generics::infer_partial(&parameter.ty, target, &signature.generics, &mut inputs, unresolved)
                    .is_ok();
        }
        let returned = signature
            .generics
            .iter()
            .filter(|parameter| !inputs.contains_key(&parameter.name))
            .cloned()
            .collect::<Vec<_>>();
        if compatible
            && crate::generics::infer_partial(&signature.result, result, &returned, &mut inputs, unresolved).is_ok()
        {
            retain(signature, inputs.clone(), &mut choices);
            crate::generics::refine_nonnull_arguments(&signature.generics, &mut inputs);
            retain(signature, inputs, &mut choices);
        }
    }
    if choices.is_empty() {
        Err(original_error.unwrap_or_else(|| "cannot infer a compatible generic reference".into()))
    } else {
        Ok(Options {
            arguments: choices,
            initial_error: original_error,
        })
    }
}

fn retain(signature: &Signature, types: BTreeMap<String, Type>, choices: &mut Vec<BTreeMap<String, Type>>) {
    if crate::generics::check(&signature.generics, &types).is_ok() && !choices.contains(&types) {
        choices.push(types);
    }
}
