//! Declarations, checked updates, and assignment addresses.

use super::{Binding, Compiler, FunctionContext, binary_type, infer, insert, lookup};
use crate::model::{Op, Type};
use crate::syntax::Expr;

impl Compiler {
    pub(super) fn variable(
        &mut self,
        name: &str,
        ty: &Option<Type>,
        mutable: &bool,
        value: &Expr,
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        let actual = self.expression_as(value, context, line, ty.as_ref())?;
        let target = ty.clone().unwrap_or(actual.clone());
        if target == Type::Null {
            return Err("null-only declaration requires an expected type".into());
        }
        context.convert(&actual, &target, line)?;
        let requires = context.value_requirements.clone();
        let identity = crate::compiler::bindings::Identity::new();
        if !mutable {
            self.immutable_iterables
                .remember(identity, value, &actual, &target, &context.scopes);
        }
        insert(
            &mut context.scopes,
            name,
            Binding {
                identity,
                index_key: false,
                global_aliases: None,
                volatile: false,
                implicit_receiver: false,
                escaped: false,
                runtime: name.to_owned(),
                ty: target.clone(),
                mutable: *mutable,
                initialized: true,
                may_be_initialized: true,
                owner: context.function_id.clone(),
                narrow: None,
                field_narrows: None,
                callables: vec![],
                requires,
                callable_requirements: Default::default(),
            },
        )?;
        context.emit(
            Op::Declare {
                name: name.to_owned(),
                ty: target,
                mutable: *mutable,
            },
            line,
        );

        Ok(())
    }

    pub(super) fn uninitialized(
        &mut self,
        name: &str,
        ty: &Type,
        mutable: &bool,
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        insert(
            &mut context.scopes,
            name,
            Binding {
                identity: crate::compiler::bindings::Identity::new(),
                index_key: false,
                global_aliases: None,
                volatile: false,
                implicit_receiver: false,
                escaped: false,
                runtime: name.to_owned(),
                ty: ty.clone(),
                mutable: *mutable,
                initialized: false,
                may_be_initialized: false,
                owner: context.function_id.clone(),
                narrow: None,
                field_narrows: None,
                callables: vec![],
                requires: Default::default(),
                callable_requirements: Default::default(),
            },
        )?;
        context.emit(
            Op::Uninitialized {
                name: name.to_owned(),
                ty: ty.clone(),
                mutable: *mutable,
            },
            line,
        );

        Ok(())
    }

    pub(super) fn assign(
        &mut self,
        name: &str,
        operator: &str,
        value: &Expr,
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        let sym = lookup(&context.scopes, name).ok_or_else(|| format!("unknown binding '{name}'"))?;
        let proven_initializer = self
            .single_use_initializers
            .get(&context.function_id)
            .is_some_and(|plan| plan.permits(name, &sym.owner));
        if context.is_reachable && !sym.mutable && sym.owner != context.function_id && !proven_initializer {
            return Err(format!(
                "cannot initialize captured val '{name}' from a repeatable function"
            ));
        }
        if context.is_reachable && !sym.mutable && sym.may_be_initialized {
            return Err(format!("cannot reassign initialized val '{name}'"));
        }
        if operator != "=" && context.is_reachable && sym.owner == context.function_id && !sym.initialized {
            return Err(format!("compound update reads uninitialized '{name}'"));
        }
        let actual = if operator == "=" {
            self.expression_as(value, context, line, Some(&sym.ty))?
        } else {
            context.emit(
                Op::Load {
                    name: sym.runtime.clone(),
                },
                line,
            );
            let rhs = self.expression(value, context, line)?;
            let op = &operator[..operator.len() - 1];
            let result = binary_type(op, sym.narrow.as_ref().unwrap_or(&sym.ty), &rhs)?;
            context.emit(Op::Binary { operator: op.into() }, line);
            result
        };
        // A block expression on the right may initialize this same binding.
        // Recheck the merged state after evaluation, before the outer write.
        if context.is_reachable
            && !sym.mutable
            && lookup(&context.scopes, name).is_some_and(|binding| binding.may_be_initialized)
        {
            return Err(format!("cannot reassign initialized val '{name}'"));
        }
        // Writing a captured outer cell publishes this frame's values beyond
        // its own locals. It can already have opaque readers even when this
        // function was compiled before the caller published a reader. The RHS
        // may also publish the target, so inspect its current escape state.
        let escaped = lookup(&context.scopes, name).is_some_and(|binding| binding.escaped);
        if context.is_reachable && (sym.owner != context.function_id || sym.owner.is_empty() || escaped) {
            super::captures::check_escaping(
                &context.value_requirements,
                &mut context.scopes,
                &context.function_id,
                &mut self.empty_dependencies,
            )?;
        }
        context.convert(&actual, &sym.ty, line)?;
        let requires = context.value_requirements.clone();
        super::bindings::record_assignment(&mut context.scopes, name, requires);
        context.emit(Op::Store { name: sym.runtime }, line);

        Ok(())
    }

    pub(super) fn write(
        &mut self,
        target: &Expr,
        operator: &str,
        value: &Expr,
        context: &mut FunctionContext,
        line: usize,
    ) -> Result<(), String> {
        let target_type = self.address(target, context, line)?;
        let actual = if operator == "=" {
            self.expression_as(value, context, line, Some(&target_type))?
        } else {
            // Address evaluation may already have effects. Retain this read's
            // type before the RHS can mutate the source or invalidate its facts.
            let read_type = infer(target, &context.scopes)?;
            context.emit(Op::ReadAddress, line);
            let rhs = self.expression(value, context, line)?;
            let op = &operator[..operator.len() - 1];
            let result = binary_type(op, &read_type, &rhs)?;
            context.emit(Op::Binary { operator: op.into() }, line);
            result
        };
        context.convert(&actual, &target_type, line)?;
        if context.is_reachable {
            super::captures::check_escaping(
                &context.value_requirements,
                &mut context.scopes,
                &context.function_id,
                &mut self.empty_dependencies,
            )?;
        }
        context.emit(Op::WriteAddress, line);
        if context.is_reachable {
            // A field or collection write can change a guarded field through a
            // different alias. Immutable paths keep their independent proofs.
            super::field_proofs::invalidate_write(target, &mut context.scopes);
        }

        Ok(())
    }
}
