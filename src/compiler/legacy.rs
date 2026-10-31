//! Install declared legacy state interfaces and lower native subroutine wrappers.

use super::{Binding, ScopeStack, insert};
use crate::model::{Export, LegacyGlobal};
use crate::syntax::Ast;
use std::collections::BTreeMap;

pub(super) fn register(
    ast: &Ast,
    module: &str,
    file: &str,
    env: &mut ScopeStack,
    exports: &mut BTreeMap<String, Export>,
) -> Result<BTreeMap<String, LegacyGlobal>, String> {
    let mut definitions = BTreeMap::new();
    for declaration in &ast.legacy_globals {
        crate::declarations::validate_type(&declaration.ty)?;
        if !crate::legacy::value_type(&declaration.ty, false) || !crate::legacy::variable(&declaration.target) {
            return Err(format!(
                "{}:{}: legacy binding '{}' requires a primitive type, a plain enum, or nullable forms and a valid ChoiceScript variable",
                file, declaration.line, declaration.name
            ));
        }
        let id = format!("{module}.{}", declaration.name);
        insert(
            env,
            &declaration.name,
            Binding {
                identity: crate::compiler::bindings::Identity::new(),
                index_key: false,
                global_aliases: None,
                volatile: true,
                implicit_receiver: false,
                escaped: false,
                runtime: id.clone(),
                ty: declaration.ty.clone(),
                mutable: declaration.mutable,
                initialized: true,
                may_be_initialized: true,
                owner: String::new(),
                narrow: None,
                field_narrows: None,
                callables: vec![],
                requires: Default::default(),
                callable_requirements: Default::default(),
            },
        )?;
        definitions.insert(
            id.clone(),
            LegacyGlobal {
                ty: declaration.ty.clone(),
                mutable: declaration.mutable,
                target: declaration.target.clone(),
            },
        );
        if declaration.exported {
            exports.insert(
                declaration.name.clone(),
                Export::Global {
                    id,
                    ty: declaration.ty.clone(),
                    mutable: declaration.mutable,
                },
            );
        }
    }
    Ok(definitions)
}
