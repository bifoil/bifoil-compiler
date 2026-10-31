//! Metadata sharing preserves detached edits and the serialized type contract.

use crate::model::{Param, SignatureData, Type};
use std::sync::Arc;

fn empty() -> SignatureData {
    serde_json::from_str(r#"{"id":"sample.first","params":[],"result":{"kind":"int"},"suspend":false,"scene":false}"#)
        .unwrap()
}

#[test]
fn derived_types_and_header_edits_remain_independent() {
    let mut first = empty();
    let second = empty();
    let before = serde_json::to_vec(&second).unwrap();
    let original = first.ty();
    let mut detached = first.ty();
    let Type::Function { result, .. } = &mut detached else {
        panic!("ordinary function")
    };
    *Arc::make_mut(result) = Type::Float;
    assert_ne!(detached, original);
    assert_eq!(second.ty(), original);
    first.result = Type::String;
    first.params = Arc::from([Param {
        name: "value".into(),
        ty: Type::Int,
        mutable: false,
        default: false,
        vararg: false,
    }]);
    first.params_mut()[0].ty = Type::Char;
    let Type::Function { params, result, .. } = first.ty() else {
        panic!("changed ordinary function")
    };
    assert_eq!(params.as_ref(), &[Type::Char]);
    assert_eq!(result.as_ref(), &Type::String);
    assert_eq!(second.ty(), original);
    assert_eq!(serde_json::to_vec(&second).unwrap(), before);
    let restored: SignatureData = serde_json::from_slice(&serde_json::to_vec(&first).unwrap()).unwrap();
    assert_eq!(restored.ty(), first.ty());
}

#[test]
fn metadata_decoding_preserves_variadic_scene_flags_and_rejects_invalid_shapes() {
    let source = r#"{"id":"sample.varargs","params":[{"name":"values","ty":{"kind":"int"},"mutable":true,"default":false,"vararg":true}],"result":{"kind":"string"},"suspend":true,"scene":false}"#;
    let mut decoded: SignatureData = serde_json::from_str(source).unwrap();
    let Type::Function {
        params,
        result,
        suspend,
        vararg,
    } = decoded.ty()
    else {
        panic!("variadic function")
    };
    assert_eq!(params.as_ref(), &[Type::Int]);
    assert_eq!(result.as_ref(), &Type::String);
    assert!(suspend && vararg);
    let canonical = serde_json::to_vec(&decoded).unwrap();
    let restored: SignatureData = serde_json::from_slice(&canonical).unwrap();
    assert_eq!(serde_json::to_vec(&restored).unwrap(), canonical);
    decoded.scene = true;
    assert_eq!(decoded.ty(), Type::Scene { params });
    assert_ne!(empty().ty(), decoded.ty());
    for invalid in [
        source.replace("\"params\":[", "\"params\":{} ,\"extra\":["),
        source.replace("\"kind\":\"int\"", "\"kind\":\"not_a_type\""),
    ] {
        assert!(serde_json::from_str::<SignatureData>(&invalid).is_err());
    }
    for invalid in [
        r#"{"kind":"named","name":"Box","args":{}}"#,
        r#"{"kind":"scene","params":{}}"#,
        r#"{"kind":"function","params":[],"result":{},"suspend":false}"#,
    ] {
        assert!(serde_json::from_str::<Type>(invalid).is_err());
    }
}
