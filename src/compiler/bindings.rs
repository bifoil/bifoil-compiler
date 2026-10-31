//! Lexical bindings and copy-on-write compile-time scopes.

mod aliases;
pub(super) use aliases::{index_global_aliases, narrow_binding, record_assignment};

use super::captures::{Publications, Requirements};
use crate::model::{Signature, Type};
use rpds::RedBlackTreeMapSync;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Analysis-only lexical identity. Copies of a scope retain it; a later binding
/// with the same spelling receives another identity. It never enters artifacts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Identity(u64);
impl Identity {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone)]
pub(super) struct Binding {
    pub(super) identity: Identity,
    // Fast assignment invalidation: ordinary stores do not scan unrelated facts.
    pub(super) index_key: bool,
    // Only multiple source names for one shared global share an alias group.
    // Local value copies continue to have independent bindings.
    pub(super) global_aliases: Option<Arc<[String]>>,
    pub(super) volatile: bool,
    // Only compiler-installed receivers grant class-private visibility.
    pub(super) implicit_receiver: bool,
    // An escaped closure can keep reading this cell after later assignments.
    pub(super) escaped: bool,
    pub(super) runtime: String,
    pub(super) ty: Type,
    pub(super) mutable: bool,
    // Reads require initialization on every reachable path; immutable writes
    // require absence of initialization on every reachable path.
    pub(super) initialized: bool,
    pub(super) may_be_initialized: bool,
    pub(super) owner: String,
    pub(super) narrow: Option<Type>,
    pub(super) field_narrows: super::field_proofs::Facts,
    pub(super) callables: Vec<Signature>,
    pub(super) requires: Requirements,
    pub(super) callable_requirements: BTreeMap<String, Arc<CallableRequirements>>,
}
/// Immutable capture facts for one declared overload. All names are retained
/// for transitive proofs and flattened runtime closure environments.
#[derive(Clone, Default)]
pub(super) struct CallableRequirements {
    pub(super) body: Requirements,
    pub(super) defaults: BTreeMap<usize, Requirements>,
    pub(super) publications: Publications,
    pub(super) default_publications: BTreeMap<usize, Publications>,
}
impl CallableRequirements {
    pub(super) fn at_declaration_depth(mut self, depth: usize) -> Self {
        self.body = self.body.iter().map(|required| required.at_depth(depth)).collect();
        for required in self.defaults.values_mut() {
            *required = required.iter().map(|required| required.at_depth(depth)).collect();
        }
        self.publications = self.publications.iter().map(|effect| effect.at_depth(depth)).collect();
        for effects in self.default_publications.values_mut() {
            *effects = effects.iter().map(|effect| effect.at_depth(depth)).collect();
        }
        self
    }
    pub(super) fn all_requirements(&self) -> Requirements {
        self.body
            .iter()
            .chain(self.defaults.values().flatten())
            .cloned()
            .collect()
    }
    pub(super) fn all_names(&self) -> std::collections::BTreeSet<String> {
        self.body
            .iter()
            .chain(self.defaults.values().flatten())
            .map(|required| required.name().to_owned())
            .collect()
    }
}

pub(super) type Scope = Arc<RedBlackTreeMapSync<String, Binding>>;
pub(super) type ScopeStack = Vec<Scope>;
pub(super) fn lookup_ref<'a>(env: &'a ScopeStack, name: &str) -> Option<&'a Binding> {
    env.iter().rev().find_map(|scope| scope.get(name))
}
pub(super) fn lookup(env: &ScopeStack, name: &str) -> Option<Binding> {
    lookup_ref(env, name).cloned()
}

pub(super) fn current_receiver(env: &ScopeStack) -> Option<&Type> {
    lookup_ref(env, "this")
        .filter(|binding| binding.implicit_receiver)
        .map(|binding| &binding.ty)
}

pub(super) fn insert(env: &mut ScopeStack, name: &str, mut value: Binding) -> Result<(), String> {
    for signature in &value.callables {
        value
            .callable_requirements
            .entry(signature.id.clone())
            .or_insert_with(|| {
                Arc::new(CallableRequirements {
                    body: value.requires.clone(),
                    ..Default::default()
                })
            });
    }
    crate::declarations::validate_type(&value.ty)?;
    if let Some(existing) = env.last().unwrap().get(name)
        && !existing.callables.is_empty()
        && !value.callables.is_empty()
    {
        for incoming in &value.callables {
            if existing.callables.iter().any(|s| same_parameter_signature(s, incoming)) {
                return Err(format!(
                    "duplicate parameter signature for '{name}'; return types cannot distinguish overloads"
                ));
            }
            if incoming.scene || existing.callables.iter().any(|s| s.scene) {
                return Err("scene entries cannot be overloaded".into());
            }
        }
        let existing = Arc::make_mut(env.last_mut().unwrap()).get_mut(name).unwrap();
        existing.callable_requirements.extend(value.callable_requirements);
        existing.callables.extend(value.callables);
        existing.requires.extend(value.requires);
        return Ok(());
    }
    if lookup(env, name).is_some() {
        return Err(format!("shadowing or duplicate declaration '{name}'"));
    }
    Arc::make_mut(env.last_mut().unwrap()).insert_mut(name.into(), value);
    Ok(())
}

pub(super) fn symbol_mut<'a>(env: &'a mut ScopeStack, name: &str) -> Option<&'a mut Binding> {
    let index = env.iter().rposition(|scope| scope.contains_key(name))?;
    Arc::make_mut(&mut env[index]).get_mut(name)
}

impl Binding {
    /// Narrowing adds a usable view; it does not change the binding's declared
    /// type. An expected type may still consume that original identity, notably T
    /// after an `is String` test. No value conversion is performed here.
    pub(super) fn value_type(&self, expected: Option<&Type>) -> Type {
        contextual_type(&self.ty, self.narrow.as_ref(), expected)
    }

    pub(super) fn signature(&self) -> Option<Signature> {
        (self.callables.len() == 1).then(|| self.callables[0].clone())
    }

    pub(super) fn signature_by_id(&self, id: &str) -> Signature {
        self.callables
            .iter()
            .find(|s| s.id == id)
            .expect("registered callable")
            .clone()
    }

    pub(super) fn update_signature(&mut self, signature: Signature) {
        let target = self
            .callables
            .iter_mut()
            .find(|s| s.id == signature.id)
            .expect("registered callable");
        *target = signature;
        if let Some(signature) = self.signature() {
            self.ty = signature.ty();
        }
    }
}

/// A stable read keeps its declared identity beside its usable refined view.
pub(super) fn contextual_type(declared: &Type, refined: Option<&Type>, expected: Option<&Type>) -> Type {
    let usable = refined.unwrap_or(declared);
    if let Some(target) = expected.filter(|target| !target.accepts(usable)) {
        if target.accepts(declared) {
            return declared.clone();
        }
        if !usable.permits_null() && target.accepts(&declared.non_null()) {
            return declared.non_null();
        }
    }
    usable.clone()
}

/// Generic parameter names, defaults, names and results do not distinguish overloads.
pub(crate) fn same_parameter_signature(a: &Signature, b: &Signature) -> bool {
    fn shape(signature: &Signature) -> Vec<(Type, bool)> {
        let arguments = signature
            .generics
            .iter()
            .enumerate()
            .map(|(index, p)| (p.name.clone(), Type::parameter(format!("#overload{index}"))))
            .collect();
        signature
            .params
            .iter()
            .map(|p| (crate::generics::substitute(&p.ty, &arguments), p.vararg))
            .collect()
    }
    shape(a) == shape(b)
}

pub(super) fn local_function_id(parent: &str, fun: &crate::syntax::FunctionAst) -> String {
    format!("{parent}${}@{}", fun.name, fun.line)
}
