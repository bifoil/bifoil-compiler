//! Type names, generic scopes, transparent aliases, and cycle checks.

use crate::model::{GenericParam, Type};
use crate::syntax::AliasAst;
use std::collections::{BTreeMap, BTreeSet};

use super::type_names::Scope as NameBindings;
use std::cell::RefCell;
use std::sync::Arc;

mod order;

/// Generic aliases add a small parameter overlay to one shared lexical map.
struct TypeNames<'a> {
    shared: &'a NameBindings,
    parameters: Option<&'a BTreeMap<String, String>>,
}
impl TypeNames<'_> {
    fn get(&self, name: &str) -> Option<&String> {
        self.parameters
            .and_then(|parameters| parameters.get(name))
            .or_else(|| self.shared.get(name))
    }
    fn values(&self) -> impl Iterator<Item = &String> {
        self.parameters
            .into_iter()
            .flat_map(|parameters| parameters.values())
            .chain(self.shared.values())
    }
}
#[derive(Clone)]
struct AliasScope {
    alias: AliasAst,
    names: NameBindings,
    parameter_names: BTreeMap<String, String>,
    normalized: bool,
}
thread_local! {
    static ALIASES:RefCell<BTreeMap<String,AliasScope>>=const{RefCell::new(BTreeMap::new())};
    static EXPANDING:RefCell<BTreeSet<String>>=const{RefCell::new(BTreeSet::new())};
}
pub(super) fn install_imported_alias(id: String, mut alias: AliasAst) {
    // Older interfaces can contain nullable application through another alias.
    // Normalize that shape once before retaining this resolved definition.
    alias.ty = crate::generics::substitute(&alias.ty, &BTreeMap::new());
    ALIASES.with(|aliases| {
        aliases.borrow_mut().insert(
            id,
            AliasScope {
                alias,
                names: NameBindings::new(),
                parameter_names: BTreeMap::new(),
                normalized: true,
            },
        )
    });
}

/// Reuse equal generic applications only within this resolution request.
/// Cached results are checked and canonical; later requests have fresh contexts.
const MAX_QUERY_INSTANCES: usize = 1024;
#[derive(Default)]
struct TypeResolver {
    instances: BTreeMap<String, Vec<AliasInstance>>,
    instance_count: usize,
}
struct AliasInstance {
    arguments: Vec<Type>,
    ty: Type,
}

pub(super) fn resolve_type(t: &mut Type, names: &NameBindings) -> Result<(), String> {
    TypeResolver::default().resolve(
        t,
        &TypeNames {
            shared: names,
            parameters: None,
        },
    )
}

impl TypeResolver {
    /// Register names first, then resolve each definition's bounds and body on
    /// demand. Forward aliases therefore use the same lexical scope as earlier
    /// aliases, while the expansion guard covers both bounds and bodies.
    fn alias_definition(&mut self, id: &str, mut scope: AliasScope) -> Result<AliasScope, String> {
        if scope.normalized {
            return Ok(scope);
        }
        if !EXPANDING.with(|set| set.borrow_mut().insert(id.into())) {
            return Err("type alias expansion cycle".into());
        }
        scope.parameter_names = bind_generic_parameters(
            &mut scope.alias.generics,
            &scope.names,
            &format!("alias:{}:{}", scope.alias.name, scope.alias.line),
        )?;
        let names = TypeNames {
            shared: &scope.names,
            parameters: Some(&scope.parameter_names),
        };
        for parameter in &mut scope.alias.generics {
            for bound in &mut parameter.bounds {
                self.resolve(bound, &names)?;
            }
            crate::generics::install(parameter);
        }
        crate::generics::validate_bound_cycles(&scope.alias.generics)?;
        self.resolve(&mut scope.alias.ty, &names)?;
        scope.normalized = true;
        ALIASES.with(|aliases| aliases.borrow_mut().insert(id.into(), scope.clone()));
        EXPANDING.with(|set| set.borrow_mut().remove(id));
        Ok(scope)
    }

    fn resolve(&mut self, t: &mut Type, names: &TypeNames<'_>) -> Result<(), String> {
        if let Type::Named { name, args } = t
            && let Some(id) = names.get(name)
            && let Some(id) = id.strip_prefix("$parameter:")
        {
            if !args.is_empty() {
                return Err("type parameter cannot have type arguments".into());
            }
            *t = Type::parameter(id);
            return Ok(());
        }

        match t {
            Type::Named { name, args } => {
                for t in std::sync::Arc::make_mut(args).iter_mut() {
                    self.resolve(t, names)?
                }
                let arity = match name.as_str() {
                    "List" | "Set" | "MutableList" | "MutableSet" => Some(1),
                    "Map" | "MutableMap" | "Entry" => Some(2),
                    "IntRange" | "IntProgression" | "Equatable" | "StableKey" => Some(0),
                    _ => None,
                };
                if let Some(n) = arity {
                    if args.len() != n {
                        return Err(format!("type {name} requires {n} arguments"));
                    }
                } else {
                    // Bare source names need one lookup, not a scan of every
                    // canonical declaration in a large alias scope. Keep the
                    // existing precedence for the enclosing module's own name.
                    let bare = (!name.contains('.') && names.get("$module") != Some(&*name))
                        .then(|| names.get(name.as_str()).cloned())
                        .flatten();
                    if let Some(resolved) = bare {
                        *name = resolved;
                    } else {
                        let mut parts = name.splitn(2, '.');
                        let head = parts.next().unwrap();
                        let tail = parts.next();
                        if !names
                            .values()
                            .any(|root| name == root || name.starts_with(&format!("{root}.")))
                        {
                            let root = names.get(head).ok_or_else(|| format!("unknown type '{head}'"))?;
                            *name = if let Some(tail) = tail {
                                format!("{root}.{tail}")
                            } else {
                                root.clone()
                            };
                        }
                    }

                    if let Some(scope) = ALIASES.with(|a| a.borrow().get(name).cloned()) {
                        if scope.alias.generics.len() != args.len() {
                            return Err("type alias argument count mismatch".into());
                        }
                        let scope = self.alias_definition(name, scope)?;
                        let alias = &scope.alias;
                        let mut expanded = alias.ty.clone();
                        let parameters = alias
                            .generics
                            .iter()
                            .zip(args.iter())
                            .map(|(p, t)| (p.name.clone(), t.clone()))
                            .collect();
                        crate::generics::check(&alias.generics, &parameters)?;
                        if !parameters.is_empty() {
                            if let Some(instance) = self.instances.get(name).and_then(|instances| {
                                instances
                                    .iter()
                                    .find(|instance| instance.arguments.as_slice() == args.as_ref())
                            }) {
                                expanded = instance.ty.clone();
                            } else {
                                expanded = crate::generics::substitute(&expanded, &parameters);
                                if self.instance_count < MAX_QUERY_INSTANCES {
                                    self.instances.entry(name.clone()).or_default().push(AliasInstance {
                                        arguments: args.to_vec(),
                                        ty: expanded.clone(),
                                    });
                                    self.instance_count += 1;
                                }
                            }
                        }
                        *t = expanded;
                    }
                }
            }
            Type::Nullable { inner } => {
                self.resolve(inner, names)?;
                *t = inner.nullable();
            }
            Type::Function { params, result, .. } => {
                for p in Arc::make_mut(params) {
                    self.resolve(p, names)?
                }
                self.resolve(Arc::make_mut(result), names)?
            }
            Type::Scene { params } => {
                for p in Arc::make_mut(params) {
                    self.resolve(p, names)?
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Canonicalize only the new bindings. Callers choose a shared overlay or an
/// ordinary owned scope without changing parameter identities or shadow rules.
fn bind_generic_parameters(
    parameters: &mut [GenericParam],
    parent: &NameBindings,
    prefix: &str,
) -> Result<BTreeMap<String, String>, String> {
    let mut names = BTreeMap::new();
    for parameter in parameters {
        let source = parameter.name.clone();
        if parent.contains_key(&source) || names.contains_key(&source) {
            return Err("generic parameter shadows a visible type name".into());
        }
        parameter.name = format!(
            "{}::{prefix}#{source}",
            parent.get("$module").map_or("", String::as_str)
        );
        names.insert(source, format!("$parameter:{}", parameter.name));
    }
    Ok(names)
}

pub(super) fn resolve_generic_parameters(
    parameters: &mut [GenericParam],
    parent: &NameBindings,
    prefix: &str,
) -> Result<NameBindings, String> {
    if parameters.is_empty() {
        return Ok(parent.clone());
    }
    let mut names = parent.child();
    names.extend(bind_generic_parameters(parameters, parent, prefix)?);
    for p in parameters.iter_mut() {
        for bound in &mut p.bounds {
            resolve_type(bound, &names)?
        }
        crate::generics::install(p);
    }
    crate::generics::validate_bound_cycles(parameters)?;
    Ok(names)
}

pub(super) fn register_type_aliases(aliases: &mut [AliasAst], names: &mut NameBindings) -> Result<(), String> {
    if aliases.is_empty() {
        return Ok(());
    }
    for a in aliases.iter() {
        let id = format!(
            "$alias:{}:{}:{}",
            names.get("$module").map_or("", String::as_str),
            a.name,
            a.line
        );
        if names.insert(a.name.clone(), id).is_some() {
            return Err("alias shadows a visible type".into());
        }
    }
    let shared_names = names.clone();
    for alias in aliases.iter() {
        ALIASES.with(|all| {
            all.borrow_mut().insert(
                names.get(&alias.name).expect("registered alias").clone(),
                AliasScope {
                    alias: alias.clone(),
                    names: shared_names.clone(),
                    parameter_names: BTreeMap::new(),
                    normalized: false,
                },
            )
        });
    }
    for index in order::declarations(aliases, &shared_names)? {
        let alias = &mut aliases[index];
        let id = names.get(&alias.name).expect("registered alias");
        let scope = ALIASES.with(|all| all.borrow().get(id).cloned()).unwrap();
        *alias = TypeResolver::default().alias_definition(id, scope)?.alias;
    }
    Ok(())
}

pub(super) struct AliasSnapshot {
    aliases: BTreeMap<String, AliasScope>,
    expanding: BTreeSet<String>,
}

pub(super) fn take_aliases() -> AliasSnapshot {
    AliasSnapshot {
        aliases: ALIASES.with(|aliases| std::mem::take(&mut *aliases.borrow_mut())),
        expanding: EXPANDING.with(|expanding| std::mem::take(&mut *expanding.borrow_mut())),
    }
}

pub(super) fn restore_aliases(snapshot: AliasSnapshot) {
    ALIASES.with(|aliases| *aliases.borrow_mut() = snapshot.aliases);
    EXPANDING.with(|expanding| *expanding.borrow_mut() = snapshot.expanding);
}
