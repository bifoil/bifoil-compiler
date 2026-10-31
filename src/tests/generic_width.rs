//! Wide generic declarations retain their bounds and complete signatures.

use super::compile;

#[test]
fn wide_generics_cover_functions_data_aliases_and_variants() {
    let names = (0..512).map(|index| format!("T{index:05}")).collect::<Vec<_>>();
    let parameters = names
        .iter()
        .map(|name| format!("{name}: StableKey"))
        .collect::<Vec<_>>()
        .join(", ");
    let arguments = names.join(", ");
    let concrete = vec!["Int"; names.len()].join(", ");
    let mut source = format!("data class Box<{parameters}>:\n");
    for (index, name) in names.iter().enumerate() {
        source.push_str(&format!("    val field{index:05}: {name}\n"));
    }
    source.push_str(&format!("typealias Payload<{parameters}> = Box<{arguments}>\n"));
    source.push_str(&format!("sealed class Outcome<{parameters}>:\n"));
    source.push_str(&format!(
        "    case Ready(value: Payload<{arguments}>)\n    case Pending\n"
    ));
    source.push_str(&format!("fun <{parameters}> wrap(\n    value: Payload<{arguments}>\n"));
    source.push_str(&format!(") -> Outcome<{arguments}>:\n"));
    source.push_str(&format!("    return Outcome.Ready<{arguments}>(value)\n"));
    source.push_str(&format!("scene start():\n    val input = Box<{concrete}>(\n"));
    for index in 0..512 {
        source.push_str(&format!("        {index},\n"));
    }
    source.push_str(&format!("    )\n    val result = wrap<{concrete}>(input)\n"));
    let module = compile(&source).unwrap();
    assert_eq!(module.functions["test.wrap"].signature.generics.len(), 512);
}

#[test]
fn duplicate_generic_names_remain_errors_after_many_distinct_parameters() {
    let mut names = (0..64).map(|index| format!("T{index:05}")).collect::<Vec<_>>();
    names.push(names[0].clone());
    let source = format!("fun <{}> repeat() = Unit\nscene start():\n    Unit\n", names.join(", "));
    assert!(compile(&source).err().unwrap().contains("duplicate type parameter"));
}

#[test]
fn a_late_generic_argument_must_satisfy_its_declared_capability() {
    let names = (0..64).map(|index| format!("T{index:05}")).collect::<Vec<_>>();
    let parameters = names
        .iter()
        .map(|name| format!("{name}: StableKey"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut arguments = vec!["Int"; names.len()];
    *arguments.last_mut().unwrap() = "MutableList<Int>";
    let source = format!(
        "fun <{parameters}> requireKey(value: T00063) = Unit\nscene start():\n    requireKey<{}>(mutable [1])\n",
        arguments.join(", "),
    );
    assert!(compile(&source).err().unwrap().contains("StableKey"));
}
