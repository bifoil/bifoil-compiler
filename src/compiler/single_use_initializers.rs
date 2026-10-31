//! Prove local initializers at direct source calls.
//! Mutable targets use optional write facts at each direct call; protected vals
//! require at most one executed invocation and a complete proof.
//!
//! A protected val and its direct source calls must share one complete lexical
//! body. Multiple sites retain frames; ordinary flow checks prove each call's
//! targets unwritten and prevent repeated writes. Every normal return must
//! initialize the protected vals.
//! Its original declaration remains type-checked and reserves its source name.
//! Context-independent scalar helpers may expand through caller checks.
//! Discarded calls preserve their instruction shape; consumed Unit calls leave one
//! value for their consumer. Other helpers retain frames and captured-write facts.

use super::{Compiler, FunctionContext, ScopeStack, local_function_id, lookup};
use crate::syntax::{Expr, FunctionAst, Statement, StatementKind};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

mod call_sites;
mod compact;
mod forward;

pub(super) struct Plan {
    creator: String,
    protected: BTreeSet<String>,
    expansion: Option<Arc<[Statement]>>,
    called_before_definition: bool,
    forward: Option<forward::Frame>,
}
impl Plan {
    pub(super) fn expands(&self) -> bool {
        self.expansion.is_some()
    }

    /// Immutable must-write facts are mandatory for a live helper. Falling back
    /// to no fact would allow a later second assignment to an already written val.
    pub(super) fn verify_writes<'a>(&self, writes: impl Iterator<Item = &'a str>) -> Result<(), String> {
        if !self.expands() {
            let writes = writes.collect::<BTreeSet<_>>();
            if !self.protected.iter().all(|name| writes.contains(name.as_str())) {
                return Err("cannot prove every captured val initialized by the single-use helper".into());
            }
        }
        Ok(())
    }

    fn check_uninitialized(&self, context: &FunctionContext) -> Result<(), String> {
        if self.creator != context.function_id {
            return Err("single-use initializer requires its declaring function".into());
        }
        for name in &self.protected {
            let binding = lookup(&context.scopes, name).ok_or_else(|| format!("unknown binding '{name}'"))?;
            if context.is_reachable && binding.may_be_initialized {
                return Err(format!(
                    "cannot initialize captured val '{name}' after a possible earlier initialization"
                ));
            }
        }
        Ok(())
    }

    /// Check a forward helper against fresh protected cells in a detached
    /// definition environment, without resetting actual caller initialization.
    pub(super) fn prepare_definition(&self, env: &mut ScopeStack) -> Result<(), String> {
        if !self.called_before_definition {
            return Ok(());
        }
        for name in &self.protected {
            let binding = super::bindings::symbol_mut(env, name)
                .ok_or_else(|| format!("missing captured val '{name}' at its initializer definition"))?;
            if binding.mutable || binding.owner != self.creator {
                return Err(format!(
                    "captured val '{name}' changed ownership or mutability before its initializer definition"
                ));
            }
            binding.initialized = false;
            binding.may_be_initialized = false;
            binding.narrow = None;
            binding.field_narrows = None;
        }
        Ok(())
    }

    pub(super) fn verify_definition(&mut self, function: &crate::model::Function) -> Result<(), String> {
        if let Some(forward) = &mut self.forward {
            forward.verify(function)?;
        }
        Ok(())
    }

    pub(super) fn permits(&self, name: &str, owner: &str) -> bool {
        self.creator == owner && self.protected.contains(name)
    }
}

struct DelayedDeclaration {
    index: usize,
    mutable: bool,
}

struct UseSite {
    statement: usize,
    compact: bool,
    loop_condition: bool,
}

struct Candidate<'a> {
    declaration: usize,
    function: &'a FunctionAst,
    targets: BTreeSet<String>,
    protected: BTreeSet<String>,
    compact: bool,
}

pub(super) fn discover(
    body: &[Statement],
    owner: &str,
    tail: Option<&Expr>,
    entry: &ScopeStack,
) -> BTreeMap<String, Plan> {
    if !body
        .iter()
        .any(|statement| matches!(statement.kind, StatementKind::Function(_)))
    {
        return BTreeMap::new();
    }
    let delayed = body
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| match &statement.kind {
            StatementKind::Uninitialized { name, mutable, .. } => Some((
                name.as_str(),
                DelayedDeclaration {
                    index,
                    mutable: *mutable,
                },
            )),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    if delayed.is_empty() {
        return BTreeMap::new();
    }
    let mut declarations = BTreeMap::<&str, usize>::new();
    for statement in body {
        if let StatementKind::Function(function) = &statement.kind {
            *declarations.entry(&function.name).or_default() += 1;
        }
    }
    let mut candidates = Vec::new();
    for (index, statement) in body.iter().enumerate() {
        let StatementKind::Function(function) = &statement.kind else {
            continue;
        };
        if !supported(function) {
            continue;
        }
        let simple = simple_assignments(function);
        let writes = if simple {
            function
                .body
                .iter()
                .filter_map(|statement| match &statement.kind {
                    StatementKind::Assign { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .collect()
        } else {
            super::captures::assigned_names(function)
        };
        let targets = writes
            .into_iter()
            .filter(|name| {
                delayed
                    .get(name.as_str())
                    .is_some_and(|declaration| declaration.index < index)
            })
            .collect::<BTreeSet<_>>();
        if !targets.is_empty() {
            let protected = targets
                .iter()
                .filter(|name| !delayed[name.as_str()].mutable)
                .cloned()
                .collect::<BTreeSet<_>>();
            let expand = !protected.is_empty()
                && simple
                && function.params.is_empty()
                && function.generics.is_empty()
                && !function.suspend;
            candidates.push(Candidate {
                declaration: index,
                function,
                targets,
                protected,
                compact: expand,
            });
        }
    }
    if candidates.is_empty() {
        return BTreeMap::new();
    }
    let candidate_names = candidates
        .iter()
        .map(|candidate| candidate.function.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut uses = BTreeMap::<&str, Vec<UseSite>>::new();
    // A post-test loop keeps this body's bindings alive in its condition.
    // That condition must not expose or call an otherwise single-use helper.
    let mut referenced = tail.map(super::captures::expression_names).unwrap_or_default();
    for (index, statement) in body.iter().enumerate() {
        let inventory = call_sites::Inventory::statement(statement);
        for call in inventory.calls {
            if candidate_names.contains(call.name) {
                uses.entry(call.name).or_default().push(UseSite {
                    statement: index,
                    compact: call.compact,
                    loop_condition: call.loop_condition,
                });
            }
        }
        referenced.extend(inventory.referenced);
    }
    let compact_bindings = compact::Bindings::new(body, entry, &declarations);
    let mut retained_entry = None;
    candidates
        .into_iter()
        .filter_map(
            |Candidate {
                 declaration,
                 function,
                 targets,
                 protected,
                 compact: expand,
             }| {
                let name = function.name.as_str();
                let calls = uses.get(name)?;
                if calls.is_empty()
                    || (!protected.is_empty() && calls.iter().any(|site| site.loop_condition))
                    || referenced.contains(name)
                {
                    return None;
                }
                let call = calls[0].statement;
                let compact = calls[0].compact;
                // Several exclusive sites share one checked body and use ordinary
                // call preconditions independently in each branch environment.
                let expands = calls.len() == 1
                    && declarations[name] == 1
                    && expand
                    && compact
                    && compact_bindings.permits(function, declaration.min(call));
                let forward = call < declaration;
                // Mutable helpers already receive ordinary write facts after their
                // definition. Only their forward call needs an early body check.
                if (protected.is_empty() && !forward)
                    || (forward && targets.iter().any(|name| delayed[name.as_str()].index >= call))
                {
                    return None;
                }
                Some((
                    local_function_id(owner, function),
                    Plan {
                        creator: owner.into(),
                        protected,
                        // Parameterized, generic, and effectful helpers keep their
                        // real frame and ordinary specialization checks.
                        expansion: expands.then(|| function.body.clone().into()),
                        called_before_definition: forward,
                        forward: (forward && !expands).then(|| {
                            forward::Frame::new(
                                function,
                                Arc::clone(retained_entry.get_or_insert_with(|| Arc::new(entry.clone()))),
                            )
                        }),
                    },
                ))
            },
        )
        .collect()
}

type Arguments = [(Option<String>, Expr)];

/// A discarded plain-name call may have grouping around the call or its callee.
/// Statement lowering avoids adding a discarded Unit value. Expression lowering
/// uses the same plain-call shape and supplies that value to its consumer.
fn compact_call(statement: &Statement) -> Option<(&str, &Arguments)> {
    let StatementKind::Expression(value) = &statement.kind else {
        return None;
    };
    let Expr::Call(callee, arguments) = super::overloads::ungroup(value) else {
        return None;
    };
    Some((plain_call_name(callee, arguments)?, arguments))
}

/// Shared by source inventory and both compact lowering paths. These calls
/// evaluate no arguments, references, receivers, or generic adapters.
pub(super) fn plain_call_name<'a>(callee: &'a Expr, arguments: &Arguments) -> Option<&'a str> {
    if !arguments.is_empty() {
        return None;
    }
    match super::overloads::ungroup(callee) {
        Expr::Name(name) => Some(name),
        _ => None,
    }
}

fn supported(function: &FunctionAst) -> bool {
    if function.scene || function.exported || function.legacy.is_some() || function.body.is_empty() {
        return false;
    }
    true
}

/// Preserve the compact expansion for straight, effect-free parameterless code.
/// Other eligible bodies retain their normal frame and instruction proof.
fn simple_assignments(function: &FunctionAst) -> bool {
    let mut targets = BTreeSet::new();
    function.body.iter().all(|statement| {
        matches!(&statement.kind, StatementKind::Assign { name, operator, value }
            if operator == "=" && targets.insert(name) && super::flow::pure_expression(value))
    })
}

impl Compiler {
    /// Supplied arguments have already run. Defaults and the body retain ordinary
    /// immutable-flow checks; the completed proof covers every normal return.
    pub(super) fn check_initializer_call(&self, id: &str, context: &FunctionContext) -> Result<(), String> {
        if let Some(plan) = self.single_use_initializers.get(id)
            && !plan.expands()
            && !plan.protected.is_empty()
        {
            plan.check_uninitialized(context)?;
            if !self.initialized_captures.contains_key(id) {
                return Err("missing captured-val write proof for single-use helper".into());
            }
        }
        Ok(())
    }

    pub(super) fn expand_initializer(
        &mut self,
        statement: &Statement,
        context: &mut FunctionContext,
    ) -> Result<bool, String> {
        let Some((name, arguments)) = compact_call(statement) else {
            return Ok(false);
        };
        if !arguments.is_empty() {
            return Ok(false);
        }
        let Some(signature) = lookup(&context.scopes, name).and_then(|binding| binding.signature()) else {
            return Ok(false);
        };
        self.expand_initializer_body(&signature.id, context)
    }

    /// A Unit result carries no captured payload. Keep surrounding operands
    /// intact, then leave exactly one Unit value for the expression's consumer.
    pub(super) fn expand_initializer_value(
        &mut self,
        id: &str,
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<bool, String> {
        if !self.expand_initializer_body(id, context)? {
            return Ok(false);
        }
        context.emit(crate::model::unit(), line);
        context.value_requirements = Default::default();
        Ok(true)
    }

    fn expand_initializer_body(&mut self, id: &str, context: &mut FunctionContext) -> Result<bool, String> {
        let Some(plan) = self.single_use_initializers.get(id) else {
            return Ok(false);
        };
        let Some(body) = &plan.expansion else {
            return Ok(false);
        };
        plan.check_uninitialized(context)?;
        let body = Arc::clone(body);
        for statement in body.iter() {
            let StatementKind::Assign { name, operator, value } = &statement.kind else {
                unreachable!()
            };
            self.assign(name, operator, value, context, statement.line)?;
        }
        Ok(true)
    }
}
