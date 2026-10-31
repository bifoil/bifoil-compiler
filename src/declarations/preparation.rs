//! Prepare one source module and its generated nominal declarations.

use super::type_names::Scope;
use super::{
    ConstructorDefaults, ConstructorSpec, generate_constructor, install, install_imported_alias, normalize_block,
    normalize_expression, normalize_function, register_type_aliases, resolve_generic_parameters, resolve_type,
};
use crate::model::{Export, FieldDef, Type, TypeDef, TypeDefinitions};
use crate::syntax::{AliasAst, Ast};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(crate) fn prepare(
    mut ast: Ast,
    module: &str,
    deps: &crate::model::ModuleView<'_>,
) -> Result<(Ast, TypeDefinitions), String> {
    let mut names = Scope::new();
    names.insert("$module".into(), module.into());
    let mut types = BTreeMap::new();
    for d in deps.values() {
        types.extend(d.types.clone());
        for t in d.types.values() {
            for p in &t.parameters {
                crate::generics::install(p)
            }
        }
        for f in d.functions.values() {
            for p in &f.signature.generics {
                crate::generics::install(p)
            }
        }
    }
    install(types.clone());
    for i in &ast.imports {
        if let Some(Export::Alias { parameters, ty }) = deps.get(&i.module).and_then(|m| m.exports.get(&i.symbol)) {
            let id = format!("$import:{}.{}", i.module, i.symbol);
            names.insert(i.alias.clone(), id.clone());
            install_imported_alias(
                id,
                AliasAst {
                    name: i.alias.clone(),
                    generics: parameters.clone(),
                    ty: ty.clone(),
                    exported: false,
                    line: 0,
                },
            );
        }
    }
    for i in &ast.imports {
        if let Some(Export::Type { id, .. }) = deps.get(&i.module).and_then(|m| m.exports.get(&i.symbol)) {
            names.insert(i.alias.clone(), id.clone());
        }
    }
    for d in &ast.types {
        if names.insert(d.name.clone(), format!("{module}.{}", d.name)).is_some() {
            return Err("duplicate type name".into());
        }
    }
    register_type_aliases(&mut ast.aliases, &mut names)?;
    let empty = BTreeSet::new();
    for global in &mut ast.legacy_globals {
        resolve_type(&mut global.ty, &names)?;
    }
    for f in &mut ast.functions {
        normalize_function(f, &names, &empty)?
    }
    for g in &mut ast.globals {
        normalize_block(std::slice::from_mut(&mut g.stmt), &names, &empty)?
    }
    for d in &mut ast.types {
        let id = names.get(&d.name).expect("registered nominal type").clone();
        let local_names = resolve_generic_parameters(&mut d.generics, &names, &id)?;
        let names = &local_names;
        let receiver_type_arguments = d
            .generics
            .iter()
            .map(|p| Type::parameter(p.name.clone()))
            .collect::<Vec<_>>();
        let ty = Type::named(&id, receiver_type_arguments.clone());
        let mut fields = BTreeMap::new();
        let mut methods = BTreeMap::new();
        let mut private_methods = vec![];
        for f in &mut d.fields {
            resolve_type(&mut f.ty, names)?;
            if let Some(e) = &mut f.default {
                normalize_expression(e, names, &empty)?
            }
            fields.insert(
                f.name.clone(),
                FieldDef {
                    ty: f.ty.clone(),
                    mutable: f.mutable,
                    private: f.private,
                },
            );
        }
        if d.kind == "data" {
            let members = d
                .fields
                .iter()
                .map(|f| f.name.clone())
                .chain(
                    d.methods
                        .iter()
                        .map(|f| f.name.trim_start_matches("$private$").to_string()),
                )
                .chain(std::iter::once("copy".to_string()))
                .collect::<BTreeSet<_>>();
            let mut method_counts = BTreeMap::<String, usize>::new();
            for method in &d.methods {
                *method_counts
                    .entry(method.name.trim_start_matches("$private$").into())
                    .or_default() += 1;
            }
            // The type catalog retains signatures; lowering owns each body once.
            for (index, mut method) in std::mem::take(&mut d.methods).into_iter().enumerate() {
                let private = method.name.starts_with("$private$");
                let member = method.name.trim_start_matches("$private$").to_string();
                normalize_function(&mut method, names, &members)?;
                method.name = if method_counts[&member] > 1 {
                    format!("$type:{}:{member}#overload:{index}", d.name)
                } else {
                    format!("$type:{}:{member}", d.name)
                };
                let sig = crate::compiler::signature(&method, format!("{module}.{}", method.name));
                if fields.contains_key(&member) {
                    return Err("duplicate member".into());
                }
                if private {
                    private_methods.push(sig.id.clone())
                }
                let overloads = methods.entry(member).or_insert_with(Vec::new);
                if overloads
                    .iter()
                    .any(|other| crate::compiler::same_parameter_signature(other, &sig))
                {
                    return Err(
                        "duplicate method parameter signature; return types cannot distinguish overloads".into(),
                    );
                }
                overloads.push(sig);
                ast.functions.push(method);
            }
            ast.functions.push(generate_constructor(
                d,
                ConstructorSpec {
                    name: d.name.clone(),
                    result: ty.clone(),
                    generics: d.generics.clone(),
                    tag: None,
                    defaults: ConstructorDefaults::Declared,
                },
            ));
            if fields.contains_key("copy") {
                return Err("copy is generated and conflicts with the field name".into());
            }
            let copy = generate_constructor(
                d,
                ConstructorSpec {
                    name: format!("$type:{}:copy", d.name),
                    result: ty.clone(),
                    generics: vec![],
                    tag: None,
                    defaults: ConstructorDefaults::CurrentFields,
                },
            );
            if methods
                .insert(
                    "copy".into(),
                    vec![crate::compiler::signature(&copy, format!("{module}.{}", copy.name))],
                )
                .is_some()
            {
                return Err("copy is generated and cannot be redeclared".into());
            }
            ast.functions.push(copy);
        }
        for v in &mut d.variants {
            for f in &mut v.fields {
                resolve_type(&mut f.ty, names)?;
                if let Some(e) = &mut f.default {
                    normalize_expression(e, names, &empty)?
                }
            }
            let variant_id = format!("{id}.{}", v.name);
            let variant_ty = if d.kind == "enum" {
                ty.clone()
            } else {
                Type::named(&variant_id, receiver_type_arguments.clone())
            };
            ast.functions.push(generate_constructor(
                v,
                ConstructorSpec {
                    name: format!("{}.{}", d.name, v.name),
                    result: variant_ty.clone(),
                    generics: d.generics.clone(),
                    tag: Some(v.name.clone()),
                    defaults: ConstructorDefaults::Declared,
                },
            ));
            if d.kind != "enum" {
                types.insert(
                    variant_id,
                    Arc::new(TypeDef {
                        parameters: d.generics.clone(),
                        field_order: v.fields.iter().map(|f| f.name.clone()).collect(),
                        kind: "variant".into(),
                        fields: v
                            .fields
                            .iter()
                            .map(|f| {
                                (
                                    f.name.clone(),
                                    FieldDef {
                                        ty: f.ty.clone(),
                                        mutable: false,
                                        private: false,
                                    },
                                )
                            })
                            .collect(),
                        methods: BTreeMap::new(),
                        private_methods: vec![],
                        parent: Some(ty.clone()),
                        variants: vec![],
                    }),
                );
            }
        }
        types.insert(
            id,
            Arc::new(TypeDef {
                parameters: d.generics.clone(),
                field_order: d.fields.iter().map(|f| f.name.clone()).collect(),
                kind: d.kind.clone(),
                fields,
                methods,
                private_methods,
                parent: None,
                variants: d
                    .variants
                    .iter()
                    .map(|v| format!("{}.{}", names.get(&d.name).expect("registered nominal type"), v.name))
                    .collect(),
            }),
        );
    }
    install(types.clone());
    crate::generics::validate_installed_bounds()?;
    Ok((ast, types))
}
