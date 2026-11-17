//! Prose suites keep lexical mode while callable invocations own their output.
use super::compile;
use crate::model::{Op, Type};

#[test]
fn escaping_prose_functions_have_their_own_output_context() {
    let module = compile(
        r#"fun factory() -> () -> Int:
    prose:
        @fun answer() -> Int:
            From the callback.
            @return 7
        @return ::answer
"#,
    )
    .unwrap();
    let function = module
        .functions
        .values()
        .find(|function| function.signature.id.contains("answer"))
        .unwrap();
    assert!(
        function
            .code
            .iter()
            .any(|instruction| matches!(instruction.op, Op::ProseStart))
    );
    assert_eq!(function.signature.result, Type::Int);
}

#[test]
fn prose_lambdas_keep_final_expression_and_early_return_rules() {
    compile(
        r#"fun factory() -> (Int) -> Int:
    prose:
        @return (number: Int) =>:
            Value ${number}.
            @if number == 0:
                @return 2
            @number * 2
fun unitFactory() -> (Int) -> Unit:
    prose:
        @return (number: Int) =>:
            Value ${number}.
            @return number
"#,
    )
    .unwrap();
}

#[test]
fn directive_value_blocks_stay_prose_and_interpolation_blocks_use_code() {
    compile(
        r#"scene start():
    prose:
        @val score: Int = if true:
            selected
            @3
        @else:
            @4
        ${if score == 3:
            "yes"
        else:
            "no"
        }
"#,
    )
    .unwrap();
}

#[test]
fn prose_function_creation_preserves_typed_captures_and_effect_checks() {
    compile(
        r#"fun factory() -> suspend () -> Int:
    prose:
        @suspend fun ask() -> Int:
            Ready.
            @choice:
                @option "Continue":
                    Selected.
            @return 3
        @return ::ask
"#,
    )
    .unwrap();
    let error = compile(
        r#"scene start():
    prose:
        @suspend fun ask() -> Int:
            @choice:
                @option "Continue":
                    Selected.
            @return 3
        ${ask()}
"#,
    )
    .unwrap_err();
    assert!(error.contains("suspend"), "{error}");
}

#[test]
fn long_blank_runs_form_one_boundary_and_retain_physical_origins() {
    let source = format!(
        "scene start():\n    prose:\n        Alpha.\n{}        Beta.\n",
        "\n".repeat(10000)
    );
    let ast = crate::parser::parse(&source, "src/test.bfl").unwrap();
    let crate::syntax::StatementKind::Narrative(body) = &ast.functions[0].body[0].kind else {
        panic!()
    };
    assert_eq!(
        body.iter()
            .filter(|statement| matches!(statement.kind, crate::syntax::StatementKind::ParagraphBreak))
            .count(),
        1
    );
    assert_eq!(body.last().unwrap().line, 10004);
    compile(&source).unwrap();
}
