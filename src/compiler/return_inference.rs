//! Infer callable result types without emitting instructions.
//!
//! Only normally completing branches contribute successor narrowing. Effects
//! invalidate mutable facts conservatively; final body checking remains authoritative.

mod branches;
mod expressions;
mod loops;

use super::result_types::InferredValue;
use expressions::collect as expression_returns;

use super::{Binding, ScopeStack, expression_requirements, infer, insert, local_function_id, signature};
use crate::model::{MAX_FUNCTIONS, Type};
use crate::syntax::{ConditionalBranch, Expr, FunctionAst, Statement, StatementKind, WhenCase};
use std::sync::Arc;

#[derive(Default)]
struct Inference {
    types: Vec<InferredValue>,
    loops: Vec<loops::Loop>,
    external_loop_transfers: bool,
    immutable_iterables: super::loops::nonempty::ImmutableValues,
}

/// Transfer only normally completing flow facts; never emit or execute source.
/// Applicability, expression inference and return analysis use the same walker.
pub(super) fn normal_completion(expression: &Expr, env: &mut ScopeStack) -> Result<bool, String> {
    let mut analysis = Inference {
        external_loop_transfers: true,
        ..Default::default()
    };
    expression_returns(expression, env, &mut analysis)
}

struct ValueResults<'a> {
    expected: Option<&'a Type>,
    types: Vec<InferredValue>,
}

/// Value cases share guarded branch environments with ordinary return inference.
/// Real returns remain transfers from the surrounding function, not block values.
pub(super) fn when_value_result(
    subject: &Option<Expr>,
    binding: &Option<String>,
    annotation: &Option<Type>,
    cases: &[WhenCase],
    env: &ScopeStack,
    expected: Option<&Type>,
) -> Result<InferredValue, String> {
    let subject_type = subject
        .as_ref()
        .map(|value| super::infer_as(value, env, annotation.as_ref()))
        .transpose()?
        .map(|actual| annotation.clone().unwrap_or(actual));
    if !super::when_exhaustive(subject_type.as_ref(), cases, env)? {
        return Err("value-producing when requires exhaustive cases or else".into());
    }
    let mut scope = env.clone();
    let mut analysis = Inference {
        external_loop_transfers: true,
        ..Default::default()
    };
    let mut values = ValueResults {
        expected,
        types: vec![],
    };
    branches::when_statement(
        subject,
        binding,
        annotation,
        cases,
        &mut scope,
        &mut analysis,
        Some(&mut values),
    )?;
    super::result_types::result_join(values.types, expected)
}

/// Inspect statements before a value block's final expression using normal flow
/// rules. All local function names are visible, but variables retain lexical order.
pub(super) fn value_prefix(body: &[Statement], env: &ScopeStack) -> Result<(ScopeStack, bool), String> {
    let mut scope = env.clone();
    scope.push(Arc::default());
    prepare_functions(body, &mut scope)?;
    let mut analysis = Inference {
        external_loop_transfers: true,
        ..Default::default()
    };
    let reaches = block(&body[..body.len().saturating_sub(1)], &mut scope, &mut analysis)?;
    Ok((scope, reaches))
}

pub(super) fn if_value_result(
    branches: &[ConditionalBranch],
    no: &[Statement],
    env: &ScopeStack,
    expected: Option<&Type>,
) -> Result<InferredValue, String> {
    let mut scope = env.clone();
    let mut analysis = Inference {
        external_loop_transfers: true,
        ..Default::default()
    };
    let mut values = ValueResults {
        expected,
        types: vec![],
    };
    branches::conditional(branches, no, &mut scope, &mut analysis, Some(&mut values))?;
    // Preserve the previous right-associated joins of an if/elif chain while
    // keeping horizontal source branches out of the native call stack.
    let mut types = values.types.into_iter().rev();
    let Some(mut result) = types.next() else {
        return super::result_types::result_join(vec![], expected);
    };
    let mut joined = false;
    for left in types {
        result = super::result_types::result_join(vec![left, result], expected)?;
        joined = true;
    }
    if joined {
        Ok(result)
    } else {
        super::result_types::result_join(vec![result], expected)
    }
}

pub(super) fn infer_returns(fun: &FunctionAst, env: &ScopeStack) -> Result<Type, String> {
    infer_prepared_returns(fun, super::functions::FunctionEnvironment::new(env))
}

pub(super) fn infer_prepared_returns(
    fun: &FunctionAst,
    environment: super::functions::FunctionEnvironment,
) -> Result<Type, String> {
    let mut env = environment.into_scopes();
    env.push(Arc::default());
    // Only generated member declarations introduce a receiver. Local functions
    // inherit any enclosing `this` through their ordinary lexical environment.
    let receiver = if fun.name.starts_with("$type:") {
        super::bindings::lookup_ref(&env, &fun.name).and_then(|binding| {
            binding
                .callables
                .iter()
                .find_map(|sig| crate::declarations::receiver(&sig.id))
        })
    } else {
        None
    };
    if let Some(ty) = receiver {
        insert(
            &mut env,
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
                owner: String::new(),
                narrow: None,
                field_narrows: None,
                callables: vec![],
                requires: Default::default(),
                callable_requirements: Default::default(),
            },
        )?;
    }
    for p in fun.params.iter() {
        insert(
            &mut env,
            &p.name,
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
                owner: String::new(),
                narrow: None,
                field_narrows: None,
                callables: vec![],
                requires: Default::default(),
                callable_requirements: Default::default(),
            },
        )?;
    }
    let mut analysis = Inference::default();
    block(&fun.body, &mut env, &mut analysis)?;
    if analysis.types.is_empty() {
        return Ok(Type::Unit);
    }
    super::result_types::result_join(analysis.types, None)
        .map(|value| value.ty)
        .map_err(|_| "function return paths have incompatible types".into())
}

fn hoist_functions(body: &[Statement], env: &mut ScopeStack) -> Result<(), String> {
    for (f, capture) in super::captures::named_captures(body, env) {
        let sig = signature(f, local_function_id("$inference", f));
        if env.last().is_some_and(|scope| {
            !scope
                .get(&f.name)
                .is_some_and(|s| s.callables.iter().any(|old| old.id == sig.id))
        }) {
            let capture = std::sync::Arc::new(capture);
            let requires = capture.all_requirements();
            let callable_requirements = [(sig.id.clone(), capture)].into_iter().collect();
            insert(
                env,
                &f.name,
                Binding {
                    identity: crate::compiler::bindings::Identity::new(),
                    index_key: false,
                    global_aliases: None,
                    volatile: false,
                    implicit_receiver: false,
                    escaped: false,
                    runtime: f.name.clone(),
                    ty: sig.ty(),
                    mutable: false,
                    initialized: true,
                    may_be_initialized: true,
                    owner: String::new(),
                    narrow: None,
                    field_narrows: None,
                    callables: vec![sig],
                    requires,
                    callable_requirements,
                },
            )?
        }
    }
    Ok(())
}

fn prepare_functions(body: &[Statement], env: &mut ScopeStack) -> Result<(), String> {
    super::enum_aliases::register_block(body, env)?;
    hoist_functions(body, env)?;
    for _ in 0..=body.len().min(MAX_FUNCTIONS) {
        if !super::local_functions::refine(body, env, "$inference")? {
            break;
        }
    }
    Ok(())
}

fn block(body: &[Statement], env: &mut ScopeStack, analysis: &mut Inference) -> Result<bool, String> {
    prepare_functions(body, env)?;
    for stmt in body {
        match &stmt.kind {
            StatementKind::Variable {
                name,
                ty,
                mutable,
                value,
            } => {
                let actual = super::infer_as(value, env, ty.as_ref())?;
                let ty = ty.clone().unwrap_or_else(|| actual.clone());
                if !expression_returns(value, env, analysis)? || ty == Type::Nothing {
                    return Ok(false);
                }
                let requires = expression_requirements(value, env);
                let identity = crate::compiler::bindings::Identity::new();
                if !mutable {
                    analysis
                        .immutable_iterables
                        .remember(identity, value, &actual, &ty, env);
                }
                insert(
                    env,
                    name,
                    Binding {
                        identity,
                        index_key: false,
                        global_aliases: None,
                        volatile: false,
                        implicit_receiver: false,
                        escaped: false,
                        runtime: name.clone(),
                        ty,
                        mutable: *mutable,
                        initialized: true,
                        may_be_initialized: true,
                        owner: String::new(),
                        narrow: None,
                        field_narrows: None,
                        callables: vec![],
                        requires,
                        callable_requirements: Default::default(),
                    },
                )?;
            }
            StatementKind::Uninitialized { name, ty, mutable } => {
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
                        runtime: name.clone(),
                        ty: ty.clone(),
                        mutable: *mutable,
                        initialized: false,
                        may_be_initialized: false,
                        owner: String::new(),
                        narrow: None,
                        field_narrows: None,
                        callables: vec![],
                        requires: Default::default(),
                        callable_requirements: Default::default(),
                    },
                )?;
            }
            StatementKind::Return(value) => {
                expressions::returned(value.as_ref(), env, analysis)?;
                return Ok(false);
            }
            StatementKind::If { branches, no } => {
                if !branches::conditional(branches, no, env, analysis, None)? {
                    return Ok(false);
                }
            }
            StatementKind::For {
                label,
                name,
                iterable,
                body,
            } => {
                if !loops::for_statement(label, name, iterable, body, env, analysis)? {
                    return Ok(false);
                }
            }
            StatementKind::While {
                label,
                condition,
                body,
                post,
            } => {
                if !loops::while_statement(label, condition, body, *post, env, analysis)? {
                    return Ok(false);
                }
            }
            StatementKind::Narrative(body) => {
                env.push(Arc::default());
                let reaches = block(body, env, analysis)?;
                env.pop();
                if !reaches {
                    return Ok(false);
                }
            }
            StatementKind::When {
                subject,
                binding,
                annotation,
                cases,
            } => {
                if !branches::when_statement(subject, binding, annotation, cases, env, analysis, None)? {
                    return Ok(false);
                }
            }
            StatementKind::Choice { options, fallback } => {
                if !branches::choice(options, fallback, env, analysis)? {
                    return Ok(false);
                }
            }
            StatementKind::Assign { name, value, .. } => {
                if !expression_returns(value, env, analysis)? {
                    return Ok(false);
                }
                let requires = expression_requirements(value, env);
                super::bindings::record_assignment(env, name, requires);
            }
            StatementKind::Write { target, value, .. } => {
                if !expression_returns(target, env, analysis)? || !expression_returns(value, env, analysis)? {
                    return Ok(false);
                }
                super::field_proofs::invalidate_write(target, env);
            }
            StatementKind::Expression(value) | StatementKind::Text(value) => {
                let ty = infer(value, env)?;
                if !expression_returns(value, env, analysis)? || ty == Type::Nothing {
                    return Ok(false);
                }
            }
            _ => {}
        }
    }
    Ok(true)
}
