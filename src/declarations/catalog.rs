//! Resolved nominal metadata and derived capabilities.

use crate::model::{FieldDef, Signature, Type, TypeDef, TypeDefinitions};
use std::collections::BTreeMap;
use std::sync::Arc;

use std::cell::RefCell;
pub(super) type Catalog = TypeDefinitions;

thread_local! {
    // AnalysisSession gives every compilation its own catalog and restores any enclosing session.
    static TYPES:RefCell<Catalog>=const{RefCell::new(BTreeMap::new())};
}

pub(crate) fn definition(name: &str) -> Option<Arc<TypeDef>> {
    TYPES.with(|t| t.borrow().get(name).cloned())
}

pub(crate) fn install(types: Catalog) {
    super::capabilities::invalidate();
    TYPES.with(|catalog| *catalog.borrow_mut() = types)
}

pub(crate) fn parent(ty: &Type) -> Option<Type> {
    if let Type::Named { name, args } = ty {
        let definition = definition(name)?;
        // Parent identity retains hidden arguments. Replacing a star with its
        // read bound would incorrectly make invariant Parent<*> a Parent<Any?>.
        let arguments = definition
            .parameters
            .iter()
            .zip(args.iter())
            .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
            .collect();
        definition
            .parent
            .as_ref()
            .map(|parent| crate::generics::substitute(parent, &arguments))
    } else {
        None
    }
}

pub(crate) fn field(ty: &Type, name: &str) -> Option<FieldDef> {
    let view = crate::generics::member_view(ty);
    let ty = &view;
    if let Type::Named { name: id, args } = ty {
        if id == "Entry" {
            let index = match name {
                "key" => 0,
                "value" => 1,
                _ => return None,
            };
            return Some(FieldDef {
                ty: args.get(index)?.projected_read(),
                mutable: false,
                private: false,
            });
        }
        let d = definition(id)?;
        let mut f = d.fields.get(name)?.clone();
        let hidden = super::projections::hidden_parameters(ty, &d);
        if super::projections::depends_on(&f.ty, &hidden) {
            f.ty = super::projections::read_type(&f.ty, ty, &d);
            f.mutable = false;
        } else {
            f.ty = crate::generics::substitute(&f.ty, &receiver_type_arguments(ty, &d));
        }
        Some(f)
    } else {
        None
    }
}

/// A projected interface may forbid writes while another alias can still
/// reassign the field. Flow proofs need the declaration's actual mutability.
pub(crate) fn field_can_change(ty: &Type, name: &str) -> bool {
    let view = crate::generics::member_view(ty);
    let Type::Named { name: id, .. } = &view else {
        return true;
    };
    if id == "Entry" && matches!(name, "key" | "value") {
        return false;
    }
    definition(id)
        .and_then(|definition| definition.fields.get(name).map(|field| field.mutable))
        .unwrap_or(true)
}

pub(crate) fn methods(ty: &Type, name: &str) -> Vec<Signature> {
    let view = crate::generics::member_view(ty);
    let ty = &view;
    let Type::Named { name: id, .. } = ty else {
        return vec![];
    };
    let Some(definition) = definition(id) else {
        return vec![];
    };
    definition
        .methods
        .get(name)
        .into_iter()
        .flatten()
        .filter_map(|signature| super::projections::method(signature, ty, &definition))
        .collect()
}

pub(crate) fn method(ty: &Type, name: &str) -> Option<Signature> {
    let mut methods = methods(ty, name);
    (methods.len() == 1).then(|| methods.remove(0))
}

pub(crate) fn same_nominal_declaration(left: &Type, right: &Type) -> bool {
    matches!(
        (crate::generics::member_view(left), crate::generics::member_view(right)),
        (Type::Named { name: left, .. }, Type::Named { name: right, .. }) if left == right
    )
}

pub(crate) fn check_method_access(ty: &Type, id: &str, current_receiver: Option<&Type>) -> Result<(), String> {
    let view = crate::generics::member_view(ty);
    let ty = &view;
    if let Type::Named { name, .. } = ty
        && let Some(definition) = definition(name)
        && definition.private_methods.iter().any(|method| method == id)
        && current_receiver.is_none_or(|current| !same_nominal_declaration(ty, current))
    {
        return Err("method is private".into());
    }
    Ok(())
}

pub(crate) fn receiver(id: &str) -> Option<Type> {
    TYPES.with(|t| {
        t.borrow().iter().find_map(|(name, d)| {
            d.methods.values().flatten().any(|s| s.id == id).then(|| {
                Type::named(
                    name,
                    d.parameters.iter().map(|p| Type::parameter(p.name.clone())).collect(),
                )
            })
        })
    })
}

pub(super) fn with_types<R>(query: impl FnOnce(&Catalog) -> R) -> R {
    TYPES.with(|types| query(&types.borrow()))
}

pub(crate) fn all_types() -> Catalog {
    TYPES.with(|t| t.borrow().clone())
}

pub(crate) fn update_method(sig: &Signature) {
    TYPES.with(|t| {
        for definition in t.borrow_mut().values_mut() {
            if definition
                .methods
                .values()
                .flatten()
                .any(|method| method.id == sig.id && method != sig)
            {
                for method in Arc::make_mut(definition).methods.values_mut().flatten() {
                    if method.id == sig.id {
                        *method = sig.clone();
                    }
                }
            }
        }
    })
}

pub(crate) fn receiver_type_arguments(ty: &Type, d: &TypeDef) -> BTreeMap<String, Type> {
    if let Type::Named { args, .. } = ty {
        d.parameters
            .iter()
            .zip(args.iter())
            .map(|(p, t)| {
                (
                    p.name.clone(),
                    if *t == Type::Star {
                        Type::Any.nullable()
                    } else {
                        t.clone()
                    },
                )
            })
            .collect()
    } else {
        BTreeMap::new()
    }
}

pub(super) struct CatalogSnapshot(Catalog);

pub(super) fn take_catalog() -> CatalogSnapshot {
    super::capabilities::invalidate();
    TYPES.with(|types| CatalogSnapshot(std::mem::take(&mut *types.borrow_mut())))
}

pub(super) fn restore_catalog(snapshot: CatalogSnapshot) {
    super::capabilities::invalidate();
    TYPES.with(|types| *types.borrow_mut() = snapshot.0);
}

/// Publish only declarations owned by this source, rather than a copied dependency catalog.
pub(crate) fn owned_types(ast: &crate::syntax::Ast, module: &str) -> TypeDefinitions {
    with_types(|types| {
        let mut owned = BTreeMap::new();
        for declaration in &ast.types {
            let id = format!("{module}.{}", declaration.name);
            if let Some(definition) = types.get(&id) {
                for variant in &definition.variants {
                    if let Some(variant_definition) = types.get(variant) {
                        owned.insert(variant.clone(), Arc::clone(variant_definition));
                    }
                }
                owned.insert(id, Arc::clone(definition));
            }
        }
        owned
    })
}
