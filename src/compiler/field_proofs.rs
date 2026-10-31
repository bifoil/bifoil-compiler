//! Flow facts for stable paths reached through lexical bindings.
//! Fields and stable indexes/keys into immutable containers retain proofs.
//! Mutable fields and selectors are transient: writes and effects invalidate
//! them across aliases. Effectful index expressions cannot establish paths.

mod computed;
mod key;
mod path;

use super::bindings::{Binding, ScopeStack, lookup_ref, symbol_mut};
use crate::model::Type;
use crate::syntax::Expr;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Step {
    Field(String),
    MutableField(String),
    Index {
        key: ConstantKey,
        mutable: bool,
    },
    // An unqualified enum key is resolved while walking its typed receiver,
    // rather than recursively inferring that receiver again.
    EnumIndex(String),
    // Lexical identity prevents a later same-named declaration reusing a proof.
    // Mutable keys additionally depend on ordinary effect invalidation.
    ComputedIndex {
        key: computed::Key,
        container_mutable: bool,
    },
    BindingIndex {
        identity: super::bindings::Identity,
        name: String,
        mutable: bool,
        container_mutable: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ConstantKey {
    Boolean(bool),
    Int(i32),
    Float(u64),
    Char(String),
    String(String),
    Null,
    Unit,
    Enum { declaration: String, case: String },
}
#[derive(Debug, Clone)]
pub(super) struct ValuePath {
    root: String,
    steps: Vec<Step>,
    // A non-null result also proves each safe receiver prefix was present.
    safe_prefixes: Vec<usize>,
}
pub(super) type Facts = Option<Arc<BTreeMap<Vec<Step>, Type>>>;

/// Map diagnostics and flow paths normalize constants through the same key type.
pub(super) fn constant_key(expression: &Expr, env: &ScopeStack, expected: &Type) -> Option<ConstantKey> {
    ConstantKey::source(expression, env, Some(expected))?.converted(expected)
}

/// Reads still perform ordinary member/privacy and index checking first.
pub(super) fn fact(expression: &Expr, env: &ScopeStack) -> Option<Type> {
    let (path, _) = resolved(expression, env)?;
    fact_path(&path, env)
}
fn fact_path(path: &ValuePath, env: &ScopeStack) -> Option<Type> {
    lookup_ref(env, &path.root)?
        .field_narrows
        .as_ref()?
        .get(&path.steps)
        .cloned()
}

fn index_view(receiver: &Type) -> Option<(Type, bool)> {
    let view = crate::generics::member_view(receiver.present());
    if view == Type::String {
        return Some((view, false));
    }
    let (family, _, mutable) = view.collection()?;
    matches!(family, "List" | "Map").then_some((view, mutable))
}

fn stable_type(path: &mut ValuePath, env: &ScopeStack) -> Option<Type> {
    let binding = lookup_ref(env, &path.root)?;
    if binding.volatile || path.steps.is_empty() {
        return None;
    }
    let mut receiver = binding.value_type(None);
    let mut declared = None;
    for index in 0..path.steps.len() {
        let ty = match &mut path.steps[index] {
            Step::Field(name) | Step::MutableField(name) => {
                let field = crate::declarations::field(receiver.present(), name)?;
                if crate::declarations::field_can_change(receiver.present(), name) {
                    path.steps[index] = Step::MutableField(name.clone());
                }
                field.ty
            }
            Step::BindingIndex {
                identity,
                name,
                container_mutable,
                ..
            } => {
                if lookup_ref(env, name)?.identity != *identity {
                    return None;
                }
                let (view, mutable) = index_view(&receiver)?;
                *container_mutable = mutable;
                super::members::index_types(&view).ok()?.1
            }
            Step::ComputedIndex { key, container_mutable } => {
                if !key.bindings_match(env) {
                    return None;
                }
                let (view, mutable) = index_view(&receiver)?;
                *container_mutable = mutable;
                super::members::index_types(&view).ok()?.1
            }
            Step::Index { key, mutable } => {
                let (view, writable) = index_view(&receiver)?;
                *mutable = writable;
                let map = view.collection().is_some_and(|(family, _, _)| family == "Map");
                if !map && !matches!(key, ConstantKey::Int(number) if *number >= 0) {
                    return None;
                }
                let (expected, item) = super::members::index_types(&view).ok()?;
                *key = key.converted(&expected)?;
                item
            }
            Step::EnumIndex(name) => {
                let (view, mutable) = index_view(&receiver)?;
                let (expected, item) = super::members::index_types(&view).ok()?;
                let key = ConstantKey::source(&Expr::Name(name.clone()), env, Some(&expected))?.converted(&expected)?;
                path.steps[index] = Step::Index { key, mutable };
                item
            }
        };
        receiver = binding
            .field_narrows
            .as_ref()
            .and_then(|facts| facts.get(&path.steps[..=index]))
            .cloned()
            .unwrap_or_else(|| ty.clone());
        declared = Some(ty);
    }
    declared
}

fn resolved(expression: &Expr, env: &ScopeStack) -> Option<(ValuePath, Type)> {
    let mut path = path::resolve(expression, env)?;
    let ty = stable_type(&mut path, env)?;
    Some((path, ty))
}
pub(super) fn declared_type(expression: &Expr, env: &ScopeStack) -> Option<Type> {
    resolved(expression, env).map(|(_, ty)| ty)
}

/// Permanent paths survive arbitrary calls; other paths require an unchanged root.
pub(super) fn subject_path(expression: &Expr, env: &ScopeStack) -> Option<(ValuePath, bool)> {
    let (path, _) = resolved(expression, env)?;
    let permanent = !lookup_ref(env, &path.root)?.mutable && !transient_path(&path.steps);
    Some((path, permanent))
}

/// None requests a non-null refinement of the current usable view.
pub(super) fn narrow(expression: &Expr, env: &mut ScopeStack, tested: Option<&Type>, stable_only: bool) {
    if let Some((path, permanent)) = subject_path(expression, env)
        && (!stable_only || permanent)
    {
        narrow_path(&path, env, tested);
    }
}

pub(super) fn narrow_path(path: &ValuePath, env: &mut ScopeStack, tested: Option<&Type>) {
    let mut path = path.clone();
    if !path.safe_prefixes.is_empty() && tested.is_some_and(Type::permits_null) {
        // Matching a nullable type can succeed because the receiver was null.
        return;
    }
    let Some(declared) = stable_type(&mut path, env) else {
        return;
    };
    for &prefix in &path.safe_prefixes {
        if prefix == 0 {
            super::bindings::narrow_binding(env, &path.root, None, false);
        } else {
            narrow_path(
                &ValuePath {
                    root: path.root.clone(),
                    steps: path.steps[..prefix].to_vec(),
                    safe_prefixes: Vec::new(),
                },
                env,
                None,
            );
        }
    }
    let current = fact_path(&path, env).unwrap_or(declared);
    let narrowed = tested.map_or_else(|| current.non_null(), Clone::clone);
    let binding = symbol_mut(env, &path.root).expect("resolved stable path root");
    let facts = binding.field_narrows.get_or_insert_with(Arc::default);
    Arc::make_mut(facts).insert(path.steps.clone(), narrowed);
    for step in &path.steps {
        match step {
            Step::BindingIndex {
                identity,
                name,
                mutable: true,
                ..
            } => mark_index_key(*identity, name, env),
            Step::ComputedIndex { key, .. } => key.mark_mutable_bindings(env),
            _ => {}
        }
    }
}

fn mutable_field_path(path: &[Step]) -> bool {
    path.iter().any(|step| matches!(step, Step::MutableField(_)))
}
fn mutable_container_path(path: &[Step]) -> bool {
    path.iter().any(|step| {
        matches!(
            step,
            Step::Index { mutable: true, .. }
                | Step::BindingIndex {
                    container_mutable: true,
                    ..
                }
                | Step::ComputedIndex {
                    container_mutable: true,
                    ..
                }
        )
    })
}
fn mutable_heap_path(path: &[Step]) -> bool {
    mutable_field_path(path) || mutable_container_path(path)
}
fn mutable_key_path(path: &[Step]) -> bool {
    path.iter().any(|step| match step {
        Step::BindingIndex { mutable: true, .. } => true,
        Step::ComputedIndex { key, .. } => key.mutable(),
        _ => false,
    })
}
fn transient_path(path: &[Step]) -> bool {
    mutable_heap_path(path) || mutable_key_path(path)
}
fn has_matching_facts(binding: &Binding, affected: fn(&[Step]) -> bool) -> bool {
    binding
        .field_narrows
        .as_ref()
        .is_some_and(|facts| facts.keys().any(|path| affected(path)))
}
pub(super) fn transient_facts(binding: &Binding) -> bool {
    has_matching_facts(binding, transient_path)
}

/// Calls invalidate all transient paths. Heap writes and key reassignments
/// affect separate selectors, preserving independent proofs of unchanged values.
pub(super) fn clear_transient(binding: &mut Binding) {
    clear_matching(binding, transient_path);
}
fn clear_matching(binding: &mut Binding, affected: fn(&[Step]) -> bool) {
    if !has_matching_facts(binding, affected) {
        return;
    }
    let facts = binding.field_narrows.as_ref().unwrap();
    let retained = facts
        .iter()
        .filter(|(path, _)| !affected(path))
        .map(|(path, ty)| (path.clone(), ty.clone()))
        .collect::<BTreeMap<_, _>>();
    binding.field_narrows = (!retained.is_empty()).then(|| Arc::new(retained));
}
fn invalidate_matching(env: &mut ScopeStack, affected: fn(&[Step]) -> bool) {
    for scope in env {
        let names = scope
            .iter()
            .filter(|(_, binding)| has_matching_facts(binding, affected))
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if names.is_empty() {
            continue;
        }
        let scope = Arc::make_mut(scope);
        for name in names {
            clear_matching(scope.get_mut(&name).unwrap(), affected);
        }
    }
}
pub(super) fn invalidate_keys(env: &mut ScopeStack) {
    invalidate_matching(env, mutable_key_path);
}
pub(super) fn invalidate_heap(env: &mut ScopeStack) {
    invalidate_matching(env, mutable_heap_path);
}

pub(super) fn invalidate_write(target: &Expr, env: &mut ScopeStack) {
    match target {
        Expr::Member { .. } => invalidate_matching(env, mutable_field_path),
        Expr::Index { .. } => invalidate_matching(env, mutable_container_path),
        _ => invalidate_heap(env),
    }
}
pub(super) fn invalidate_builtin(effect: super::members::BuiltinHeapEffect, env: &mut ScopeStack) {
    match effect {
        super::members::BuiltinHeapEffect::ReadOnly => {}
        super::members::BuiltinHeapEffect::Containers => invalidate_matching(env, mutable_container_path),
        super::members::BuiltinHeapEffect::Unknown => invalidate_heap(env),
    }
}

fn mark_index_key(identity: super::bindings::Identity, name: &str, env: &mut ScopeStack) {
    let Some(binding) = lookup_ref(env, name).filter(|binding| binding.identity == identity) else {
        return;
    };
    if binding.index_key {
        return;
    }
    let aliases = binding.global_aliases.clone();
    if let Some(aliases) = aliases {
        for name in aliases.iter() {
            symbol_mut(env, name).expect("registered shared alias").index_key = true;
        }
    } else {
        symbol_mut(env, name).unwrap().index_key = true;
    }
}

/// Every reaching branch must prove the same path type. Scope snapshots share
/// maps until a new proof is added; no map is allocated for ordinary bindings.
pub(super) fn merge(states: &[Option<&Binding>]) -> Facts {
    let first = states.first().copied().flatten()?.field_narrows.as_ref()?;
    if states.iter().all(|state| {
        state
            .and_then(|binding| binding.field_narrows.as_ref())
            .is_some_and(|facts| Arc::ptr_eq(first, facts))
    }) {
        return Some(first.clone());
    }
    let common = first
        .iter()
        .filter(|(path, ty)| {
            states.iter().all(|state| {
                state
                    .and_then(|binding| binding.field_narrows.as_ref())
                    .and_then(|facts| facts.get(*path))
                    == Some(*ty)
            })
        })
        .map(|(path, ty)| (path.clone(), ty.clone()))
        .collect::<BTreeMap<_, _>>();
    if common.len() == first.len() {
        Some(first.clone())
    } else if common.is_empty() {
        None
    } else {
        Some(Arc::new(common))
    }
}
