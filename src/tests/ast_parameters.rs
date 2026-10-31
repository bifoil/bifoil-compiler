//! Parsed headers can be reused without leaking normalized types or defaults.

use crate::model::Type;
use crate::syntax::Expr;
use crate::{compiler, parser};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

#[test]
fn compiling_one_parsed_module_twice_keeps_parameter_normalization_isolated() {
    let source = r#"data class Record:
    val number: Int = 3
    fun read(amount: Int = number) -> Int = amount
fun pick(value: Record = Record()) -> Record = value
"#;
    let ast = parser::parse(source, "src/original.bfl").unwrap();
    let parameters = ast.functions[0].params.clone();
    for name in ["first", "second"] {
        let file = format!("src/{name}.bfl");
        let module = compiler::compile(ast.clone(), name, &file, &BTreeMap::new(), Instant::now()).unwrap();
        assert_eq!(
            module.functions[&format!("{name}.pick")].signature.params[0].ty,
            Type::named(&format!("{name}.Record"), vec![])
        );
        assert!(Arc::ptr_eq(&parameters, &ast.functions[0].params));
        assert_eq!(parameters[0].ty, Type::named("Record", vec![]));
        assert!(matches!(
            &ast.types[0].methods[0].params[0].default,
            Some(Expr::Name(name)) if name == "number"
        ));
    }
}
