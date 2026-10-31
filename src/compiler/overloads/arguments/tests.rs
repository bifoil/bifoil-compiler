use super::{Form, omitted_defaults, positions};
use crate::model::{Param, SignatureData, Type};
use std::sync::Arc;

#[test]
fn wide_named_calls_keep_source_order_and_declaration_order_defaults() {
    let signature = Arc::new(SignatureData {
        generics: vec![],
        id: "main.Record".into(),
        params: (0..2048)
            .map(|index| Param {
                name: format!("field{index:04}"),
                ty: Type::Int,
                mutable: false,
                default: true,
                vararg: false,
            })
            .collect(),
        result: Type::Unit,
        suspend: false,
        scene: false,
    });
    let supplied: Vec<_> = signature
        .params
        .iter()
        .rev()
        .step_by(2)
        .map(|parameter| Form {
            name: Some(parameter.name.as_str()),
            spread: false,
        })
        .collect();
    let actual = positions(&signature, supplied).unwrap();
    assert_eq!(actual, (0..2048).rev().step_by(2).collect::<Vec<_>>());
    assert_eq!(
        omitted_defaults(&signature, &actual),
        (0..2048).step_by(2).collect::<Vec<_>>()
    );
    let duplicate = positions(
        &signature,
        [
            Form {
                name: Some("field0000"),
                spread: false,
            },
            Form {
                name: Some("field0000"),
                spread: false,
            },
        ],
    );
    assert_eq!(duplicate.unwrap_err(), "duplicate argument");
    let unknown = positions(
        &signature,
        [Form {
            name: Some("missing"),
            spread: false,
        }],
    );
    assert_eq!(unknown.unwrap_err(), "unknown argument 'missing'");
}
