//! Declaration-site variance checks.
//!
//! A receiver's type parameter may appear only in positions permitted by its
//! declared variance. Writable fields are invariant; function inputs reverse
//! the position, and nested type constructors compose their own variances.

use super::{all_types, definition};
use crate::model::{GenericParam, Type};
use crate::types::graph::SharedNode;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn argument_variance(name: &str, index: usize) -> i8 {
    match name {
        "List" | "Set" | "Entry" => 1,
        "Map" if index == 1 => 1,
        "MutableList" | "MutableSet" | "MutableMap" | "Map" => 0,
        _ => definition(name)
            .and_then(|d| d.parameters.get(index).map(|p| p.variance))
            .unwrap_or(0),
    }
}

fn check_position(ty: &Type, position: i8, parameters: &BTreeMap<&str, &GenericParam>) -> Result<(), String> {
    if parameters.is_empty() {
        return Ok(());
    }
    let mut pending = vec![(ty, position)];
    let mut checked = BTreeSet::new();
    while let Some((ty, position)) = pending.pop() {
        if let Some(node) = SharedNode::of(ty)
            && !checked.insert((node, position))
        {
            continue;
        }
        match ty {
            Type::Parameter { name, .. } => {
                if let Some(parameter) = parameters.get(name.as_str())
                    && parameter.variance != 0
                    && parameter.variance != position
                {
                    let name = name.rsplit('#').next().unwrap_or(name);
                    return Err(format!(
                        "variance violation: {name} cannot occur in this {} position",
                        match position {
                            1 => "output",
                            -1 => "input",
                            _ => "invariant",
                        }
                    ));
                }
            }
            Type::Nullable { inner } => pending.push((inner, position)),
            Type::Named { name, args } => pending.extend(
                args.iter()
                    .enumerate()
                    .map(|(index, argument)| (argument, position * argument_variance(name, index))),
            ),
            Type::Function { params, result, .. } => {
                pending.extend(params.iter().map(|parameter| (parameter, -position)));
                pending.push((result, position));
            }
            Type::Scene { params } => pending.extend(params.iter().map(|parameter| (parameter, -position))),
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn validate_variance() -> Result<(), String> {
    for (name, definition) in all_types() {
        let parameters = definition.parameters.iter().map(|p| (p.name.as_str(), p)).collect();
        for field in definition.fields.values() {
            check_position(&field.ty, if field.mutable { 0 } else { 1 }, &parameters)
                .map_err(|error| format!("{name}: {error}"))?;
        }
        for (member, overloads) in &definition.methods {
            // The generated copy creates a new instance. It does not consume a
            // value into the existing receiver, unlike ordinary instance methods.
            if member == "copy" {
                continue;
            }
            for signature in overloads {
                for parameter in signature.params.iter() {
                    check_position(&parameter.ty, -1, &parameters)
                        .map_err(|error| format!("{name}.{member}: {error}"))?;
                }
                for parameter in &signature.generics {
                    for bound in &parameter.bounds {
                        check_position(bound, -1, &parameters)?;
                    }
                }
                check_position(&signature.result, 1, &parameters)
                    .map_err(|error| format!("{name}.{member}: {error}"))?;
            }
        }
    }
    Ok(())
}
