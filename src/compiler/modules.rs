//! Shared declaration preparation and module instruction lowering.

mod interfaces;
pub(crate) use interfaces::{declaration_interface, interface, seed};

use super::{Binding, Compiler, infer_returns, insert, lookup, pending_type, signature};
use crate::model::{
    Export, FORMAT, Function, MAX_FUNCTIONS, MAX_INSTRUCTIONS, Module, PROFILE, Param, Signature, SignatureData, Type,
};
use crate::syntax::Ast;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

pub(crate) fn compile(
    ast: Ast,
    name: &str,
    file: &str,
    deps: &crate::model::ModuleView<'_>,
    started: Instant,
) -> Result<Module, String> {
    lower(ast, name, file, deps, started, true)
}
pub(crate) fn compile_group_member(
    ast: Ast,
    name: &str,
    file: &str,
    deps: &crate::model::ModuleView<'_>,
    started: Instant,
) -> Result<Module, String> {
    lower(ast, name, file, deps, started, false)
}
fn lower(
    ast: Ast,
    name: &str,
    file: &str,
    deps: &crate::model::ModuleView<'_>,
    started: Instant,
    analyze_initialization: bool,
) -> Result<Module, String> {
    let _analysis_session = crate::declarations::AnalysisSession::enter();
    let Prepared {
        ast,
        env,
        function_ids,
        legacy_globals,
        mut exports,
        dependencies,
    } = prepare(ast, name, file, deps, started)?;
    crate::declarations::validate_variance()?;
    let mut compiler = Compiler {
        empty_dependencies: Default::default(),
        empty_publications: Default::default(),
        initialized_captures: Default::default(),
        single_use_initializers: Default::default(),
        immutable_iterables: Default::default(),
        functions: BTreeMap::new(),
        file: file.into(),
        started,
    };
    let function_environment = super::functions::FunctionEnvironment::new(&env);
    for (index, fun) in ast.functions.iter().enumerate() {
        let sig = lookup(&env, &fun.name).unwrap().signature_by_id(&function_ids[index]);
        if pending_type(&sig.result) {
            infer_returns(fun, &env).map_err(|error| crate::diagnostics::at_source(file, Some(fun.line), error))?;
            return Err(format!(
                "{file}:{}: cannot infer recursive result; add -> Type",
                fun.line
            ));
        }
        compiler.function(fun, function_environment.clone(), sig.clone())?;
        if fun.exported {
            export_function(&mut exports, &fun.name, sig)?;
        }
    }
    let (mut shared, init) = compiler.shared(&ast, name, &env, &mut exports)?;
    export_types(&ast, name, &compiler.functions, &mut exports);
    // Generated constructors, copies, and initializers count toward the same
    // module envelope as explicitly declared functions.
    if compiler.functions.len() > MAX_FUNCTIONS {
        return Err("module function budget exceeded".into());
    }
    for function in compiler.functions.values_mut() {
        super::reachability::prune(&mut function.code, started)?;
    }
    let count: usize = compiler.functions.values().map(|f| f.code.len()).sum();
    if count > MAX_INSTRUCTIONS {
        return Err("module instruction budget exceeded".into());
    }
    let summaries = if analyze_initialization {
        let summaries = crate::initialization::discover(&compiler.functions, &mut shared, deps, started)?;
        let mut all_shared = BTreeMap::new();
        for dependency in deps.values() {
            all_shared.extend(dependency.shared.clone());
        }
        all_shared.extend(shared.clone());
        crate::initialization::order(&all_shared, started)?;
        summaries
    } else {
        BTreeMap::new()
    };
    let mut module = Module {
        summaries,
        group: None,
        legacy_globals,
        shared,
        types: crate::declarations::owned_types(&ast, name),
        format: FORMAT.into(),
        profile: PROFILE.into(),
        name: name.into(),
        digest: String::new(),
        dependencies,
        exports,
        functions: compiler.functions,
        init,
    };
    crate::signature_sharing::module(&mut module);
    module.digest = digest(&module)?;
    Ok(module)
}

use crate::model::digest;

struct Prepared {
    ast: Ast,
    env: super::ScopeStack,
    function_ids: Vec<String>,
    legacy_globals: BTreeMap<String, crate::model::LegacyGlobal>,
    exports: BTreeMap<String, Export>,
    dependencies: BTreeMap<String, String>,
}
fn prepare(
    ast: Ast,
    name: &str,
    file: &str,
    deps: &crate::model::ModuleView<'_>,
    started: Instant,
) -> Result<Prepared, String> {
    let (ast, types) = crate::declarations::prepare(ast, name, deps)?;
    for alias in &ast.aliases {
        crate::declarations::validate_type(&alias.ty)?;
    }

    let mut env = vec![Arc::default()];
    // Unit is a value binding even though lowering emits its constant directly.
    // Register it so no-shadowing and expected-enum conflict checks agree.
    insert(
        &mut env,
        "Unit",
        Binding {
            identity: crate::compiler::bindings::Identity::new(),
            index_key: false,
            global_aliases: None,
            volatile: false,
            implicit_receiver: false,
            escaped: false,
            runtime: "Unit".into(),
            ty: Type::Unit,
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
    for (name, input, result) in [
        ("Int", Type::Float, Type::Int),
        ("Float", Type::Float, Type::Float),
        ("String", Type::Any.nullable(), Type::String),
        ("Char", Type::Int, Type::Char),
    ] {
        let sig = Arc::new(SignatureData {
            generics: vec![],
            id: format!("$builtin.{name}"),
            params: vec![Param {
                name: "value".into(),
                ty: input,
                mutable: false,
                default: false,
                vararg: false,
            }]
            .into(),
            result,
            suspend: false,
            scene: false,
        });
        insert(
            &mut env,
            name,
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
                owner: String::new(),
                narrow: None,
                field_narrows: None,
                callables: vec![sig],
                requires: Default::default(),
                callable_requirements: Default::default(),
            },
        )?;
    }
    let mut dependencies = BTreeMap::new();
    for import in &ast.imports {
        let module = deps
            .get(&import.module)
            .ok_or_else(|| format!("{file}: missing compiled module '{}'", import.module))?;
        let exported = module
            .exports
            .get(&import.symbol)
            .ok_or_else(|| format!("{file}: '{}' is not exported by {}", import.symbol, import.module))?;
        let namespace = match exported {
            Export::Type { id, .. } => Some(id.as_str()),
            Export::Alias {
                parameters,
                ty: Type::Named { name, args },
            } if parameters.is_empty()
                && args.is_empty()
                && types.get(name).is_some_and(|definition| definition.kind == "enum") =>
            {
                Some(name.as_str())
            }
            _ => None,
        };
        if let Some(id) = namespace
            && let Some(def) = types.get(id)
        {
            // A transparent alias may export an enum owned by a transitive
            // dependency. Its case constructors stay with that declaration.
            let owner = deps
                .values()
                .find(|dependency| dependency.types.contains_key(id))
                .ok_or("missing imported type owner")?;
            for variant in &def.variants {
                let sig = owner
                    .functions
                    .get(variant)
                    .ok_or("missing variant constructor")?
                    .signature
                    .clone();
                let local = format!("{}.{}", import.alias, variant.rsplit('.').next().unwrap());
                insert(
                    &mut env,
                    &local,
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
                        owner: String::new(),
                        narrow: None,
                        field_narrows: None,
                        callables: vec![sig],
                        requires: Default::default(),
                        callable_requirements: Default::default(),
                    },
                )?;
            }
        }
        let sym = match exported {
            Export::Function { signature: s } => Binding {
                identity: crate::compiler::bindings::Identity::new(),
                index_key: false,
                global_aliases: None,
                volatile: false,
                implicit_receiver: false,
                escaped: false,
                runtime: s.id.clone(),
                ty: s.ty(),
                mutable: false,
                initialized: true,
                may_be_initialized: true,
                owner: String::new(),
                narrow: None,
                field_narrows: None,
                callables: vec![s.clone()],
                requires: Default::default(),
                callable_requirements: Default::default(),
            },
            Export::Functions { signatures } => Binding {
                identity: crate::compiler::bindings::Identity::new(),
                index_key: false,
                global_aliases: None,
                volatile: false,
                implicit_receiver: false,
                escaped: false,
                runtime: signatures[0].id.clone(),
                ty: signatures[0].ty(),
                mutable: false,
                initialized: true,
                may_be_initialized: true,
                owner: String::new(),
                narrow: None,
                field_narrows: None,
                callables: signatures.clone(),
                requires: Default::default(),
                callable_requirements: Default::default(),
            },
            Export::Alias { .. } => {
                dependencies.insert(import.module.clone(), module.digest.clone());
                continue;
            }
            Export::Type { id, constructor } => {
                // Enums and abstract sums supply a type namespace and variants,
                // not a runtime value or an unapplied generic instance.
                let Some(constructor) = constructor else {
                    dependencies.insert(import.module.clone(), module.digest.clone());
                    continue;
                };
                Binding {
                    identity: crate::compiler::bindings::Identity::new(),
                    index_key: false,
                    global_aliases: None,
                    volatile: false,
                    implicit_receiver: false,
                    escaped: false,
                    runtime: id.clone(),
                    ty: constructor.ty(),
                    mutable: false,
                    initialized: true,
                    may_be_initialized: true,
                    owner: String::new(),
                    narrow: None,
                    field_narrows: None,
                    callables: vec![constructor.clone()],
                    requires: Default::default(),
                    callable_requirements: Default::default(),
                }
            }
            Export::Global { id, ty, mutable } => Binding {
                identity: crate::compiler::bindings::Identity::new(),
                index_key: false,
                global_aliases: None,
                volatile: module.legacy_globals.contains_key(id),
                implicit_receiver: false,
                escaped: false,
                runtime: id.clone(),
                ty: ty.clone(),
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
        };
        insert(&mut env, &import.alias, sym)?;
        dependencies.insert(import.module.clone(), module.digest.clone());
    }
    let mut exports = BTreeMap::new();
    let legacy_globals = super::legacy::register(&ast, name, file, &mut env, &mut exports)?;
    super::shared::register_shared(&ast, name, &mut env)?;
    let function_ids = function_ids(&ast, name);
    for (index, fun) in ast.functions.iter().enumerate() {
        let sig = signature(fun, function_ids[index].clone());
        insert(
            &mut env,
            &fun.name,
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
                owner: String::new(),
                narrow: None,
                field_narrows: None,
                callables: vec![sig],
                requires: Default::default(),
                callable_requirements: Default::default(),
            },
        )?;
    }
    super::enum_aliases::register(&ast.aliases, &mut env)?;
    super::bindings::index_global_aliases(&mut env);
    for _ in 0..=ast.functions.len() + ast.globals.len() {
        crate::budget(started)?;
        let mut changed = super::shared::infer_shared(&ast, &mut env);
        let mut inference_environment = super::functions::FunctionEnvironment::new(&env);
        for (index, fun) in ast.functions.iter().enumerate() {
            crate::budget(started)?;
            if fun.result.is_some() || fun.scene {
                continue;
            }
            let mut sig = lookup(&env, &fun.name).unwrap().signature_by_id(&function_ids[index]);
            let Ok(result) = super::return_inference::infer_prepared_returns(fun, inference_environment.clone()) else {
                continue;
            };
            if !pending_type(&result) && sig.result != result {
                Arc::make_mut(&mut sig).result = result;
                crate::declarations::update_method(&sig);
                if !sig.ty().within_depth(32) {
                    return Err("type graph budget exceeded".into());
                }
                let symbol = Arc::make_mut(&mut env[0]).get_mut(&fun.name).unwrap();
                symbol.ty = sig.ty();
                symbol.update_signature(sig.clone());
                inference_environment.update_signature(&fun.name, sig);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    Ok(Prepared {
        ast,
        env,
        function_ids,
        legacy_globals,
        exports,
        dependencies,
    })
}

fn export_function(exports: &mut BTreeMap<String, Export>, name: &str, sig: Signature) -> Result<(), String> {
    match exports.get_mut(name) {
        Some(Export::Function { signature }) => {
            let signatures = vec![signature.clone(), sig];
            exports.insert(name.into(), Export::Functions { signatures });
        }
        Some(Export::Functions { signatures }) => signatures.push(sig),
        None => {
            exports.insert(name.into(), Export::Function { signature: sig });
        }
        _ => return Err("export name conflicts with another declaration".into()),
    }
    Ok(())
}

fn function_ids(ast: &Ast, name: &str) -> Vec<String> {
    let mut function_counts = BTreeMap::<&str, usize>::new();
    for fun in &ast.functions {
        *function_counts.entry(&fun.name).or_default() += 1;
    }
    ast.functions
        .iter()
        .enumerate()
        .map(|(index, fun)| {
            let base = format!("{name}.{}", fun.name);
            if function_counts[fun.name.as_str()] > 1 {
                format!("{base}#overload:{index}")
            } else {
                base
            }
        })
        .collect()
}

fn export_types(ast: &Ast, name: &str, functions: &BTreeMap<String, Function>, exports: &mut BTreeMap<String, Export>) {
    for alias in &ast.aliases {
        if alias.exported {
            exports.insert(
                alias.name.clone(),
                Export::Alias {
                    parameters: alias.generics.clone(),
                    ty: alias.ty.clone(),
                },
            );
        }
    }
    for definition in &ast.types {
        if definition.exported {
            let id = format!("{name}.{}", definition.name);
            let constructor = functions.get(&id).map(|function| function.signature.clone());
            exports.insert(definition.name.clone(), Export::Type { id, constructor });
        }
    }
}
