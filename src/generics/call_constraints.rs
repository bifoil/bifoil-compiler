//! Candidate-only constraints for calls rejected by the ordinary inference pass.
//! Final argument applicability remains authoritative; this emits no code.

mod collect;
mod common;
mod dependent;

use crate::model::{GenericParam, Type};
use std::collections::BTreeMap;

#[derive(Debug)]
pub(crate) enum Failure {
    Conflict,
    Budget,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Conflict => "generic argument constraints are incompatible",
            Self::Budget => "generic inference type graph budget exceeded; supply type arguments explicitly",
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Direction {
    Lower,
    Upper,
    Exact,
}
impl Direction {
    fn reverse(self) -> Self {
        match self {
            Self::Lower => Self::Upper,
            Self::Upper => Self::Lower,
            Self::Exact => Self::Exact,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Mode {
    Value,
    Construct,
    Subtype,
    Key,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Origin {
    Argument,
    Context,
}
#[derive(Clone, Copy)]
enum BoundPhase {
    Independent,
    Complete,
}

struct Candidate {
    ty: Type,
    contextual: bool,
}

#[derive(Clone)]
struct Edge {
    actual: Type,
    direction: Direction,
    mode: Mode,
    origin: Origin,
}

#[derive(Clone)]
pub(crate) struct Constraints<'a> {
    parameters: &'a [GenericParam],
    edges: BTreeMap<String, Vec<Edge>>,
}
impl<'a> Constraints<'a> {
    pub(crate) fn new(parameters: &'a [GenericParam]) -> Self {
        Self {
            parameters,
            edges: BTreeMap::new(),
        }
    }
    pub(crate) fn argument(&mut self, pattern: &Type, actual: &Type, constructible: bool) -> Result<(), Failure> {
        self.collect(
            pattern,
            actual,
            Direction::Lower,
            if constructible { Mode::Construct } else { Mode::Value },
            Origin::Argument,
        )
    }
    pub(crate) fn context(&mut self, pattern: &Type, expected: &Type) -> Result<(), Failure> {
        self.collect(pattern, expected, Direction::Upper, Mode::Value, Origin::Context)
    }
    pub(crate) fn solve(&self) -> Result<BTreeMap<String, Type>, Failure> {
        let mut types = BTreeMap::new();
        for _ in 0..64 {
            // First propagate observed bounded values back into their bound's
            // parameters. A bound alone cannot seed an unobserved type argument.
            let constraints = self.with_dependent_bounds(&types)?;
            let (provisional, provisional_conflict) = constraints.select(&types, BoundPhase::Independent)?;
            let (proposed, bound_conflict) = constraints.select(&provisional, BoundPhase::Complete)?;
            if proposed.values().any(|ty| !ty.within_depth(32)) {
                return Err(Failure::Budget);
            }
            if proposed == types {
                return if provisional_conflict || bound_conflict {
                    Err(Failure::Conflict)
                } else {
                    Ok(proposed)
                };
            }
            types = proposed;
        }
        Err(Failure::Budget)
    }
    fn select(
        &self,
        previous: &BTreeMap<String, Type>,
        phase: BoundPhase,
    ) -> Result<(BTreeMap<String, Type>, bool), Failure> {
        let mut selected = BTreeMap::new();
        let mut conflict = false;
        for parameter in self.parameters {
            match self.choose(parameter, previous, phase) {
                Ok(Some(candidate)) => {
                    selected.insert(parameter.name.clone(), candidate);
                }
                Ok(None) => {}
                Err(Failure::Budget) => return Err(Failure::Budget),
                Err(Failure::Conflict) => {
                    // A conflicting dependent bound may become satisfiable when
                    // another parameter advances in this round. Reject only at
                    // a fixed point, retaining the last observed type meanwhile.
                    conflict = true;
                    if let Some(candidate) = previous.get(&parameter.name) {
                        selected.insert(parameter.name.clone(), candidate.clone());
                    }
                }
            }
        }
        Ok((selected, conflict))
    }
    fn choose(
        &self,
        parameter: &GenericParam,
        types: &BTreeMap<String, Type>,
        phase: BoundPhase,
    ) -> Result<Option<Type>, Failure> {
        let Some(edges) = self.edges.get(&parameter.name) else {
            return Ok(None);
        };
        let mut common = common::Common::default();
        let exact = edges
            .iter()
            .filter(|edge| edge.direction == Direction::Exact)
            .collect::<Vec<_>>();
        if let Some(first) = exact.first() {
            if exact.iter().any(|edge| edge.actual != first.actual) {
                return Err(Failure::Conflict);
            }
            return self
                .checked(
                    parameter,
                    edges,
                    Candidate {
                        ty: first.actual.clone(),
                        contextual: false,
                    },
                    types,
                    phase,
                )
                .map(Some);
        }
        let lower = edges
            .iter()
            .filter(|edge| edge.direction == Direction::Lower)
            .collect::<Vec<_>>();
        let argument_upper = edges
            .iter()
            .filter(|edge| edge.direction == Direction::Upper && edge.origin == Origin::Argument)
            .collect::<Vec<_>>();
        let contexts = edges
            .iter()
            .filter(|edge| edge.origin == Origin::Context)
            .collect::<Vec<_>>();
        let mut candidates = vec![];
        if let Some(first) = lower.first() {
            let mut joined = Some(first.actual.clone());
            for edge in lower.iter().skip(1) {
                joined = if let Some(previous) = joined {
                    common.join(&previous, &edge.actual)?
                } else {
                    None
                };
            }
            if let Some(joined) = joined {
                candidates.push(Candidate {
                    ty: joined,
                    contextual: false,
                });
            }
            candidates.extend(lower.iter().map(|edge| Candidate {
                ty: edge.actual.clone(),
                contextual: false,
            }));
        } else if let Some(first) = argument_upper.first() {
            let mut meet = Some(first.actual.clone());
            for edge in argument_upper.iter().skip(1) {
                meet = if let Some(previous) = meet {
                    common.meet(&previous, &edge.actual)?
                } else {
                    None
                };
            }
            if let Some(meet) = meet {
                candidates.push(Candidate {
                    ty: meet,
                    contextual: false,
                });
            }
        }
        // Candidates originate in observations, including values propagated
        // through dependent bounds, rather than in an unobserved bound alone.
        candidates.extend(argument_upper.iter().map(|edge| Candidate {
            ty: edge.actual.clone(),
            contextual: false,
        }));
        candidates.extend(contexts.iter().map(|edge| Candidate {
            ty: edge.actual.clone(),
            contextual: true,
        }));
        if candidates.is_empty() {
            return Ok(None);
        }
        for candidate in candidates {
            match self.checked(parameter, edges, candidate, types, phase) {
                Ok(candidate) => return Ok(Some(candidate)),
                Err(Failure::Budget) => return Err(Failure::Budget),
                Err(_) => {}
            }
        }
        Err(Failure::Conflict)
    }
    fn checked(
        &self,
        parameter: &GenericParam,
        edges: &[Edge],
        candidate: Candidate,
        types: &BTreeMap<String, Type>,
        phase: BoundPhase,
    ) -> Result<Type, Failure> {
        let Candidate {
            ty: mut candidate,
            contextual,
        } = candidate;
        if !candidate.within_depth(32) {
            return Err(Failure::Budget);
        }
        let mut upper = edges
            .iter()
            .filter(|edge| edge.direction == Direction::Upper && edge.mode == Mode::Subtype)
            .map(|edge| edge.actual.clone())
            .collect::<Vec<_>>();
        for bound in &parameter.bounds {
            if matches!(phase, BoundPhase::Independent) && self.depends_on_parameters(bound) {
                continue;
            }
            if matches!(bound, Type::Named { name, .. } if name == "Equatable" || name == "StableKey") {
                continue;
            }
            let unresolved = bound.any_node(|node| matches!(node, Type::Parameter { name, .. }
                if self.parameters.iter().any(|p| p.name == *name) && !types.contains_key(name) && *name != parameter.name));
            if !unresolved {
                let mut known = types.clone();
                known.insert(parameter.name.clone(), candidate.clone());
                upper.push(super::substitute(bound, &known));
            }
        }
        for bound in &upper {
            if !candidate.subtype_of(bound) {
                candidate = super::constraints::widening(&candidate, bound).ok_or(Failure::Conflict)?;
            }
        }
        if !candidate.within_depth(32) {
            return Err(Failure::Budget);
        }
        if upper.iter().any(|bound| !candidate.subtype_of(bound)) {
            return Err(Failure::Conflict);
        }
        if edges.iter().any(|edge| !satisfies(&candidate, edge, contextual)) {
            return Err(Failure::Conflict);
        }
        Ok(candidate)
    }
}

fn satisfies(candidate: &Type, edge: &Edge, contextual: bool) -> bool {
    let (source, target) = match edge.direction {
        Direction::Lower => (&edge.actual, candidate),
        Direction::Upper => (candidate, &edge.actual),
        Direction::Exact => return *candidate == edge.actual,
    };
    match edge.mode {
        Mode::Subtype => source.subtype_of(target),
        Mode::Key => target.conversion_from(source).is_some_and(|rank| rank <= 1),
        Mode::Value => target
            .conversion_from(source)
            .is_some_and(|rank| edge.direction == Direction::Upper || contextual || rank <= 1),
        Mode::Construct => construction(target, source),
    }
}

fn construction(target: &Type, source: &Type) -> bool {
    if target.accepts(source) {
        return true;
    }
    if let Type::Nullable { inner } = target {
        return *source == Type::Null || construction(inner, &source.non_null());
    }
    if source.permits_null() {
        return false;
    }
    match (target, source) {
        (Type::Named { name: a, args: aa }, Type::Named { name: b, args: ba }) if a == b && aa.len() == ba.len() => {
            aa.iter().zip(ba.iter()).all(|(a, b)| construction(a, b))
        }
        _ => false,
    }
}
