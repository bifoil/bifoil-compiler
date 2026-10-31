//! Shared bindings and once-only initialization functions.
//!
//! All names exist during inference. A separate dependency pass orders these
//! functions before runtime execution; a dependent initializer never starts early.

use super::{Binding, Compiler, FunctionContext, ScopeStack, infer, insert, lookup, pending_type, symbol_mut};
use crate::model::{Export, Function, Op, SharedDef, SignatureData, Type, unit};
use crate::syntax::{Ast, FunctionAst, Statement, StatementKind};
use std::collections::BTreeMap;
use std::sync::Arc;

pub(super) fn register_shared(ast: &Ast, module: &str, env: &mut ScopeStack) -> Result<(), String> {
    for global in &ast.globals {
        let StatementKind::Variable { name, ty, mutable, .. } = &global.stmt.kind else {
            return Err("shared declaration requires an initializer".into());
        };
        insert(
            env,
            name,
            Binding {
                identity: crate::compiler::bindings::Identity::new(),
                index_key: false,
                global_aliases: None,
                volatile: false,
                implicit_receiver: false,
                escaped: false,
                runtime: format!("{module}.{name}"),
                ty: ty.clone().unwrap_or(Type::Pending),
                mutable: *mutable,
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
    }
    Ok(())
}

/// Resolve what is currently known; lowering diagnoses invalid/unresolved cases.
pub(super) fn infer_shared(ast: &Ast, env: &mut ScopeStack) -> bool {
    let mut changed = false;
    for global in &ast.globals {
        if let StatementKind::Variable {
            name, ty: None, value, ..
        } = &global.stmt.kind
            && let Ok(actual) = infer(value, env)
            && !pending_type(&actual)
            && lookup(env, name).is_some_and(|binding| binding.ty != actual)
        {
            symbol_mut(env, name).unwrap().ty = actual;
            changed = true;
        }
    }
    changed
}

impl Compiler {
    pub(super) fn shared(
        &mut self,
        ast: &Ast,
        module: &str,
        env: &ScopeStack,
        exports: &mut BTreeMap<String, Export>,
    ) -> Result<(BTreeMap<String, SharedDef>, Option<String>), String> {
        if ast.globals.is_empty() {
            return Ok((BTreeMap::new(), None));
        }
        let bootstrap = format!("{module}.$init");
        let mut context = FunctionContext {
            scopes: env.clone(),
            instructions: vec![],
            source_file: self.file.clone(),
            function_id: bootstrap.clone(),
            return_type: Type::Unit,
            may_suspend: false,
            loop_stack: vec![],
            next_iterator_id: 0,
            is_reachable: true,
            prose_depth: 0,
            value_requirements: Default::default(),
            value_scopes: vec![],
        };
        let function_environment = super::functions::FunctionEnvironment::new(env);
        let mut definitions = BTreeMap::new();
        for (order, global) in ast.globals.iter().enumerate() {
            let StatementKind::Variable { name, value, .. } = &global.stmt.kind else {
                unreachable!()
            };
            let binding = lookup(env, name).unwrap();
            if pending_type(&binding.ty) || binding.ty == Type::Null {
                return Err(format!(
                    "{}:{}: cannot infer shared type; add an explicit annotation",
                    self.file, global.stmt.line
                ));
            }
            let initializer = format!("{module}.$shared:{name}");
            let function = FunctionAst {
                legacy: None,
                generics: vec![],
                name: format!("$shared:{name}"),
                params: Default::default(),
                result: Some(binding.ty.clone()),
                suspend: false,
                scene: false,
                exported: false,
                expression: true,
                line: global.stmt.line,
                body: vec![Statement {
                    line: global.stmt.line,
                    kind: StatementKind::Return(Some(value.clone())),
                }],
            };
            let signature = super::signature(&function, initializer.clone());
            self.function(&function, function_environment.clone(), signature)?;
            self.functions.get_mut(&initializer).unwrap().local = true;
            definitions.insert(
                binding.runtime.clone(),
                SharedDef {
                    order,
                    dependencies: Default::default(),
                    ty: binding.ty.clone(),
                    mutable: binding.mutable,
                    initializer,
                },
            );
            if global.exported {
                exports.insert(
                    name.clone(),
                    Export::Global {
                        id: binding.runtime.clone(),
                        ty: binding.ty,
                        mutable: binding.mutable,
                    },
                );
            }
            context.emit(Op::Load { name: binding.runtime }, global.stmt.line);
            context.emit(Op::Pop, global.stmt.line);
        }
        context.emit(unit(), 1);
        context.emit(Op::Return, 1);
        self.functions.insert(
            bootstrap.clone(),
            Function {
                signature: Arc::new(SignatureData {
                    generics: vec![],
                    id: bootstrap.clone(),
                    params: Default::default(),
                    result: Type::Unit,
                    suspend: false,
                    scene: false,
                }),
                code: context.instructions,
                init: true,
                local: false,
            },
        );
        Ok((definitions, Some(bootstrap)))
    }
}
