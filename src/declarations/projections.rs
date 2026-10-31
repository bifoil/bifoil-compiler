//! Safe upper and lower views for existential receiver arguments.
//!
//! Reads widen hidden values; inputs must fit every possible hidden argument.
//! Callable parameters and contravariant nominal arguments reverse that direction.
//! Invariant inputs remain unavailable, while invariant reads retain a star.

use crate::model::{Signature, Type, TypeDef};
use crate::types::graph::SharedNode;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[cfg(test)]
mod tests;

const MAX_VIEW_NODES: usize = 4096;

pub(super) fn hidden_parameters(receiver: &Type, definition: &TypeDef) -> BTreeSet<String> {
    let Type::Named { args, .. } = receiver else {
        return BTreeSet::new();
    };
    definition
        .parameters
        .iter()
        .zip(args.iter())
        .filter(|(_, argument)| **argument == Type::Star)
        .map(|(parameter, _)| parameter.name.clone())
        .collect()
}

pub(super) fn depends_on(ty: &Type, hidden: &BTreeSet<String>) -> bool {
    !hidden.is_empty() && ty.any_node(|ty| matches!(ty, Type::Parameter { name, .. } if hidden.contains(name)))
}

/// Find a sound upper view for a hidden argument. Capability bounds do not
/// denote runtime value types. Recursive bounds retain a conservative view.
fn upper(
    name: &str,
    definition: &TypeDef,
    known: &BTreeMap<String, Type>,
    hidden: &BTreeSet<String>,
    active: &mut BTreeSet<String>,
) -> Type {
    if !active.insert(name.to_owned()) {
        return Type::Any.nullable();
    }
    let mut best = Type::Any.nullable();
    if let Some(parameter) = definition.parameters.iter().find(|parameter| parameter.name == name) {
        for bound in &parameter.bounds {
            let bound = crate::generics::substitute(bound, known);
            if matches!(&bound, Type::Named { name, .. } if name == "Equatable" || name == "StableKey") {
                continue;
            }
            let candidate = if !depends_on(&bound, hidden) {
                bound
            } else if let Type::Parameter { name: other, .. } = &bound {
                upper(other, definition, known, hidden, active)
            } else if !bound.permits_null() {
                Type::Any
            } else {
                Type::Any.nullable()
            };
            if candidate.subtype_of(&best) {
                best = candidate;
            }
        }
    }
    active.remove(name);
    best
}

pub(super) fn read_type<'a>(ty: &'a Type, receiver: &Type, definition: &'a TypeDef) -> Type {
    ProjectionView::new(receiver, definition).view(ty, Direction::Read)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Direction {
    Read,
    Input,
}
impl Direction {
    fn reversed(self) -> Self {
        match self {
            Self::Read => Self::Input,
            Self::Input => Self::Read,
        }
    }
}

/// A node may occur in both callable input and output positions. Cache their
/// distinct proofs separately, within one bounded receiver/query context.
struct ProjectionView<'a> {
    definition: &'a TypeDef,
    known: BTreeMap<String, Type>,
    hidden: BTreeSet<String>,
    checked: BTreeMap<(SharedNode<'a>, Direction), Type>,
}
impl<'a> ProjectionView<'a> {
    fn new(receiver: &Type, definition: &'a TypeDef) -> Self {
        let hidden = hidden_parameters(receiver, definition);
        let known = super::catalog::receiver_type_arguments(receiver, definition)
            .into_iter()
            .filter(|(name, _)| !hidden.contains(name))
            .collect();
        Self {
            definition,
            known,
            hidden,
            checked: BTreeMap::new(),
        }
    }

    fn view(&mut self, ty: &'a Type, direction: Direction) -> Type {
        let node = SharedNode::of(ty).map(|node| (node, direction));
        if let Some(previous) = node.and_then(|node| self.checked.get(&node)) {
            return previous.clone();
        }
        let result = self.view_node(ty, direction);
        if let Some(node) = node
            && self.checked.len() < MAX_VIEW_NODES
        {
            self.checked.insert(node, result.clone());
        }
        result
    }

    fn view_node(&mut self, ty: &'a Type, direction: Direction) -> Type {
        if !depends_on(ty, &self.hidden) {
            return crate::generics::substitute(ty, &self.known);
        }
        match ty {
            Type::Parameter { name, nonnull } => match direction {
                Direction::Input => Type::Nothing,
                Direction::Read => {
                    let bound = upper(name, self.definition, &self.known, &self.hidden, &mut BTreeSet::new());
                    if *nonnull { bound.non_null() } else { bound }
                }
            },
            Type::Nullable { inner } => self.view(inner, direction).nullable(),
            Type::Named { name, args } => self.named(name, args, direction),
            Type::Function {
                params,
                result,
                suspend,
                vararg,
            } => Type::Function {
                params: params
                    .iter()
                    .map(|parameter| self.view(parameter, direction.reversed()))
                    .collect(),
                result: Arc::new(self.view(result, direction)),
                suspend: *suspend,
                vararg: *vararg,
            },
            _ => match direction {
                Direction::Read => Type::Any.nullable(),
                Direction::Input => Type::Nothing,
            },
        }
    }

    fn named(&mut self, name: &str, args: &'a [Type], direction: Direction) -> Type {
        let definition = super::definition(name);
        let mut unavailable = false;
        let arguments = args
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                if !depends_on(argument, &self.hidden) {
                    return crate::generics::substitute(argument, &self.known);
                }
                let variance = if matches!(name, "List" | "Set" | "Entry") || (name == "Map" && index == 1) {
                    1
                } else {
                    definition
                        .as_ref()
                        .and_then(|d| d.parameters.get(index))
                        .map_or(0, |p| p.variance)
                };
                match variance {
                    1 => self.view(argument, direction),
                    -1 => self.view(argument, direction.reversed()),
                    _ => match direction {
                        Direction::Read => Type::Star,
                        Direction::Input => {
                            // Replacing an invariant argument would invent a
                            // different container, rather than a safe subtype.
                            unavailable = true;
                            Type::Nothing
                        }
                    },
                }
            })
            .collect();
        if unavailable {
            return Type::Nothing;
        }
        let candidate = Type::named(name, arguments);
        if super::validate_type(&candidate).is_ok() {
            return candidate;
        }
        if direction == Direction::Input {
            return Type::Nothing;
        }
        // Widening may lose a required capability, such as StableKey for Set<T>.
        // A star retains the original checked argument instead of inventing one.
        let projected = Type::named(
            name,
            args.iter()
                .map(|argument| {
                    if depends_on(argument, &self.hidden) {
                        Type::Star
                    } else {
                        crate::generics::substitute(argument, &self.known)
                    }
                })
                .collect(),
        );
        if super::validate_type(&projected).is_ok() {
            projected
        } else {
            Type::Any
        }
    }
}

pub(super) fn method(signature: &Signature, receiver: &Type, definition: &TypeDef) -> Option<Signature> {
    let hidden = hidden_parameters(receiver, definition);
    if signature
        .generics
        .iter()
        .flat_map(|parameter| &parameter.bounds)
        .any(|bound| depends_on(bound, &hidden))
    {
        return None;
    }
    let known = super::catalog::receiver_type_arguments(receiver, definition);
    let mut result = crate::generics::signature(signature, &known);
    let mut views = ProjectionView::new(receiver, definition);
    for (index, parameter) in signature.params.iter().enumerate() {
        if depends_on(&parameter.ty, &hidden) {
            Arc::make_mut(&mut result).params_mut()[index].ty = views.view(&parameter.ty, Direction::Input);
        }
    }
    if depends_on(&signature.result, &hidden) {
        Arc::make_mut(&mut result).result = views.view(&signature.result, Direction::Read);
    }
    Some(result)
}
