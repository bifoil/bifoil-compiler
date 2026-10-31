//! Validate reusable module structure after its canonical digest has been checked.
//! Definitions are owned by their declaring module; dependency catalogs cannot
//! silently replace another module's nominal identity or member interfaces.

pub(crate) mod body;

use crate::model::{Export, FORMAT, MAX_FUNCTIONS, MAX_INSTRUCTIONS, Module, PROFILE, Type, digest};
use crate::module_input::FunctionCode;
use crate::{MAX_MODULES, legacy, module_name};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn verify<C: FunctionCode>(
    module: &mut Module<BTreeMap<String, crate::model::Function<C>>>,
) -> Result<(), String> {
    if module.format != FORMAT || module.profile != PROFILE || !module_name(&module.name) {
        return Err("unsupported module format, profile, or name; recompile the module from source".into());
    }
    if let Some(group) = &module.group
        && (group.members.is_empty()
            || group.members.len() > MAX_MODULES
            || !group.members.contains(&module.name)
            || group.members.windows(2).any(|pair| pair[0] >= pair[1])
            || group.members.iter().any(|name| !module_name(name))
            || group.digest.len() != 64
            || !group.digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err("malformed cyclic module group identity".into());
    }
    let expected = std::mem::take(&mut module.digest);
    let actual = digest(module);
    module.digest = expected;
    if actual? != module.digest {
        return Err(format!("module {}: digest mismatch", module.name));
    }
    if module.functions.len() > MAX_FUNCTIONS
        || module.functions.values().try_fold(0usize, |total, f| {
            total
                .checked_add(f.code.instruction_count()?)
                .ok_or_else(|| "module complexity budget exceeded".to_owned())
        })? > MAX_INSTRUCTIONS
    {
        return Err("module complexity budget exceeded".into());
    }
    // Owned metadata can decide immediately. Imported nominal interfaces are
    // checked again against the complete dependency graph before load succeeds.
    let enum_interface = |name: &str| match module.types.get(name) {
        Some(definition) => legacy::enum_definition(definition),
        // A transparent alias can name an enum owned by a transitive import.
        // Qualified unresolved names stay provisional until graph validation.
        None => name.contains('.'),
    };
    let mut closure_captures = body::ClosureCaptures::new();
    for (id, f) in &module.functions {
        if *id != f.signature.id || !id.starts_with(&format!("{}.", module.name)) || f.code.instruction_count()? == 0 {
            return Err("malformed function identity or body".into());
        }
        let mut parameter_names = BTreeSet::new();
        for (index, parameter) in f.signature.params.iter().enumerate() {
            if parameter.name.is_empty()
                || !parameter_names.insert(parameter.name.as_str())
                || (parameter.vararg && (index + 1 != f.signature.params.len() || parameter.default))
            {
                return Err(format!("invalid function parameter interface for {id}"));
            }
        }
        f.code.structural_facts().verify(
            &f.signature,
            |target| module.functions.contains_key(target),
            &mut closure_captures,
            &enum_interface,
        )?;
    }
    validate_nominals(module)?;
    for export in module.exports.values() {
        let signatures = match export {
            Export::Function { signature } => std::slice::from_ref(signature),
            Export::Functions { signatures } => signatures.as_slice(),
            Export::Type { id, constructor } => {
                if !module.types.contains_key(id) {
                    return Err("exported nominal type is not owned by its module".into());
                }
                constructor.as_slice()
            }
            Export::Global { id, .. } => {
                if !id.starts_with(&format!("{}.", module.name)) {
                    return Err("exported binding is not owned by its module".into());
                }
                &[]
            }
            _ => &[],
        };
        for signature in signatures {
            if module
                .functions
                .get(&signature.id)
                .is_none_or(|function| function.signature != *signature)
            {
                return Err("exported function signature does not match its body".into());
            }
        }
    }
    for (id, definition) in &module.legacy_globals {
        if !id.starts_with(&format!("{}.", module.name))
            || !legacy::value_type_with(&definition.ty, false, &enum_interface)
            || !legacy::variable(&definition.target)
        {
            return Err("invalid legacy state interface".into());
        }
    }
    for (id, definition) in &module.shared {
        if !id.starts_with(&format!("{}.", module.name))
            || module.functions.contains_key(id)
            || module.legacy_globals.contains_key(id)
        {
            return Err("invalid shared binding identity".into());
        }
        if module.functions.get(&definition.initializer).is_none_or(|function| {
            !function.signature.params.is_empty()
                || function.signature.suspend
                || function.signature.scene
                || function.signature.result != definition.ty
        }) {
            return Err("invalid shared initializer signature".into());
        }
    }
    if module
        .init
        .as_ref()
        .is_some_and(|s| module.functions.get(s).is_none_or(|f| !f.init))
    {
        return Err("invalid module initializer".into());
    }
    crate::signature_sharing::module(module);
    Ok(())
}
fn validate_nominals<C>(module: &Module<BTreeMap<String, crate::model::Function<C>>>) -> Result<(), String> {
    let prefix = format!("{}.", module.name);
    for (id, definition) in &module.types {
        if !id.starts_with(&prefix) {
            return Err(format!(
                "type {id} is not owned by module {}; recompile its artifact",
                module.name
            ));
        }
        let ordered = definition.field_order.iter().collect::<std::collections::BTreeSet<_>>();
        if ordered.len() != definition.field_order.len()
            || ordered.len() != definition.fields.len()
            || ordered.iter().any(|name| !definition.fields.contains_key(*name))
        {
            return Err(format!("invalid nominal field order for {id}"));
        }
        for signature in definition.methods.values().flatten() {
            if module
                .functions
                .get(&signature.id)
                .is_none_or(|function| function.signature != *signature)
            {
                return Err(format!(
                    "nominal method signature does not match its body: {}",
                    signature.id
                ));
            }
        }
        match (&definition.parent, definition.kind.as_str()) {
            (Some(Type::Named { name, .. }), "variant") => {
                if module
                    .types
                    .get(name)
                    .is_none_or(|parent| parent.kind != "sum" || !parent.variants.contains(id))
                {
                    return Err(format!("invalid variant parent for {id}"));
                }
            }
            (None, "data" | "enum" | "sum") => {}
            _ => return Err(format!("invalid nominal kind or parent for {id}")),
        }
        let variant_prefix = format!("{id}.");
        let mut variants = BTreeSet::new();
        for variant in &definition.variants {
            if !variants.insert(variant)
                || variant.strip_prefix(&variant_prefix).is_none_or(str::is_empty)
                || (definition.kind != "enum"
                    && module
                        .types
                        .get(variant)
                        .is_none_or(|child| !matches!(&child.parent, Some(Type::Named { name, .. }) if name == id)))
            {
                return Err(format!("invalid declared variant for {id}"));
            }
        }
    }
    Ok(())
}
