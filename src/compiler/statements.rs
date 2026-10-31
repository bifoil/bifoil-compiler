//! Statement sequencing, local-function visibility, and statement lowering.

use super::{
    Binding, Compiler, FunctionContext, captured_names, insert, local_function_id, lookup, pending_type, signature,
};
use crate::model::{MAX_FUNCTIONS, Op, Type};
use crate::syntax::{Expr, Statement, StatementKind};

impl Compiler {
    pub(super) fn body(&mut self, body: &[Statement], context: &mut FunctionContext) -> Result<bool, String> {
        self.body_with_tail(body, context, None)
    }

    /// Lower one lexical body. A do/while condition shares this body's scope;
    /// include its names in proofs before compiling any local declaration.
    pub(super) fn body_with_tail(
        &mut self,
        body: &[Statement],
        context: &mut FunctionContext,
        tail: Option<&Expr>,
    ) -> Result<bool, String> {
        super::enum_aliases::register_block(body, &mut context.scopes)?;
        let entry = context.scopes.clone();
        self.single_use_initializers
            .extend(super::single_use_initializers::discover(
                body,
                &context.function_id,
                tail,
                &entry,
            ));
        let mut closure_sites = std::collections::BTreeMap::new();
        for (f, capture) in super::captures::named_captures(body, &context.scopes) {
            let sig = signature(f, local_function_id(&context.function_id, f));
            let capture = std::sync::Arc::new(capture);
            let requires = capture.all_requirements();
            let callable_requirements = [(sig.id.clone(), capture)].into_iter().collect();
            insert(
                &mut context.scopes,
                &f.name,
                Binding {
                    identity: crate::compiler::bindings::Identity::new(),
                    index_key: false,
                    global_aliases: None,
                    volatile: false,
                    implicit_receiver: false,
                    escaped: false,
                    runtime: sig.id.clone(),
                    ty: sig.ty(),
                    mutable: false,
                    initialized: true,
                    may_be_initialized: true,
                    owner: context.function_id.clone(),
                    narrow: None,
                    field_narrows: None,
                    callables: vec![sig.clone()],
                    requires,
                    callable_requirements,
                },
            )?;
            let id = sig.id.clone();
            let site = context.emit(
                Op::Closure {
                    name: sig.id.clone(),
                    function: sig.id.clone(),
                    captures: None,
                },
                f.line,
            );
            closure_sites.insert(id, site);
        }
        // Resolve local callable results without exposing later variables to statements.
        for _ in 0..=body.len().min(MAX_FUNCTIONS) {
            if !super::local_functions::refine(body, &mut context.scopes, &context.function_id)? {
                break;
            }
            self.check_budget(context)?;
        }
        for stmt in body {
            let line = stmt.line;
            let result = (|| -> Result<(), String> {
                match &stmt.kind {
                    StatementKind::Alias(alias) => crate::declarations::validate_type(&alias.ty)?,
                    StatementKind::When {
                        subject,
                        binding,
                        annotation,
                        cases,
                    } => self.when_statement(subject, binding, annotation, cases, context, line)?,
                    StatementKind::Yield { value, expected } => {
                        let actual = self.expression_as(value, context, line, Some(expected))?;
                        context.convert(&actual, expected, line)?;
                        if context.is_reachable {
                            let depth = context.value_scopes.last().ok_or("yield outside value block")?.0;
                            let requires =
                                super::captures::outside_scope(&context.value_requirements, &context.scopes, depth)?;
                            context.value_scopes.last_mut().unwrap().1.extend(requires);
                        }
                    }
                    StatementKind::Narrative(body) => self.prose_block(body, context, line)?,
                    StatementKind::Text(value) => self.prose_text(value, context, line)?,
                    StatementKind::ParagraphBreak => {
                        context.emit(Op::ProseFlush, line);
                    }

                    StatementKind::Function(f) => {
                        let sig = lookup(&context.scopes, &f.name)
                            .unwrap()
                            .signature_by_id(&local_function_id(&context.function_id, f));
                        if pending_type(&sig.result) {
                            return Err("cannot infer local function result; add an annotation".into());
                        }
                        // Names follow definition-site visibility, although the
                        // closure cells are installed at the start of this block.
                        let names = if self
                            .single_use_initializers
                            .get(&sig.id)
                            .is_some_and(|plan| plan.expands())
                        {
                            vec![]
                        } else {
                            super::captures::runtime_capture_names(&captured_names(f), &context.scopes)
                        };
                        let site = closure_sites[&sig.id];
                        if let Op::Closure { captures, .. } = &mut context.instructions[site].op {
                            *captures = Some(names);
                        }
                        let environment = super::captures::named_environment(&context.scopes, &entry);
                        self.check_local_definition(f, sig, environment, context)?;
                    }
                    StatementKind::Variable {
                        name,
                        ty,
                        mutable,
                        value,
                    } => self.variable(name, ty, mutable, value, context, line)?,
                    StatementKind::Uninitialized { name, ty, mutable } => {
                        self.uninitialized(name, ty, mutable, context, line)?
                    }
                    StatementKind::Assign { name, operator, value } => {
                        self.assign(name, operator, value, context, line)?
                    }
                    StatementKind::Write {
                        target,
                        operator,
                        value,
                    } => self.write(target, operator, value, context, line)?,
                    StatementKind::Expression(e) => {
                        if self.expand_initializer(stmt, context)? {
                            return Ok(());
                        }
                        let ty = self.expression(e, context, line)?;
                        if ty != Type::Nothing {
                            context.emit(Op::Pop, line);
                        }
                    }
                    StatementKind::Return(e) => self.return_value(e, context, line)?,
                    StatementKind::If { branches, no } => self.conditional(branches, no, context, line)?,
                    StatementKind::For {
                        label,
                        name,
                        iterable,
                        body,
                    } => self.for_loop(label, name, iterable, body, context, line)?,
                    StatementKind::While {
                        label,
                        condition,
                        body,
                        post,
                    } => self.while_loop(label, condition, body, post, context, line)?,
                    StatementKind::Choice { options, fallback } => self.choice(options, fallback, context, line)?,
                }
                Ok(())
            })();
            result.map_err(|e| crate::diagnostics::at_source(&self.file, Some(line), e))?;
            self.check_budget(context)?;
        }
        Ok(!context.is_reachable)
    }
}
