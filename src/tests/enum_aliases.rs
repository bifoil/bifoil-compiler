//! Transparent enum aliases retain case identity and scope-wide visibility.

use super::compile;
use crate::model::Type;

#[test]
fn top_level_enum_aliases_preserve_qualified_case_values_in_functions() {
    let module = compile(
        r#"typealias Color = Shade
typealias Shade = Mood
enum Mood:
    Happy
    Sad
fun selected() = Color.Happy
scene start():
    val factory: () -> Mood = () => Shade.Sad
    val first = selected()
    val second = factory()
"#,
    )
    .unwrap();
    assert_eq!(
        module.functions["test.selected"].signature.result,
        Type::named("test.Mood", vec![])
    );
}

#[test]
fn local_enum_aliases_are_visible_before_declaration_and_inside_inferred_functions() {
    let module = compile(
        r#"enum Mood:
    Happy
    Sad
fun select():
    fun choose() = Local.Happy
    val selected = choose()
    typealias Local = Mood
    return selected
fun retained() -> () -> Mood:
    typealias Local = Mood
    return () => Local.Sad
fun blocked() = if true:
    val selected = Local.Happy
    typealias Local = Mood
    selected
else:
    Mood.Sad
scene start():
    val result = retained()()
"#,
    )
    .unwrap();
    for name in ["select", "blocked"] {
        assert_eq!(
            module.functions[&format!("test.{name}")].signature.result,
            Type::named("test.Mood", vec![])
        );
    }
}
