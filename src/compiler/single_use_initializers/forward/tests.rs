use super::*;
use crate::model::{Op, Type};
use std::collections::BTreeMap;
use std::time::Instant;

fn fixture() -> (Frame, Function) {
    let ast = crate::parser::parse("fun answer(value: Int = 3) -> Int = value + 1\n", "src/guard.bfl").unwrap();
    let frame = Frame::new(&ast.functions[0], Arc::new(Vec::new()));
    let module = crate::compiler::compile(ast, "guard", "src/guard.bfl", &BTreeMap::new(), Instant::now()).unwrap();
    (frame, module.functions["guard.answer"].clone())
}

#[test]
fn final_code_identity_ignores_allocations_and_rejects_changed_instructions() {
    let (mut frame, function) = fixture();
    frame.verify(&function).unwrap();
    let mut rebuilt: Function = serde_json::from_slice(&serde_json::to_vec(&function).unwrap()).unwrap();
    frame.verify(&rebuilt).unwrap();
    let constant = rebuilt
        .code
        .iter_mut()
        .find_map(|instruction| {
            if let Op::Constant { value, .. } = &mut instruction.op {
                Some(value)
            } else {
                None
            }
        })
        .unwrap();
    *constant = serde_json::json!(7);
    let error = frame.verify(&rebuilt).unwrap_err();
    assert!(error.contains("final declaration environment"), "{error}");
}

#[test]
fn final_code_identity_rejects_a_changed_signature_with_the_same_body() {
    let (mut frame, mut function) = fixture();
    frame.verify(&function).unwrap();
    Arc::make_mut(&mut function.signature).result = Type::Float;
    let error = frame.verify(&function).unwrap_err();
    assert!(error.contains("final declaration environment"), "{error}");
}
