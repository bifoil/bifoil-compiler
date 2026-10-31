//! Finite capability formulas for recursive nominal types.
//!
//! A formula says which generic arguments must qualify, or that qualification is
//! impossible. Starting with no requirements and propagating field requirements
//! handles recursive graphs without expanding types such as Node<List<T>> forever.

use super::catalog::Catalog;
use crate::model::Type;
use crate::types::graph::SharedNode;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

type Formulas = BTreeMap<String, Requirement>;
thread_local! {
    static CACHE: RefCell<Option<[Formulas; 2]>> = const { RefCell::new(None) };
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Requirement {
    impossible: bool,
    parameters: BTreeSet<String>,
}
impl Requirement {
    fn impossible() -> Self {
        Self {
            impossible: true,
            parameters: BTreeSet::new(),
        }
    }
    fn require(&mut self, other: Self) {
        if self.impossible || other.impossible {
            *self = Self::impossible();
        } else {
            self.parameters.extend(other.parameters);
        }
    }
}

pub(super) fn invalidate() {
    CACHE.with(|cache| *cache.borrow_mut() = None);
}

fn requirement(ty: &Type, definitions: &Catalog, formulas: &Formulas, stable: bool) -> Requirement {
    requirement_walk(ty, definitions, formulas, stable, &mut BTreeMap::new())
}
fn requirement_walk<'a>(
    ty: &'a Type,
    definitions: &Catalog,
    formulas: &Formulas,
    stable: bool,
    checked: &mut BTreeMap<SharedNode<'a>, Requirement>,
) -> Requirement {
    let node = SharedNode::of(ty);
    if let Some(previous) = node.and_then(|node| checked.get(&node)) {
        return previous.clone();
    }
    let result = match ty {
        Type::Parameter { name, .. } => Requirement {
            impossible: false,
            parameters: BTreeSet::from([name.clone()]),
        },
        Type::Any | Type::Star | Type::Function { .. } | Type::Pending => Requirement::impossible(),
        Type::Nullable { inner } => requirement_walk(inner, definitions, formulas, stable, checked),
        Type::Named { name, args } => {
            let mut result = Requirement::default();
            if let Some(definition) = definitions.get(name) {
                let formula = &formulas[name];
                if formula.impossible || definition.parameters.len() != args.len() {
                    return Requirement::impossible();
                }
                for parameter in &formula.parameters {
                    let Some(index) = definition.parameters.iter().position(|p| p.name == *parameter) else {
                        return Requirement::impossible();
                    };
                    // Projections retain guarantees from explicit capability
                    // markers and closed value bounds. Read the current formulas
                    // directly; derivation cannot recursively enter the cache.
                    let projected_bound = args[index] == Type::Star
                        && definition.parameters[index]
                            .bounds
                            .iter()
                            .any(|bound| bound_guarantees(bound, definitions, formulas, stable));
                    if !projected_bound {
                        result.require(requirement_walk(&args[index], definitions, formulas, stable, checked));
                    }
                }
            } else {
                let allowed = ["List", "Set", "Map", "Entry", "IntRange", "IntProgression"].contains(&name.as_str())
                    || !stable && ["MutableList", "MutableSet", "MutableMap"].contains(&name.as_str());
                if !allowed {
                    return Requirement::impossible();
                }
                for (index, argument) in args.iter().enumerate() {
                    // Set elements and map keys already satisfy StableKey at
                    // construction. A projection hides their identity, not that
                    // guarantee. Map values and List elements have no such bound.
                    let bounded_key = projected_key(name, index, argument);
                    if !bounded_key {
                        result.require(requirement_walk(argument, definitions, formulas, stable, checked));
                    }
                }
            }
            result
        }
        _ => Requirement::default(),
    };
    if let Some(node) = node {
        checked.insert(node, result.clone());
    }
    result
}

fn projected_key(name: &str, index: usize, argument: &Type) -> bool {
    index == 0 && *argument == Type::Star && ["Set", "MutableSet", "Map", "MutableMap"].contains(&name)
}

/// A closed bound has no hidden/outer argument requiring another proof. More
/// general dependent bounds need existential requirements rather than guessing
/// that an unresolved type parameter qualifies.
fn closed_bound(ty: &Type) -> bool {
    let mut pending = vec![ty];
    let mut checked = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        if let Some(node) = SharedNode::of(ty)
            && !checked.insert(node)
        {
            continue;
        }
        match ty {
            Type::Parameter { .. } | Type::Star | Type::Pending | Type::Function { .. } => return false,
            Type::Nullable { inner } => pending.push(inner),
            Type::Named { name, args } => pending.extend(
                args.iter()
                    .enumerate()
                    .filter_map(|(index, argument)| (!projected_key(name, index, argument)).then_some(argument)),
            ),
            Type::Scene { params } => pending.extend(params.iter()),
            _ => {}
        }
    }
    true
}

fn bound_guarantees(bound: &Type, definitions: &Catalog, formulas: &Formulas, stable: bool) -> bool {
    if matches!(bound, Type::Named { name, args }
        if args.is_empty() && (name == "StableKey" || !stable && name == "Equatable"))
    {
        return true;
    }
    if !closed_bound(bound) {
        return false;
    }
    let required = requirement(bound, definitions, formulas, stable);
    !required.impossible && required.parameters.is_empty()
}

fn referenced_types<'a>(
    ty: &'a Type,
    definitions: &'a Catalog,
    names: &mut BTreeSet<String>,
    expanded_bounds: &mut BTreeSet<String>,
) {
    let mut pending = vec![ty];
    let mut checked = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        if let Some(node) = SharedNode::of(ty)
            && !checked.insert(node)
        {
            continue;
        }
        match ty {
            Type::Named { name, args } => {
                names.insert(name.clone());
                pending.extend(args.iter());
                // Bounds contribute edges when a projection relies on them.
                if args.contains(&Type::Star)
                    && expanded_bounds.insert(name.clone())
                    && let Some(definition) = definitions.get(name)
                {
                    for parameter in &definition.parameters {
                        pending.extend(&parameter.bounds);
                    }
                }
            }
            Type::Nullable { inner } => pending.push(inner),
            // Function types never qualify, so their requirements do not matter.
            _ => {}
        }
    }
}

fn derive(definitions: &Catalog, stable: bool) -> Formulas {
    let mut formulas: Formulas = definitions
        .keys()
        .map(|name| (name.clone(), Requirement::default()))
        .collect();
    let mut users = BTreeMap::<String, BTreeSet<String>>::new();
    for (name, definition) in definitions {
        let mut dependencies = BTreeSet::new();
        let mut expanded_bounds = BTreeSet::new();
        for field in definition.fields.values() {
            referenced_types(&field.ty, definitions, &mut dependencies, &mut expanded_bounds);
        }
        if definition.kind != "enum" {
            dependencies.extend(definition.variants.iter().cloned());
        }
        for dependency in dependencies {
            users.entry(dependency).or_default().insert(name.clone());
        }
    }
    let mut pending: VecDeque<_> = definitions.keys().cloned().collect();
    let mut queued: BTreeSet<_> = definitions.keys().cloned().collect();
    while let Some(name) = pending.pop_front() {
        queued.remove(&name);
        let definition = &definitions[&name];
        let mut result = Requirement::default();
        for field in definition.fields.values() {
            if stable && field.mutable {
                result.require(Requirement::impossible());
            }
            result.require(requirement(&field.ty, definitions, &formulas, stable));
        }
        if definition.kind != "enum" {
            for variant in &definition.variants {
                result.require(formulas.get(variant).cloned().unwrap_or_else(Requirement::impossible));
            }
        }
        if formulas[&name] != result {
            formulas.insert(name.clone(), result);
            for user in users.get(&name).into_iter().flatten() {
                if queued.insert(user.clone()) {
                    pending.push_back(user.clone());
                }
            }
        }
    }
    formulas
}

fn uses_nominal_formula(ty: &Type, definitions: &Catalog) -> bool {
    let mut pending = vec![ty];
    let mut checked = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        if let Some(node) = SharedNode::of(ty)
            && !checked.insert(node)
        {
            continue;
        }
        match ty {
            Type::Named { name, args } => {
                if definitions.contains_key(name) {
                    return true;
                }
                pending.extend(args.iter());
            }
            Type::Nullable { inner } => pending.push(inner),
            _ => {}
        }
    }
    false
}

fn formula_for(ty: &Type, stable: bool) -> Requirement {
    super::catalog::with_types(|definitions| {
        // Primitive-only container queries need no unrelated nominal derivation.
        // Both paths use the same requirement rules, without duplicating policy.
        if !uses_nominal_formula(ty, definitions) {
            return requirement(ty, definitions, &Formulas::new(), stable);
        }
        CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.is_none() {
                *cache = Some([derive(definitions, false), derive(definitions, true)]);
            }
            requirement(ty, definitions, &cache.as_ref().unwrap()[usize::from(stable)], stable)
        })
    })
}

fn prove(requirement: Requirement, stable: bool, visiting: &mut BTreeSet<String>) -> bool {
    !requirement.impossible
        && requirement
            .parameters
            .iter()
            .all(|name| parameter_proof(name, stable, visiting))
}

fn parameter_proof(name: &str, stable: bool, visiting: &mut BTreeSet<String>) -> bool {
    if !visiting.insert(name.into()) {
        return false;
    }
    let result = crate::generics::parameter(name).is_some_and(|parameter| {
        parameter.bounds.iter().any(|bound| match bound {
            Type::Named { name, .. } if name == "StableKey" => true,
            Type::Named { name, .. } if name == "Equatable" => !stable,
            _ => prove(formula_for(bound, stable), stable, visiting),
        })
    });
    visiting.remove(name);
    result
}

pub(crate) fn capability(ty: &Type, stable: bool) -> bool {
    prove(formula_for(ty, stable), stable, &mut BTreeSet::new())
}

pub(crate) fn parameter_capability(name: &str, stable: bool) -> bool {
    parameter_proof(name, stable, &mut BTreeSet::new())
}
