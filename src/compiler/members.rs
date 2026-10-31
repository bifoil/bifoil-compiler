//! Primitive and nominal member lookup, accessibility, and callable interfaces.

use super::{Binding, ScopeStack, lookup, numeric};
use crate::model::{Param, Signature, SignatureData, Type};
use crate::syntax::Expr;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub(super) enum BuiltinHeapEffect {
    ReadOnly,
    Containers,
    Unknown,
}

/// New builtins conservatively invalidate heap facts until their effects are
/// classified. Collection mutators cannot reassign independent object fields.
pub(super) fn builtin_heap_effect(id: &str) -> BuiltinHeapEffect {
    match id {
        "$builtin.Int"
        | "$builtin.Float"
        | "$builtin.Char"
        | "$builtin.String"
        | "$builtin.toInt"
        | "$builtin.toFloat"
        | "$builtin.toString"
        | "$builtin.substring"
        | "$builtin.isEmpty"
        | "$builtin.contains"
        | "$builtin.containsKey"
        | "$builtin.toList"
        | "$builtin.toMutableList"
        | "$builtin.toSet"
        | "$builtin.toMutableSet"
        | "$builtin.toMap"
        | "$builtin.toMutableMap" => BuiltinHeapEffect::ReadOnly,
        "$builtin.clear" | "$builtin.remove" | "$builtin.removeAt" | "$builtin.insert" | "$builtin.add"
        | "$builtin.put" => BuiltinHeapEffect::Containers,
        _ => BuiltinHeapEffect::Unknown,
    }
}

pub(super) fn builtin_signature(receiver: &Type, name: &str) -> Option<Signature> {
    let view = crate::generics::member_view(receiver);
    let receiver = &view;
    let primitive = |params: Vec<(&str, Type)>, result: Type| {
        Arc::new(SignatureData {
            generics: vec![],
            id: format!("$builtin.{name}"),
            params: params
                .into_iter()
                .map(|(n, ty)| Param {
                    name: n.into(),
                    ty,
                    mutable: false,
                    default: false,
                    vararg: false,
                })
                .collect(),
            result,
            suspend: false,
            scene: false,
        })
    };
    if name == "toString" {
        return Some(primitive(vec![], Type::String));
    }
    if numeric(receiver) && name == "toInt" {
        return Some(primitive(vec![], Type::Int));
    }
    if numeric(receiver) && name == "toFloat" {
        return Some(primitive(vec![], Type::Float));
    }
    if *receiver == Type::String && name == "substring" {
        return Some(primitive(
            vec![("start", Type::Int), ("endExclusive", Type::Int)],
            Type::String,
        ));
    }
    if (*receiver == Type::String || receiver.collection().is_some()) && name == "isEmpty" {
        return Some(primitive(vec![], Type::Boolean));
    }
    if let Some((family, args, mutable)) = receiver.collection() {
        if (name == "contains" || name == "containsKey") && args[0].equatable() {
            return Some(primitive(vec![("element", args[0].clone())], Type::Boolean));
        }
        if [
            "toList",
            "toMutableList",
            "toSet",
            "toMutableSet",
            "toMap",
            "toMutableMap",
        ]
        .contains(&name)
        {
            let target = name.strip_prefix("to").unwrap();
            if target.ends_with(family) {
                return Some(primitive(vec![], Type::named(target, args.to_vec())));
            }
        }
        if mutable {
            if name == "clear" {
                return Some(primitive(vec![], Type::Unit));
            }
            if name == "remove" && args[0].equatable() {
                return Some(primitive(
                    vec![(if family == "Map" { "key" } else { "element" }, args[0].clone())],
                    if family == "Map" {
                        args[1].projected_read().nullable()
                    } else {
                        Type::Boolean
                    },
                ));
            }
            if family != "Map" && name == "add" {
                return Some(primitive(vec![("element", args[0].clone())], Type::Boolean));
            }
            if family == "List" && name == "insert" {
                return Some(primitive(
                    vec![("index", Type::Int), ("element", args[0].clone())],
                    Type::Unit,
                ));
            }
            if family == "List" && name == "removeAt" {
                return Some(primitive(vec![("index", Type::Int)], args[0].projected_read()));
            }
            if family == "Map" && name == "put" {
                return Some(primitive(
                    vec![("key", args[0].clone()), ("value", args[1].clone())],
                    args[1].projected_read().nullable(),
                ));
            }
        }
    }
    None
}

/// A field may itself contain a function; only methods have a resolved target.
pub(super) fn resolved_member_target(receiver: &Type, name: &str) -> Option<String> {
    if crate::declarations::field(receiver, name).is_some() {
        None
    } else {
        crate::declarations::method(receiver, name)
            .or_else(|| builtin_signature(receiver, name))
            .map(|signature| signature.id.clone())
    }
}

pub(super) fn member_type(receiver: &Type, name: &str) -> Result<Type, String> {
    let view = crate::generics::member_view(receiver);
    let receiver = &view;
    if let Some(f) = crate::declarations::field(receiver, name) {
        return Ok(f.ty);
    }
    if let Some(m) = crate::declarations::method(receiver, name) {
        return Ok(m.ty());
    }
    if *receiver == Type::String && name == "length" {
        return Ok(Type::Int);
    }
    if *receiver == Type::Char && name == "code" {
        return Ok(Type::Int);
    }
    if receiver.collection().is_some() && name == "size" {
        return Ok(Type::Int);
    }
    if let Type::Named { name: ty, .. } = receiver
        && ["IntRange", "IntProgression"].contains(&ty.as_str())
        && ["first", "last", "step"].contains(&name)
    {
        return Ok(Type::Int);
    }
    builtin_signature(receiver, name)
        .map(|s| s.ty())
        .ok_or_else(|| format!("type {receiver} has no accessible member '{name}'"))
}

pub(super) fn index_types(receiver: &Type) -> Result<(Type, Type), String> {
    let view = crate::generics::member_view(receiver);
    let receiver = &view;
    if *receiver == Type::String {
        return Ok((Type::Int, Type::Char));
    }
    if let Some((family, args, _)) = receiver.collection() {
        return match family {
            "List" => Ok((
                Type::Int,
                if args[0] == Type::Star {
                    Type::Any.nullable()
                } else {
                    args[0].clone()
                },
            )),
            "Map" => Ok((
                args[0].clone(),
                if args[1] == Type::Star {
                    Type::Any.nullable()
                } else {
                    args[1].projected_read().nullable()
                },
            )),
            _ => Err("Set cannot be indexed".into()),
        };
    }
    Err(format!("type {receiver} cannot be indexed"))
}

pub(super) fn expression_path(e: &Expr) -> Option<String> {
    match e {
        Expr::Name(n) | Expr::Reference(n) => Some(n.clone()),
        Expr::Member {
            receiver,
            name,
            safe: false,
        } => Some(format!("{}.{}", expression_path(receiver)?, name)),
        _ => None,
    }
}

fn constructor_has_member(receiver: &Expr, name: &str, env: &ScopeStack) -> bool {
    let receiver = super::overloads::callable_base(receiver);
    // An explicit constructor reference is an ordinary function value.
    if matches!(receiver, Expr::Reference(_)) {
        return false;
    }
    let Some(symbol) = expression_path(receiver).and_then(|path| lookup(env, &path)) else {
        return false;
    };
    symbol.callables.iter().any(|signature| {
        let Type::Named { name: id, .. } = &signature.result else {
            return false;
        };
        signature.id == *id
            && crate::declarations::definition(id).is_some_and(|definition| {
                definition.kind == "data"
                    && (definition.fields.contains_key(name) || definition.methods.contains_key(name))
            })
    })
}

pub(super) fn check_static_receiver(expression: &Expr, env: &ScopeStack) -> Result<(), String> {
    let base = super::overloads::callable_base(expression);
    if let Expr::Member { receiver, name, .. } | Expr::BoundReference(receiver, name) = base
        && constructor_has_member(receiver, name, env)
    {
        return Err(format!("instance member '{name}' requires a receiver value"));
    }
    let path = if let Expr::BoundReference(receiver, name) = base {
        expression_path(receiver).map(|receiver| format!("{receiver}.{name}"))
    } else {
        expression_path(base)
    };
    if let Some(path) = path
        && let Some(symbol) = lookup(env, &path)
        && symbol
            .callables
            .iter()
            .any(|signature| crate::declarations::receiver(&signature.id).is_some())
    {
        return Err(format!("instance member '{path}' requires a receiver value"));
    }
    Ok(())
}

pub(super) fn static_symbol(e: &Expr, env: &ScopeStack) -> Option<Binding> {
    if matches!(e, Expr::Member { .. }) {
        let symbol = lookup(env, &expression_path(e)?)?;
        // Instance methods are not static members of their declaring type.
        if symbol
            .callables
            .iter()
            .any(|signature| crate::declarations::receiver(&signature.id).is_some())
        {
            return None;
        }
        Some(symbol)
    } else {
        None
    }
}

pub(super) fn constant_case(s: &Binding) -> bool {
    let Some(sig) = &s.signature() else {
        return false;
    };
    if !sig.params.is_empty() {
        return false;
    }
    if let Type::Named { name, .. } = &sig.result {
        crate::declarations::definition(name).is_some_and(|definition| {
            (definition.kind == "enum" && definition.variants.contains(&sig.id))
                || (definition.kind == "variant" && definition.fields.is_empty() && sig.id == *name)
        })
    } else {
        false
    }
}

pub(super) fn enum_shorthand(name: &str, env: &ScopeStack, expected: Option<&Type>) -> Result<Option<Binding>, String> {
    let view = expected.map(|ty| crate::generics::member_view(ty.present()));
    let Some(Type::Named { name: id, .. }) = view.as_ref() else {
        return Ok(None);
    };
    let Some(def) = crate::declarations::definition(id) else {
        return Ok(None);
    };
    if def.kind != "enum" || !def.variants.iter().any(|v| v.rsplit('.').next() == Some(name)) {
        return Ok(None);
    }
    if lookup(env, name).is_some() {
        return Err(format!(
            "enum case '{name}' conflicts with a visible binding; qualify the case"
        ));
    }
    let id = format!("{id}.{name}");
    for scope in env {
        if let Some(s) = scope.values().find(|s| s.runtime == id) {
            return Ok(Some(s.clone()));
        }
    }
    Err("enum case is not imported".into())
}

pub(super) fn access_member(receiver: &Type, name: &str, env: &ScopeStack) -> Result<(), String> {
    let private = crate::declarations::field(receiver, name).is_some_and(|f| f.private)
        || if let Type::Named { name: id, .. } = receiver {
            crate::declarations::definition(id).is_some_and(|d| d.private_methods.iter().any(|n| n == name))
        } else {
            false
        };
    if private
        && super::bindings::current_receiver(env)
            .is_none_or(|current| !crate::declarations::same_nominal_declaration(receiver, current))
    {
        return Err(format!("member '{name}' is private"));
    }
    Ok(())
}
