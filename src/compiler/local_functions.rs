//! Resolve local callable result types without emitting instructions.
//!
//! A round visits variable declarations in source order, so later bindings do not
//! enter an earlier function's lexical scope. Callers repeat only while signatures
//! change and retain their own resource/deadline checks.

mod definition;

use super::{Binding, ScopeStack, infer, infer_returns, insert, local_function_id, lookup, pending_type, symbol_mut};
use crate::model::Type;
use crate::syntax::{Statement, StatementKind};

pub(super) fn refine(body: &[Statement], scopes: &mut ScopeStack, owner: &str) -> Result<bool, String> {
    if !body
        .iter()
        .any(|stmt| matches!(&stmt.kind, StatementKind::Function(fun) if fun.result.is_none()))
    {
        return Ok(false);
    }
    let mut types = scopes.clone();
    let mut changed = false;
    for stmt in body {
        match &stmt.kind {
            StatementKind::Variable {
                name,
                ty,
                value,
                mutable,
            } => {
                let actual = ty
                    .clone()
                    .or_else(|| infer(value, &types).ok())
                    .unwrap_or(Type::Pending);
                let _ = insert(
                    &mut types,
                    name,
                    Binding {
                        identity: crate::compiler::bindings::Identity::new(),
                        index_key: false,
                        global_aliases: None,
                        volatile: false,
                        implicit_receiver: false,
                        escaped: false,
                        runtime: name.clone(),
                        ty: actual,
                        mutable: *mutable,
                        initialized: true,
                        may_be_initialized: true,
                        owner: owner.to_owned(),
                        narrow: None,
                        field_narrows: None,
                        callables: vec![],
                        requires: Default::default(),
                        callable_requirements: Default::default(),
                    },
                );
            }
            StatementKind::Uninitialized { name, ty, mutable } => {
                let _ = insert(
                    &mut types,
                    name,
                    Binding {
                        identity: crate::compiler::bindings::Identity::new(),
                        index_key: false,
                        global_aliases: None,
                        volatile: false,
                        implicit_receiver: false,
                        escaped: false,
                        runtime: name.clone(),
                        ty: ty.clone(),
                        mutable: *mutable,
                        initialized: false,
                        may_be_initialized: false,
                        owner: owner.to_owned(),
                        narrow: None,
                        field_narrows: None,
                        callables: vec![],
                        requires: Default::default(),
                        callable_requirements: Default::default(),
                    },
                );
            }
            StatementKind::Function(f) if f.result.is_none() => {
                let mut sig = lookup(&types, &f.name)
                    .unwrap()
                    .signature_by_id(&local_function_id(owner, f));
                if let Ok(result) = infer_returns(f, &types)
                    && !pending_type(&result)
                    && sig.result != result
                {
                    std::sync::Arc::make_mut(&mut sig).result = result;
                    crate::declarations::update_method(&sig);
                    if !sig.ty().within_depth(32) {
                        return Err("type graph budget exceeded".into());
                    }
                    for env in [&mut types, &mut *scopes] {
                        let s = symbol_mut(env, &f.name).unwrap();
                        s.ty = sig.ty();
                        s.update_signature(sig.clone());
                    }
                    changed = true;
                }
            }
            _ => {}
        }
    }
    Ok(changed)
}
