//! Lift nested shared initializers without giving their cells public names.
//!
//! Source declarations keep lexical order. Each site receives one persistent
//! cell before entry, including sites in unused functions and untaken branches.
//! An initializer can use earlier visible shared cells, but cannot close over
//! surrounding call/visit state or its type parameters.

use crate::model::Type;
use crate::syntax::{Ast, Expr, FunctionAst, Global, Pattern, Statement, StatementKind};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Default)]
struct Context {
    initializing: bool,
    shared: BTreeMap<String, String>,
    locals: BTreeSet<String>,
    parameters: BTreeSet<String>,
    restricted_values: BTreeSet<String>,
    restricted_types: BTreeSet<String>,
}

impl Context {
    fn check_local_name(&self, name: &str) -> Result<(), String> {
        if self.initializing && (self.locals.contains(name) || self.shared.contains_key(name)) {
            return Err(format!("shadowing or duplicate declaration '{name}'"));
        }
        Ok(())
    }

    fn check_type(&self, ty: &Type) -> Result<(), String> {
        if ty.any_node(|node| {
            matches!(node, Type::Parameter { name, .. }
            if self.restricted_types.contains(name))
        }) {
            return Err("nested shared initializer cannot use enclosing type parameters".into());
        }
        Ok(())
    }

    fn reference(&self, name: &mut String) -> Result<(), String> {
        if self.restricted_values.contains(name) {
            return Err(format!("nested shared initializer cannot use enclosing local '{name}'"));
        }
        if let Some(private) = self.shared.get(name) {
            *name = private.clone();
        }
        Ok(())
    }
}

struct Hoister<'a> {
    file: &'a str,
    module: &'a str,
    globals: Vec<Global>,
}

pub(in crate::compiler) fn hoist(ast: &mut Ast, module: &str, file: &str) -> Result<(), String> {
    let mut hoister = Hoister {
        file,
        module,
        globals: vec![],
    };
    for global in &mut ast.globals {
        if let StatementKind::Variable { value, .. } = &mut global.stmt.kind {
            hoister.expression(value, &Context::default())?;
        }
    }
    for function in &mut ast.functions {
        hoister.function(function, &Context::default())?;
    }
    ast.globals.extend(hoister.globals);
    // Dependencies initialize first; independent sites retain physical order.
    ast.globals.sort_by_key(|global| global.stmt.line);
    Ok(())
}

impl Hoister<'_> {
    fn function(&mut self, function: &mut FunctionAst, parent: &Context) -> Result<(), String> {
        let mut context = parent.clone();
        if let Some(receiver) = crate::declarations::receiver(&format!("{}.{}", self.module, function.name)) {
            context.locals.insert("this".into());
            receiver.for_each_node(|node| {
                if let Type::Parameter { name, .. } = node {
                    context.parameters.insert(name.clone());
                }
            });
        }
        for generic in &function.generics {
            context.parameters.insert(generic.name.clone());
            for bound in &generic.bounds {
                parent.check_type(bound)?;
            }
        }
        for parameter in Arc::make_mut(&mut function.params) {
            context.check_local_name(&parameter.name)?;
            parent.check_type(&parameter.ty)?;
            // Generated method receivers also carry their owning class's type
            // parameters, which must not become persistent cell type arguments.
            parameter.ty.for_each_node(|node| {
                if let Type::Parameter { name, .. } = node {
                    context.parameters.insert(name.clone());
                }
            });
            if let Some(default) = &mut parameter.default {
                self.expression(default, &context)?;
            }
            context.locals.insert(parameter.name.clone());
        }
        if let Some(result) = &function.result {
            parent.check_type(result)?;
        }
        self.block(&mut function.body, &context)?;
        Ok(())
    }

    fn block(&mut self, body: &mut [Statement], parent: &Context) -> Result<Context, String> {
        let mut context = parent.clone();
        for statement in body.iter() {
            if let StatementKind::Function(function) = &statement.kind {
                context.check_local_name(&function.name)?;
            }
        }
        context.locals.extend(body.iter().filter_map(|statement| {
            if let StatementKind::Function(function) = &statement.kind {
                Some(function.name.clone())
            } else {
                None
            }
        }));
        for statement in body {
            let result = self.statement(statement, &mut context);
            result.map_err(|error| crate::diagnostics::at_source(self.file, Some(statement.line), error))?;
        }
        Ok(context)
    }

    fn statement(&mut self, statement: &mut Statement, context: &mut Context) -> Result<(), String> {
        match &mut statement.kind {
            StatementKind::Variable {
                shared: true,
                name,
                ty,
                mutable,
                value,
            } => {
                context.check_local_name(name)?;
                let private = format!("$scoped:{}:{}", statement.line, self.globals.len());
                let mut initializer = context.clone();
                initializer.initializing = true;
                initializer.restricted_values.extend(context.locals.iter().cloned());
                initializer.restricted_types.extend(context.parameters.iter().cloned());
                if let Some(ty) = ty {
                    initializer.check_type(ty)?;
                }
                self.expression(value, &initializer)?;
                let value = std::mem::replace(value, Expr::Name(private.clone()));
                self.globals.push(Global {
                    exported: false,
                    stmt: Statement {
                        line: statement.line,
                        kind: StatementKind::Variable {
                            shared: false,
                            name: private.clone(),
                            ty: ty.clone(),
                            mutable: *mutable,
                            value,
                        },
                    },
                });
                context.shared.insert(name.clone(), private);
            }
            StatementKind::Variable { name, ty, value, .. } => {
                context.check_local_name(name)?;
                if let Some(ty) = ty {
                    context.check_type(ty)?;
                }
                self.expression(value, context)?;
                context.locals.insert(name.clone());
            }
            StatementKind::Uninitialized { name, ty, .. } => {
                context.check_local_name(name)?;
                context.check_type(ty)?;
                context.locals.insert(name.clone());
            }
            StatementKind::Alias(alias) => {
                context.check_type(&alias.ty)?;
                for generic in &alias.generics {
                    for bound in &generic.bounds {
                        context.check_type(bound)?;
                    }
                }
            }
            StatementKind::Function(function) => self.function(function, context)?,
            StatementKind::Expression(value) | StatementKind::Text(value) | StatementKind::Return(Some(value)) => {
                self.expression(value, context)?
            }
            StatementKind::Yield { value, expected } => {
                context.check_type(expected)?;
                self.expression(value, context)?;
            }
            StatementKind::Assign { name, value, .. } => {
                self.expression(value, context)?;
                if context.initializing {
                    context.reference(name)?;
                }
            }
            StatementKind::Write { target, value, .. } => {
                self.expression(target, context)?;
                self.expression(value, context)?;
            }
            StatementKind::Narrative(body) => {
                self.block(body, context)?;
            }
            StatementKind::If { branches, no } => {
                for branch in branches {
                    self.expression(&mut branch.condition, context)?;
                    self.block(&mut branch.body, context)?;
                }
                self.block(no, context)?;
            }
            StatementKind::While {
                condition, body, post, ..
            } => {
                if *post {
                    let body_context = self.block(body, context)?;
                    self.expression(condition, &body_context)?;
                } else {
                    self.expression(condition, context)?;
                    self.block(body, context)?;
                }
            }
            StatementKind::For {
                name, iterable, body, ..
            } => {
                self.expression(iterable, context)?;
                let mut nested = context.clone();
                nested.check_local_name(name)?;
                nested.locals.insert(name.clone());
                self.block(body, &nested)?;
            }
            StatementKind::When {
                subject,
                binding,
                annotation,
                cases,
            } => {
                if let Some(ty) = annotation {
                    context.check_type(ty)?;
                }
                if let Some(value) = subject {
                    self.expression(value, context)?;
                }
                let mut branch_context = context.clone();
                if let Some(name) = binding {
                    branch_context.check_local_name(name)?;
                    branch_context.locals.insert(name.clone());
                }
                for case in cases {
                    let mut nested = branch_context.clone();
                    for pattern in &mut case.patterns {
                        match pattern {
                            Pattern::Value(value) | Pattern::Membership { value, .. } => {
                                self.expression(value, &nested)?;
                            }
                            Pattern::Type { ty, bindings, .. } => {
                                nested.check_type(ty)?;
                                if let Some(names) = bindings {
                                    for name in names.iter().filter(|name| *name != "_") {
                                        branch_context.check_local_name(name)?;
                                    }
                                    nested.locals.extend(names.iter().filter(|name| *name != "_").cloned());
                                }
                            }
                        }
                    }
                    if let Some(guard) = &mut case.guard {
                        self.expression(guard, &nested)?;
                    }
                    self.block(&mut case.body, &nested)?;
                }
            }
            StatementKind::Choice { options, fallback } => {
                for option in options {
                    self.expression(&mut option.visible, context)?;
                    self.expression(&mut option.enabled, context)?;
                    self.expression(&mut option.label, context)?;
                    self.block(&mut option.body, context)?;
                }
                if let Some(body) = fallback {
                    self.block(body, context)?;
                }
            }
            StatementKind::Return(None) | StatementKind::ImplicitReturn | StatementKind::ParagraphBreak => {}
        }
        Ok(())
    }

    fn expression(&mut self, expression: &mut Expr, context: &Context) -> Result<(), String> {
        match expression {
            Expr::Name(name) | Expr::Reference(name) => {
                // Ordinary executable code keeps its lexical spelling. Only a
                // lifted initializer rewrites prior shared aliases.
                if context.initializing {
                    context.reference(name)?;
                }
            }
            Expr::TypeApply { callee, args } => {
                for ty in args {
                    context.check_type(ty)?;
                }
                self.expression(callee, context)?;
            }
            Expr::Construct { ty, fields, .. } => {
                context.check_type(ty)?;
                for (_, value) in fields {
                    self.expression(value, context)?;
                }
            }
            Expr::Block(body) => {
                self.block(body, context)?;
            }
            Expr::Lambda(function) => self.function(function, context)?,
            Expr::Group(value)
            | Expr::Unary(_, value, ..)
            | Expr::Unwrap(value, ..)
            | Expr::Spread(value)
            | Expr::BoundReference(value, ..) => self.expression(value, context)?,
            Expr::Member { receiver, .. } => self.expression(receiver, context)?,
            Expr::TypeOp { value, ty, .. } => {
                context.check_type(ty)?;
                self.expression(value, context)?;
            }
            Expr::Index { receiver, index, .. } | Expr::Binary(_, receiver, index, ..) => {
                self.expression(receiver, context)?;
                self.expression(index, context)?;
            }
            Expr::Call(callee, arguments, ..) | Expr::SafeCall(callee, arguments, ..) => {
                self.expression(callee, context)?;
                for (_, value) in arguments {
                    self.expression(value, context)?;
                }
            }
            Expr::Collection { entries, .. } => {
                for (key, value) in entries {
                    if let Some(key) = key {
                        self.expression(key, context)?;
                    }
                    self.expression(value, context)?;
                }
            }
            Expr::Format(parts) => {
                for part in parts {
                    self.expression(&mut part.value, context)?;
                }
            }
            Expr::Transfer { value: Some(value), .. } => self.expression(value, context)?,
            Expr::Transfer { value: None, .. }
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Boolean(_)
            | Expr::String(_)
            | Expr::Char(_)
            | Expr::Null => {}
        }
        Ok(())
    }
}
