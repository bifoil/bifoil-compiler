//! Shared argument-shape validation; no value expressions are evaluated here.

use crate::model::Signature;
use std::collections::HashMap;

pub(in crate::compiler) struct Form<'a> {
    pub name: Option<&'a str>,
    pub spread: bool,
}

pub(in crate::compiler) fn positions<'a>(
    signature: &Signature,
    arguments: impl IntoIterator<Item = Form<'a>>,
) -> Result<Vec<usize>, String> {
    let mut used = vec![false; signature.params.len()];
    let mut named = false;
    let mut named_indices = None;
    let mut positions = vec![];
    let variadic = signature.params.last().is_some_and(|p| p.vararg);
    for (position, Form { name, spread }) in arguments.into_iter().enumerate() {
        let index = if let Some(name) = name {
            named = true;
            let indices = named_indices.get_or_insert_with(|| {
                signature
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, parameter)| (parameter.name.as_str(), index))
                    .collect::<HashMap<_, _>>()
            });
            indices
                .get(name)
                .copied()
                .ok_or_else(|| format!("unknown argument '{name}'"))?
        } else {
            if named {
                return Err("positional arguments must precede named arguments".into());
            }
            if variadic {
                position.min(signature.params.len() - 1)
            } else {
                position
            }
        };
        let parameter = signature.params.get(index).ok_or("too many arguments")?;
        if used[index] && (!parameter.vararg || name.is_some() || named) {
            return Err("duplicate argument".into());
        }
        if spread && !parameter.vararg {
            return Err("spread arguments supply only the variadic parameter".into());
        }
        used[index] = true;
        positions.push(index);
    }
    for (index, parameter) in signature.params.iter().enumerate() {
        if !used[index] && !parameter.default && !parameter.vararg {
            return Err(format!("missing required argument '{}'", parameter.name));
        }
    }
    Ok(positions)
}

/// Declaration-order defaults that remain after resolving supplied arguments.
pub(in crate::compiler) fn omitted_defaults(signature: &Signature, positions: &[usize]) -> Vec<usize> {
    let mut supplied = vec![false; signature.params.len()];
    for &position in positions {
        supplied[position] = true;
    }
    signature
        .params
        .iter()
        .enumerate()
        .filter_map(|(index, parameter)| (parameter.default && !supplied[index]).then_some(index))
        .collect()
}

#[cfg(test)]
mod tests;
