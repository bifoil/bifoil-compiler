//! Extract parameter constraints; applicability still checks the complete types.

use super::{Constraints, Direction, Edge, Failure, Mode, Origin};
use crate::model::Type;
use crate::types::graph::RetainedNode;
use std::collections::BTreeSet;

type QueryKey = (Direction, Mode, Origin, usize, RetainedNode, RetainedNode);

impl Constraints<'_> {
    pub(super) fn collect(
        &mut self,
        pattern: &Type,
        actual: &Type,
        direction: Direction,
        mode: Mode,
        origin: Origin,
    ) -> Result<(), Failure> {
        Collector {
            constraints: self,
            checked: BTreeSet::new(),
        }
        .check(pattern, actual, direction, mode, origin, 0)
    }
}

struct Collector<'a, 'p> {
    constraints: &'a mut Constraints<'p>,
    checked: BTreeSet<QueryKey>,
}

impl Collector<'_, '_> {
    fn check(
        &mut self,
        pattern: &Type,
        actual: &Type,
        direction: Direction,
        mode: Mode,
        origin: Origin,
        depth: usize,
    ) -> Result<(), Failure> {
        if depth > 32 {
            return Err(Failure::Budget);
        }
        if matches!(actual, Type::Star | Type::Pending) {
            return Ok(());
        }
        let key = RetainedNode::of(pattern)
            .zip(RetainedNode::of(actual))
            .map(|(pattern, actual)| (direction, mode, origin, depth, pattern, actual));
        if key.as_ref().is_some_and(|key| self.checked.contains(key)) {
            return Ok(());
        }
        self.shape(pattern, actual, direction, mode, origin, depth)?;
        if let Some(key) = key
            && self.checked.len() < 4096
        {
            self.checked.insert(key);
        }
        Ok(())
    }

    fn shape(
        &mut self,
        pattern: &Type,
        actual: &Type,
        direction: Direction,
        mode: Mode,
        origin: Origin,
        depth: usize,
    ) -> Result<(), Failure> {
        if let Type::Parameter { name, .. } = pattern
            && self
                .constraints
                .parameters
                .iter()
                .any(|parameter| parameter.name == *name)
        {
            self.constraints.edges.entry(name.clone()).or_default().push(Edge {
                actual: actual.clone(),
                direction,
                // A bare inference variable receives the literal's already
                // inferred whole type. Reconstruction of that whole value must
                // not fabricate a mutable or invariant common type (G14).
                mode: if direction == Direction::Lower && mode == Mode::Construct {
                    Mode::Value
                } else {
                    mode
                },
                origin,
            });
            return Ok(());
        }
        if let Type::Nullable { inner } = pattern {
            if *actual != Type::Null {
                return self.check(inner, &actual.non_null(), direction, mode, origin, depth + 1);
            }
            return Ok(());
        }
        // Inspect a caller's bound only to recover a structural shape. A leaf
        // parameter above keeps that caller's identity, even during recursion.
        let view = if matches!(actual, Type::Parameter { .. }) {
            crate::generics::member_view(actual)
        } else {
            actual.clone()
        };
        let actual = &view;
        if mode != Mode::Subtype
            && let (Some((left, parameters, mutable)), Some((right, arguments, actual_mutable))) =
                (pattern.collection(), actual.collection())
            && left == right
            && parameters.len() == arguments.len()
        {
            // A pre-existing value in another container family cannot supply a
            // missing type argument by inventing a copy during inference (G14).
            // A literal can be constructed contextually; a completed result may
            // also use an explicit expected type. Final applicability still
            // permits approved copies once other facts establish the target.
            if direction == Direction::Lower && mutable != actual_mutable && mode != Mode::Construct {
                return Ok(());
            }
            for (index, (parameter, argument)) in parameters.iter().zip(arguments).enumerate() {
                let invariant = mutable && actual_mutable && mode != Mode::Construct;
                let child_mode = if invariant {
                    Mode::Subtype
                } else if mode == Mode::Construct {
                    Mode::Construct
                } else if left == "Map" && index == 0 {
                    Mode::Key
                } else {
                    mode
                };
                self.check(
                    parameter,
                    argument,
                    if invariant { Direction::Exact } else { direction },
                    child_mode,
                    origin,
                    depth + 1,
                )?;
            }
            return Ok(());
        }
        match (pattern, actual) {
            (
                Type::Function { params, result, .. },
                Type::Function {
                    params: supplied,
                    result: returned,
                    ..
                },
            ) if params.len() == supplied.len() => {
                for (parameter, argument) in params.iter().zip(supplied.iter()) {
                    self.check(
                        parameter,
                        argument,
                        direction.reverse(),
                        Mode::Subtype,
                        origin,
                        depth + 1,
                    )?;
                }
                self.check(
                    result,
                    returned,
                    direction,
                    if mode == Mode::Construct {
                        Mode::Construct
                    } else {
                        Mode::Subtype
                    },
                    origin,
                    depth + 1,
                )?;
            }
            (Type::Scene { params }, Type::Scene { params: supplied }) if params.len() == supplied.len() => {
                for (parameter, argument) in params.iter().zip(supplied.iter()) {
                    self.check(parameter, argument, Direction::Exact, Mode::Subtype, origin, depth + 1)?;
                }
            }
            (
                Type::Named { name, args },
                Type::Named {
                    name: supplied,
                    args: actual_args,
                },
            ) if name == supplied && args.len() == actual_args.len() => {
                for (index, (parameter, argument)) in args.iter().zip(actual_args.iter()).enumerate() {
                    let variance = crate::declarations::argument_variance(name, index);
                    let position = match variance {
                        1 => direction,
                        -1 => direction.reverse(),
                        _ if mode == Mode::Construct => direction,
                        _ => Direction::Exact,
                    };
                    self.check(
                        parameter,
                        argument,
                        position,
                        if mode == Mode::Construct { mode } else { Mode::Subtype },
                        origin,
                        depth + 1,
                    )?;
                }
            }
            _ => {
                if direction == Direction::Lower
                    && let Some(parent) = crate::declarations::parent(actual)
                {
                    self.check(pattern, &parent, direction, mode, origin, depth + 1)?;
                } else if direction == Direction::Upper
                    && let Some(parent) = crate::declarations::parent(pattern)
                {
                    self.check(&parent, actual, direction, mode, origin, depth + 1)?;
                }
            }
        }
        Ok(())
    }
}
