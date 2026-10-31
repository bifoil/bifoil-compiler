//! Generated field constructors and shallow copy functions share one implementation.

use crate::model::{GenericParam, Type};
use crate::syntax::{Expr, FunctionAst, Parameter, Statement, StatementKind, TypeAst};

pub(super) enum ConstructorDefaults {
    Declared,
    CurrentFields,
}

pub(super) struct ConstructorSpec {
    pub(super) name: String,
    pub(super) result: Type,
    pub(super) generics: Vec<GenericParam>,
    pub(super) tag: Option<String>,
    pub(super) defaults: ConstructorDefaults,
}

pub(super) fn generate_constructor(declaration: &TypeAst, spec: ConstructorSpec) -> FunctionAst {
    let params = declaration
        .fields
        .iter()
        .map(|field| Parameter {
            name: field.name.clone(),
            ty: field.ty.clone(),
            mutable: false,
            vararg: false,
            default: match spec.defaults {
                ConstructorDefaults::Declared => field.default.clone(),
                ConstructorDefaults::CurrentFields => Some(Expr::Member {
                    receiver: Box::new(Expr::Name("this".into())),
                    name: field.name.clone(),
                    safe: false,
                }),
            },
        })
        .collect();
    let fields = declaration
        .fields
        .iter()
        .map(|field| (field.name.clone(), Expr::Name(field.name.clone())))
        .collect();
    FunctionAst {
        legacy: None,
        name: spec.name,
        generics: spec.generics,
        params,
        result: Some(spec.result.clone()),
        suspend: false,
        scene: false,
        exported: false,
        expression: true,
        line: declaration.line,
        body: vec![Statement {
            line: declaration.line,
            kind: StatementKind::Return(Some(Expr::Construct {
                ty: spec.result,
                fields,
                tag: spec.tag,
            })),
        }],
    }
}
