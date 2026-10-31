//! Provisional declarations for mutually importing source modules.
//! Placeholders are private to the loader; complete bodies are checked before serialization.

use super::{Prepared, export_function, export_types, function_ids, lookup, prepare, signature};
use crate::model::{Export, FORMAT, Function, Module, PROFILE, Type};
use crate::syntax::{Ast, StatementKind};
use std::collections::BTreeMap;
use std::time::Instant;

/// Publish resolved declarations before any instruction bodies are lowered.
/// Pending returns remain internal to the fixed-point pass and are never serialized.
pub(crate) fn interface(
    ast: &Ast,
    name: &str,
    file: &str,
    deps: &crate::model::ModuleView<'_>,
    started: Instant,
) -> Result<Module, String> {
    let _analysis_session = crate::declarations::AnalysisSession::enter();
    let Prepared {
        ast,
        env,
        function_ids,
        legacy_globals,
        mut exports,
        dependencies,
    } = prepare(ast.clone(), name, file, deps, started)?;
    let mut functions = BTreeMap::new();
    for (index, fun) in ast.functions.iter().enumerate() {
        let sig = lookup(&env, &fun.name).unwrap().signature_by_id(&function_ids[index]);
        if fun.exported {
            export_function(&mut exports, &fun.name, sig.clone())?;
        }
        functions.insert(
            sig.id.clone(),
            Function {
                signature: sig,
                code: vec![],
                init: false,
                local: false,
            },
        );
    }
    for global in &ast.globals {
        if global.exported {
            let StatementKind::Variable { name, .. } = &global.stmt.kind else {
                unreachable!()
            };
            let binding = lookup(&env, name).unwrap();
            exports.insert(
                name.clone(),
                Export::Global {
                    id: binding.runtime,
                    ty: binding.ty,
                    mutable: binding.mutable,
                },
            );
        }
    }
    export_types(&ast, name, &functions, &mut exports);
    Ok(Module {
        summaries: BTreeMap::new(),
        group: None,
        legacy_globals,
        shared: BTreeMap::new(),
        types: crate::declarations::owned_types(&ast, name),
        format: FORMAT.into(),
        profile: PROFILE.into(),
        name: name.into(),
        digest: String::new(),
        dependencies,
        exports,
        functions,
        init: None,
    })
}

/// Only names and directly written signatures are needed to start interface resolution.
/// All types are normalized and all bodies checked before these placeholders can escape.
pub(crate) fn seed(ast: &Ast, name: &str) -> Result<Module, String> {
    let mut exports = BTreeMap::new();
    let mut functions = BTreeMap::new();
    let identities = function_ids(ast, name);
    for (index, fun) in ast.functions.iter().enumerate() {
        let id = identities[index].clone();
        let sig = signature(fun, id);
        if fun.exported {
            export_function(&mut exports, &fun.name, sig.clone())?;
        }
        functions.insert(
            sig.id.clone(),
            Function {
                signature: sig,
                code: vec![],
                init: false,
                local: false,
            },
        );
    }
    for global in &ast.globals {
        if global.exported {
            let StatementKind::Variable {
                name: local,
                ty,
                mutable,
                ..
            } = &global.stmt.kind
            else {
                unreachable!()
            };
            exports.insert(
                local.clone(),
                Export::Global {
                    id: format!("{name}.{local}"),
                    ty: ty.clone().unwrap_or(Type::Pending),
                    mutable: *mutable,
                },
            );
        }
    }
    let mut legacy_globals = BTreeMap::new();
    for global in &ast.legacy_globals {
        let id = format!("{name}.{}", global.name);
        legacy_globals.insert(
            id.clone(),
            crate::model::LegacyGlobal {
                ty: global.ty.clone(),
                mutable: global.mutable,
                target: global.target.clone(),
            },
        );
        if global.exported {
            exports.insert(
                global.name.clone(),
                Export::Global {
                    id,
                    ty: global.ty.clone(),
                    mutable: global.mutable,
                },
            );
        }
    }
    export_types(ast, name, &functions, &mut exports);
    Ok(Module {
        summaries: BTreeMap::new(),
        group: None,
        legacy_globals,
        shared: BTreeMap::new(),
        types: BTreeMap::new(),
        format: FORMAT.into(),
        profile: PROFILE.into(),
        name: name.into(),
        digest: String::new(),
        dependencies: ast
            .imports
            .iter()
            .map(|import| (import.module.clone(), String::new()))
            .collect(),
        exports,
        functions,
        init: None,
    })
}
/// Resolve nominal names, generated signatures, aliases and generic parameter IDs
/// before lexical insertion attempts to validate imported declaration types.
pub(crate) fn declaration_interface(
    ast: &Ast,
    name: &str,
    deps: &crate::model::ModuleView<'_>,
) -> Result<Module, String> {
    let _analysis_session = crate::declarations::AnalysisSession::enter();
    let (prepared, _) = crate::declarations::prepare(ast.clone(), name, deps)?;
    let mut module = seed(&prepared, name)?;
    module.types = crate::declarations::owned_types(&prepared, name);
    Ok(module)
}
