//! Definition-time well-formedness of annotated types and generic arguments.

use super::definition;
use crate::model::Type;
use crate::types::graph::SharedNode;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn contains_projection(ty: &Type) -> bool {
    ty.any_node(|ty| matches!(ty, Type::Star))
}

pub(crate) fn validate_type(ty: &Type) -> Result<(), String> {
    if !ty.within_depth(32) {
        return Err("type graph budget exceeded".into());
    }
    validate(ty, false, &mut BTreeSet::new())
}

fn validate<'a>(ty: &'a Type, projection: bool, checked: &mut BTreeSet<SharedNode<'a>>) -> Result<(), String> {
    if let Some(node) = SharedNode::of(ty)
        && !checked.insert(node)
    {
        return Ok(());
    }
    match ty {
        Type::Star if !projection => return Err("star projection is only valid as a type argument".into()),
        Type::Nullable { inner } => validate(inner, false, checked)?,
        Type::Function { params, result, .. } => {
            for parameter in params.iter() {
                validate(parameter, false, checked)?;
            }
            validate(result, false, checked)?;
        }
        Type::Scene { params } => {
            for parameter in params.iter() {
                validate(parameter, false, checked)?;
            }
        }
        Type::Named { name, args } => {
            for argument in args.iter() {
                validate(argument, true, checked)?;
            }
            let builtin_arity = match name.as_str() {
                "List" | "MutableList" | "Set" | "MutableSet" => Some(1),
                "Map" | "MutableMap" | "Entry" => Some(2),
                "IntRange" | "IntProgression" | "Equatable" | "StableKey" => Some(0),
                _ => None,
            };
            if let Some(arity) = builtin_arity {
                if args.len() != arity {
                    return Err(format!("type {name} requires {arity} arguments"));
                }
                if ["Set", "MutableSet", "Map", "MutableMap"].contains(&name.as_str())
                    && args[0] != Type::Star
                    && !args[0].stable_key()
                {
                    return Err("set elements and map keys require StableKey".into());
                }
            } else {
                let declaration = definition(name).ok_or_else(|| format!("unknown nominal type '{name}'"))?;
                if declaration.parameters.len() != args.len() {
                    return Err(format!(
                        "type {name} requires {} arguments",
                        declaration.parameters.len()
                    ));
                }
                let arguments = declaration
                    .parameters
                    .iter()
                    .zip(args.iter())
                    .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
                    .collect::<BTreeMap<_, _>>();
                for parameter in &declaration.parameters {
                    if arguments[&parameter.name] == Type::Star {
                        continue;
                    }
                    let mut parameter = parameter.clone();
                    // A projection denotes an unknown argument satisfying its bounds,
                    // rather than a concrete constructor instantiation.
                    parameter
                        .bounds
                        .retain(|bound| !contains_projection(&crate::generics::substitute(bound, &arguments)));
                    crate::generics::check(std::slice::from_ref(&parameter), &arguments)?;
                }
            }
        }
        Type::Parameter { name, .. } if crate::generics::parameter(name).is_none() => {
            return Err(format!("unknown type parameter '{name}'"));
        }
        _ => {}
    }
    Ok(())
}
