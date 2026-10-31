//! Callable signatures, defaults, parameters, and function-body compilation.

use super::{Binding, Compiler, FunctionContext, ScopeStack, insert, lookup};
use crate::model::{Function, Op, Param, Signature, SignatureData, Type};
use crate::syntax::FunctionAst;
use std::sync::Arc;

pub(crate) fn signature(fun: &FunctionAst, id: String) -> Signature {
    Arc::new(SignatureData {
        generics: fun.generics.clone(),
        id,
        params: crate::type_storage::parameter_list(
            fun.params
                .iter()
                .map(|p| Param {
                    name: p.name.clone(),
                    ty: p.ty.clone(),
                    mutable: p.mutable,
                    default: p.default.is_some(),
                    vararg: p.vararg,
                })
                .collect(),
        ),
        result: fun
            .result
            .clone()
            .unwrap_or(if fun.scene { Type::Unit } else { Type::Pending }),
        suspend: fun.suspend,
        scene: fun.scene,
    })
}

/// A capture environment whose unstable flow proofs have already been removed.
/// Module siblings can share this immutable snapshot instead of rescanning the
/// same declarations for every body. Local functions prepare their own snapshot.
#[derive(Clone)]
pub(super) struct FunctionEnvironment(ScopeStack);
impl FunctionEnvironment {
    pub(super) fn new(scopes: &ScopeStack) -> Self {
        let mut scopes = scopes.clone();
        // A closure can run after an enclosing var changes. Only immutable
        // captured bindings retain narrowing established outside this body.
        super::invalidate_narrowing(&mut scopes);
        Self(scopes)
    }

    pub(super) fn into_scopes(self) -> ScopeStack {
        self.0
    }

    /// Inference can refine a callable's signature without introducing flow
    /// facts. Keep later siblings in the same round up to date with that result.
    pub(super) fn update_signature(&mut self, name: &str, signature: Signature) {
        let binding = super::bindings::symbol_mut(&mut self.0, name).expect("registered callable");
        binding.ty = signature.ty();
        binding.update_signature(signature);
    }
}

impl Compiler {
    pub(super) fn function(
        &mut self,
        fun: &FunctionAst,
        environment: FunctionEnvironment,
        sig: Signature,
    ) -> Result<(), String> {
        crate::declarations::validate_type(&sig.ty())?;
        let env = environment.into_scopes();
        let local = env.len() > 1;
        let mut context = FunctionContext {
            scopes: env,
            instructions: vec![],
            source_file: self.file.clone(),
            function_id: sig.id.clone(),
            return_type: sig.result.clone(),
            may_suspend: sig.suspend,
            loop_stack: vec![],
            next_iterator_id: 0,
            is_reachable: true,
            prose_depth: 0,
            value_requirements: Default::default(),
            value_scopes: vec![],
        };
        context.scopes.push(Arc::default());
        if let Some(ty) = crate::declarations::receiver(&sig.id) {
            insert(
                &mut context.scopes,
                "this",
                Binding {
                    identity: crate::compiler::bindings::Identity::new(),
                    index_key: false,
                    global_aliases: None,
                    volatile: false,
                    implicit_receiver: true,
                    escaped: false,
                    runtime: "this".into(),
                    ty,
                    mutable: false,
                    initialized: true,
                    may_be_initialized: true,
                    owner: context.function_id.clone(),
                    narrow: None,
                    field_narrows: None,
                    callables: vec![],
                    requires: Default::default(),
                    callable_requirements: Default::default(),
                },
            )?;
        }
        for p in fun.params.iter() {
            if lookup(&context.scopes, &p.name).is_some() {
                return Err(format!(
                    "{}:{}: parameter '{}' shadows a visible binding",
                    self.file, fun.line, p.name
                ));
            }
            if let Some(default) = &p.default {
                let supplied = super::flow::SkippedPath::new(&context);
                let jump = context.emit(
                    Op::Default {
                        name: p.name.clone(),
                        target: 0,
                    },
                    fun.line,
                );
                // Defaults execute in this invocation's frame, under the same
                // checked effect as its body. Earlier parameters are installed;
                // later parameters remain outside the default's source scope.
                let actual = self.expression_as(default, &mut context, fun.line, Some(&p.ty))?;
                context.convert(&actual, &p.ty, fun.line)?;
                context.emit(
                    Op::Declare {
                        name: p.name.clone(),
                        ty: p.ty.clone(),
                        mutable: p.mutable,
                    },
                    fun.line,
                );
                context.patch(jump, context.instructions.len());
                supplied.merge(&mut context);
            }
            Arc::make_mut(context.scopes.last_mut().unwrap()).insert_mut(
                p.name.clone(),
                Binding {
                    identity: crate::compiler::bindings::Identity::new(),
                    index_key: false,
                    global_aliases: None,
                    volatile: false,
                    implicit_receiver: false,
                    escaped: false,
                    runtime: p.name.clone(),
                    ty: if p.vararg {
                        Type::named("List", vec![p.ty.clone()])
                    } else {
                        p.ty.clone()
                    },
                    mutable: p.mutable,
                    initialized: true,
                    may_be_initialized: true,
                    owner: context.function_id.clone(),
                    narrow: None,
                    field_narrows: None,
                    callables: vec![],
                    requires: Default::default(),
                    callable_requirements: Default::default(),
                },
            );
        }
        let returns = if let Some(target) = &fun.legacy {
            crate::legacy::validate_signature(&sig, target)
                .map_err(|error| crate::diagnostics::at_source(&self.file, Some(fun.line), error))?;
            for parameter in sig.params.iter() {
                context.emit(
                    Op::Load {
                        name: parameter.name.clone(),
                    },
                    fun.line,
                );
            }
            context.emit(
                Op::LegacyCall {
                    target: target.clone(),
                    arguments: sig.params.len(),
                },
                fun.line,
            );
            context.emit(Op::Return, fun.line);
            true
        } else {
            self.body(&fun.body, &mut context)?
        };
        if sig.result != Type::Unit && !returns {
            return Err(format!(
                "{}:{}: value-returning block must return on every path; add an annotation for inferred block results",
                self.file, fun.line
            ));
        }
        if self
            .single_use_initializers
            .get(&sig.id)
            .is_some_and(|plan| plan.expands())
        {
            // The only source call was expanded into its caller. Retain the
            // reserved local callable without an unused captured-write body.
            context.instructions.clear();
        }
        context.emit(unit(), fun.line);
        context.emit(Op::Return, fun.line);
        self.functions.insert(
            sig.id.clone(),
            Function {
                signature: sig,
                code: context.instructions,
                init: false,
                local,
            },
        );
        Ok(())
    }
}

use crate::model::unit;

impl Compiler {
    pub(super) fn return_value(
        &mut self,
        e: &Option<Expr>,
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        let target = context.return_type.clone();
        let ty = if let Some(e) = e {
            self.expression_as(e, context, line, Some(&target))?
        } else {
            context.emit(unit(), line);
            Type::Unit
        };
        if e.is_some() && context.is_reachable {
            super::captures::check_escaping(
                &context.value_requirements,
                &mut context.scopes,
                &context.function_id,
                &mut self.empty_dependencies,
            )?;
        }
        context.convert(&ty, &target, line)?;
        context.emit(Op::Return, line);
        context.is_reachable = false;

        Ok(())
    }
}

use crate::syntax::Expr;
