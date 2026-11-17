//! Greatest fixed points for nominal fields; least proofs for hidden bounds.

use super::{Results, State};
use crate::declarations::catalog::Catalog;
use crate::model::{Type, TypeDef};
use crate::types::graph::SharedNode;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

type Arguments = BTreeMap<String, bool>;

pub(super) struct Query<'a> {
    definitions: &'a Catalog,
    confirmed: &'a Results,
    pending: Results,
    queue: VecDeque<State>,
    queued: BTreeSet<State>,
    // None denotes the root query; Some identifies a dependent nominal state.
    users: BTreeMap<State, BTreeSet<Option<State>>>,
    current: Option<State>,
    root_dirty: bool,
    stable: bool,
    active_parameters: BTreeSet<String>,
}

impl<'a> Query<'a> {
    pub(super) fn new(definitions: &'a Catalog, confirmed: &'a Results, stable: bool) -> Self {
        Self {
            definitions,
            confirmed,
            pending: Results::new(),
            queue: VecDeque::new(),
            queued: BTreeSet::new(),
            users: BTreeMap::new(),
            current: None,
            root_dirty: false,
            stable,
            active_parameters: BTreeSet::new(),
        }
    }

    pub(super) fn completed(self) -> Results {
        self.pending
    }

    pub(super) fn solve(&mut self, root: &Type) -> bool {
        let mut result = self.value(root, &Arguments::new());
        while let Some(state) = self.queue.pop_front() {
            self.queued.remove(&state);
            if !self.pending[&state] {
                continue;
            }
            self.current = Some(state.clone());
            let eligible = self.definition(&state);
            self.current = None;
            if !eligible {
                self.pending.insert(state.clone(), false);
                let users = self.users.get(&state).cloned().unwrap_or_default();
                for user in users {
                    if let Some(user) = user {
                        self.enqueue(user);
                    } else {
                        self.root_dirty = true;
                    }
                }
            }
            if self.root_dirty {
                // Rechecking the root can discover a different argument-fact
                // configuration, whose new states join this same work queue.
                result = self.value(root, &Arguments::new()) && result;
                self.root_dirty = false;
            }
        }
        result
    }

    fn enqueue(&mut self, state: State) {
        if self.queued.insert(state.clone()) {
            self.queue.push_back(state);
        }
    }

    fn nominal(&mut self, name: &str, arguments: Vec<bool>) -> bool {
        let state = State {
            name: name.to_owned(),
            arguments,
        };
        if let Some(result) = self.confirmed.get(&state) {
            return *result;
        }
        if !self.pending.contains_key(&state) {
            self.pending.insert(state.clone(), true);
            self.enqueue(state.clone());
        }
        let result = self.pending[&state];
        self.users.entry(state).or_default().insert(self.current.clone());
        result
    }

    fn definition(&mut self, state: &State) -> bool {
        let definitions = self.definitions;
        let Some(definition) = definitions.get(&state.name) else {
            return false;
        };
        if definition.parameters.len() != state.arguments.len() {
            return false;
        }
        let arguments = definition
            .parameters
            .iter()
            .zip(&state.arguments)
            .map(|(parameter, value)| (parameter.name.clone(), *value))
            .collect();
        for field in definition.fields.values() {
            if self.stable && field.mutable || !self.value(&field.ty, &arguments) {
                return false;
            }
        }
        definition.kind == "enum"
            || definition
                .variants
                .iter()
                .all(|variant| self.nominal(variant, state.arguments.clone()))
    }

    fn value(&mut self, ty: &Type, arguments: &Arguments) -> bool {
        self.type_value(ty, arguments, &mut BTreeMap::new())
    }

    fn type_value<'t>(
        &mut self,
        ty: &'t Type,
        arguments: &Arguments,
        checked: &mut BTreeMap<SharedNode<'t>, bool>,
    ) -> bool {
        let node = SharedNode::of(ty);
        if let Some(previous) = node.and_then(|node| checked.get(&node)) {
            return *previous;
        }
        let result = match ty {
            Type::Parameter { name, .. } => arguments
                .get(name)
                .copied()
                .unwrap_or_else(|| self.parameter(name, arguments)),
            Type::Any | Type::Star | Type::Function { .. } | Type::Pending => false,
            Type::Nullable { inner } => self.type_value(inner, arguments, checked),
            Type::Named { name, args } => {
                let definitions = self.definitions;
                if let Some(definition) = definitions.get(name) {
                    if definition.parameters.len() != args.len() {
                        return false;
                    }
                    let resolved = self.arguments(definition, args, arguments, checked);
                    self.nominal(name, resolved)
                } else {
                    self.builtin(name, args, arguments, checked)
                }
            }
            _ => true,
        };
        if let Some(node) = node {
            checked.insert(node, result);
        }
        result
    }

    fn builtin<'t>(
        &mut self,
        name: &str,
        args: &'t [Type],
        arguments: &Arguments,
        checked: &mut BTreeMap<SharedNode<'t>, bool>,
    ) -> bool {
        let allowed = ["List", "Set", "Map", "Entry", "IntRange", "IntProgression"].contains(&name)
            || !self.stable && ["MutableList", "MutableSet", "MutableMap"].contains(&name);
        allowed
            && args.iter().enumerate().all(|(index, argument)| {
                // Construction already checked key stability; projection hides the
                // identity but retains this guarantee. Lists/Map values lack it.
                index == 0 && *argument == Type::Star && ["Set", "MutableSet", "Map", "MutableMap"].contains(&name)
                    || self.type_value(argument, arguments, checked)
            })
    }

    fn arguments<'t>(
        &mut self,
        definition: &TypeDef,
        actual: &'t [Type],
        outer: &Arguments,
        checked: &mut BTreeMap<SharedNode<'t>, bool>,
    ) -> Vec<bool> {
        let mut values: Arguments = definition
            .parameters
            .iter()
            .zip(actual)
            .map(|(parameter, argument)| {
                (
                    parameter.name.clone(),
                    *argument != Type::Star && self.type_value(argument, outer, checked),
                )
            })
            .collect();
        loop {
            let mut changed = false;
            for (parameter, argument) in definition.parameters.iter().zip(actual) {
                if *argument == Type::Star
                    && !values[&parameter.name]
                    && parameter.bounds.iter().any(|bound| self.bound(bound, &values))
                {
                    values.insert(parameter.name.clone(), true);
                    changed = true;
                }
            }
            if !changed {
                return definition
                    .parameters
                    .iter()
                    .map(|parameter| values[&parameter.name])
                    .collect();
            }
        }
    }

    fn bound(&mut self, ty: &Type, arguments: &Arguments) -> bool {
        matches!(ty, Type::Named { name, args } if args.is_empty()
            && (name == "StableKey" || !self.stable && name == "Equatable"))
            || self.value(ty, arguments)
    }

    fn parameter(&mut self, name: &str, arguments: &Arguments) -> bool {
        if !self.active_parameters.insert(name.to_owned()) {
            return false;
        }
        let result = crate::generics::parameter(name)
            .is_some_and(|parameter| parameter.bounds.iter().any(|bound| self.bound(bound, arguments)));
        self.active_parameters.remove(name);
        result
    }
}
