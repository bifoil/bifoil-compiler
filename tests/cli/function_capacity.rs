//! Production code-table capacity, including every retained callable reference.

use super::Project;
use std::fs;

#[test]
fn full_function_table_compiles_and_rejects_the_next_declaration() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    let mut source = String::new();
    for index in 0..16383 {
        source.push_str(&format!("fun f{index:05}() -> Int = {index}\n"));
    }
    source.push_str("scene start():\n");
    let mut batches = Vec::new();
    for start in (0..16383).step_by(1024) {
        let name = format!("batch{}", batches.len());
        source.push_str(&format!("    val {name} = [\n"));
        for index in start..(start + 1024).min(16383) {
            source.push_str(&format!("        ::f{index:05},\n"));
        }
        source.push_str("    ]\n");
        batches.push(name);
    }
    source.push_str(&format!("    val batches = [{}]\n", batches.join(", ")));
    source.push_str(
        "    var total = 0\n    for batch in batches:\n        for routine in batch:\n            total += routine()\n    prose:\n        ${total}\n",
    );
    let mut completed = String::new();
    for inferred in [false, true] {
        let text = if inferred {
            source.replace("() -> Int =", "() =")
        } else {
            source.clone()
        };
        project.write("src/main.bfl", &text);
        let built = project.run(&["build", ".", "--out", "startup.txt", "--clean"]);
        assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
        let current = fs::read_to_string(project.0.join("startup.txt")).unwrap();
        if inferred {
            assert_eq!(completed, current, "return inference changed the compiled program");
        }
        completed = current;
    }
    let program: serde_json::Value = serde_json::from_str(
        completed
            .lines()
            .find_map(|line| line.strip_prefix("*bifoil "))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(program["functions"].as_object().unwrap().len(), 16384);
    assert_eq!(program["entry"], "main.start");
    source.push_str("fun excess() -> Int = 0\n");
    project.write("src/main.bfl", &source);
    let rejected = project.run(&["build", ".", "--out", "startup.txt", "--clean"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("function budget exceeded"));
    assert_eq!(completed, fs::read_to_string(project.0.join("startup.txt")).unwrap());
}

#[test]
fn wide_named_function_parameters_use_the_existing_source_and_code_guards() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    let parameters = (0..2048)
        .map(|index| format!("    p{index:05}: Int = {index}"))
        .collect::<Vec<_>>()
        .join(",\n");
    let mut source = format!("fun total(\n{parameters}\n) -> Int:\n    val values = [\n");
    for index in 0..2048 {
        source.push_str(&format!("        p{index:05},\n"));
    }
    source.push_str(
        r#"    ]
    var result = 0
    for item in values:
        result += item
    return result
scene start():
    val result = total(p02047 = 2048)
    prose:
        ${result}
"#,
    );
    project.write("src/main.bfl", &source);
    let built = project.run(&["build", ".", "--out", "startup.txt", "--clean"]);
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let completed = fs::read_to_string(project.0.join("startup.txt")).unwrap();
    let program: serde_json::Value = serde_json::from_str(
        completed
            .lines()
            .find_map(|line| line.strip_prefix("*bifoil "))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        program["functions"]["main.total"]["signature"]["params"]
            .as_array()
            .unwrap()
            .len(),
        2048
    );
    let checked = project.run(&["format", "src/main.bfl", "--check"]);
    assert!(checked.status.success(), "{}", String::from_utf8_lossy(&checked.stderr));
}

#[test]
fn source_free_modules_can_exceed_the_individual_function_table_limit() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    let declarations: String = (0..10000)
        .map(|index| format!("export fun f{index:05}() -> Int = {index}\n"))
        .collect();
    for name in ["first", "second"] {
        let source = format!("src/{name}.bfl");
        let artifact = format!("modules/{name}.bfm.json");
        project.write(&source, &declarations);
        let published = project.run(&["module", &source, "--name", name, "--out", &artifact]);
        assert!(
            published.status.success(),
            "{}",
            String::from_utf8_lossy(&published.stderr)
        );
        fs::remove_file(project.0.join(source)).unwrap();
    }
    project.write("src/main.bfl", "import first.f00000\nimport second.f00000 as other\nscene start():\n    prose:\n        ${f00000()}|${other()}\n");
    let built = project.run(&["build", ".", "--clean", "--package", "--out", "startup.txt"]);
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let mut total = 0;
    for entry in fs::read_dir(&project.0).unwrap() {
        let path = entry.unwrap().path();
        if !path.file_name().unwrap().to_string_lossy().starts_with("bifoil_code_") {
            continue;
        }
        let source = fs::read_to_string(path).unwrap();
        let json = source
            .lines()
            .find_map(|line| line.strip_prefix("*bifoil_module "))
            .unwrap();
        let chunk: serde_json::Value = serde_json::from_str(json).unwrap();
        let count = chunk["functions"].as_object().unwrap().len();
        assert!(count <= 16384);
        total += count;
    }
    assert_eq!(total, 20001);
}

#[test]
fn generated_functions_cannot_push_a_module_past_its_independent_limit() {
    let project = Project::new();
    let data = "data class Token:\n    val value: Int = 0\n";
    project.write("src/bounds.bfl", data);
    let command = [
        "module",
        "src/bounds.bfl",
        "--name",
        "bounds",
        "--out",
        "modules/bounds.bfm.json",
    ];
    let published = project.run(&command);
    assert!(
        published.status.success(),
        "{}",
        String::from_utf8_lossy(&published.stderr)
    );
    let previous = fs::read(project.0.join("modules/bounds.bfm.json")).unwrap();
    let module: serde_json::Value = serde_json::from_slice(&previous).unwrap();
    let helpers = module["functions"].as_object().unwrap().len();
    assert!(helpers > 0);
    let mut source: String = (0..16385 - helpers)
        .map(|index| format!("fun f{index:05}() -> Int = {index}\n"))
        .collect();
    source.push_str(data);
    project.write("src/bounds.bfl", &source);
    let rejected = project.run(&command);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("module function budget exceeded"));
    assert_eq!(fs::read(project.0.join("modules/bounds.bfm.json")).unwrap(), previous);
}
