//! Isolate return collection; final expression checking separately validates operand types.
use super::*;
use crate::syntax::TemplatePart;

fn returning(value: Expr) -> Expr {
    Expr::Transfer {
        kind: "return".into(),
        label: None,
        value: Some(Box::new(value)),
    }
}

#[test]
fn earlier_operand_returns_exclude_later_return_types_from_the_flow_proof() {
    let first = returning(Expr::Int(7));
    let late = returning(Expr::String("Retired operand.".into()));
    for value in [
        Expr::Call(Box::new(first.clone()), vec![(None, late.clone())], 1),
        Expr::Index {
            receiver: Box::new(first.clone()),
            index: Box::new(late.clone()),
            safe: false,
            line: 1,
        },
        Expr::Unwrap(Box::new(first.clone()), 1),
        Expr::Collection {
            family: Some("Map".into()),
            mutable: false,
            entries: vec![(Some(first.clone()), late.clone())],
        },
        Expr::Collection {
            family: Some("List".into()),
            mutable: false,
            entries: vec![(None, first.clone()), (None, late.clone())],
        },
        Expr::Construct {
            ty: Type::named("Original", vec![]),
            fields: vec![("first".into(), first.clone()), ("second".into(), late.clone())],
            tag: None,
        },
        Expr::Format(vec![
            TemplatePart { value: first, line: 1 },
            TemplatePart { value: late, line: 1 },
        ]),
    ] {
        let mut analysis = Inference::default();
        let mut scope = vec![Arc::default()];
        assert!(!collect(&value, &mut scope, &mut analysis).unwrap(), "{value:?}");
        assert_eq!(analysis.types.len(), 1, "{value:?}");
        assert_eq!(analysis.types[0].ty, Type::Int, "{value:?}");
    }
}

#[test]
fn unit_context_discards_returned_values_but_preserves_divergence() {
    let expected = Type::Unit;
    let mut analysis = Inference {
        expected_return: Some(&expected),
        ..Default::default()
    };
    returned(Some(&returning(Expr::Int(7))), &mut vec![Arc::default()], &mut analysis).unwrap();
    assert_eq!(
        analysis.types.iter().map(|value| &value.ty).collect::<Vec<_>>(),
        [&Type::Unit, &Type::Nothing]
    );
    let mut flow = Inference {
        flow_only: true,
        ..Default::default()
    };
    returned(None, &mut vec![Arc::default()], &mut flow).unwrap();
    assert!(flow.types.is_empty());
}
