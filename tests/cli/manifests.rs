use super::Project;
use std::fs;

#[test]
fn standard_toml_syntax_preserves_source_free_builds_and_semantic_identity() {
    let project = Project::new();
    project.write("src/helpers.bfl", "export fun answer(value: Int) -> Int = value + 1\n");
    project.write(
        "src/main.bfl",
        "import helpers.answer\nscene start():\n    prose:\n        ${answer(6)}\n",
    );
    fs::create_dir_all(project.0.join("compiled modules #1")).unwrap();
    let compiled = project.run(&[
        "module",
        "src/helpers.bfl",
        "--name",
        "helpers",
        "--out",
        "compiled modules #1/helpers.bfm.json",
    ]);
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    fs::remove_file(project.0.join("src/helpers.bfl")).unwrap();

    let manifests = [
        "entry = \"main.start\"\nmodule_path = \"compiled modules #1\"\n",
        "\"entry\" = 'main.start' # entry scene\r\n'module_path' = 'compiled modules #1'\r\n",
        r#"entry = """main.\
    start"""
module_path = '''compiled modules #1'''
"#,
        r#"entry = "ma\u0069n.start"
module_path = "compiled modules #1" # hash inside the value remains literal
"#,
    ];
    let mut expected = None;
    for manifest in manifests {
        project.write("bifoil.toml", manifest);
        let output = project.run(&["build", ".", "--out", "startup.txt"]);
        assert!(
            output.status.success(),
            "{}\n{manifest}",
            String::from_utf8_lossy(&output.stderr)
        );
        let bytes = fs::read(project.0.join("startup.txt")).unwrap();
        if let Some(previous) = &expected {
            assert_eq!(&bytes, previous, "equivalent TOML must not change the compiled story");
        } else {
            let source = std::str::from_utf8(&bytes).unwrap();
            let encoded = source.lines().find_map(|line| line.strip_prefix("*bifoil ")).unwrap();
            let program: serde_json::Value = serde_json::from_str(encoded).unwrap();
            assert_eq!(program["entry"], "main.start");
            assert!(
                !program["functions"]["helpers.answer"]["code"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            expected = Some(bytes);
        }
    }
}

#[test]
fn invalid_manifests_report_the_file_and_preserve_completed_output() {
    let project = Project::new();
    project.write("src/main.bfl", "scene start():\n    prose:\n        Ready.\n");
    project.write("bifoil.toml", "entry = 'main.start'\n");
    let args = ["build", ".", "--out", "startup.txt"];
    let output = project.run(&args);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let expected = fs::read(project.0.join("startup.txt")).unwrap();

    for (manifest, diagnostic) in [
        ("entry = 'main.start'\nentry = 'other.start'\n", "duplicate key"),
        ("entry = 'main.start'\nextra = 'ignored'\n", "unknown field"),
        ("entry = 3\n", "invalid type"),
        ("module_path = 'modules'\n", "entry"),
        ("entry = 'invalid'\n", "entry must be module.scene"),
    ] {
        project.write("bifoil.toml", manifest);
        let output = project.run(&args);
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "invalid manifest accepted: {manifest}");
        assert!(error.contains("bifoil.toml") && error.contains(diagnostic), "{error}");
        assert_eq!(expected, fs::read(project.0.join("startup.txt")).unwrap());
    }
    project.write(
        "bifoil.toml",
        &format!("entry = 'main.start'\n#{}\n", "x".repeat(16 * 1024)),
    );
    let output = project.run(&args);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("budget exceeded"));
    assert_eq!(expected, fs::read(project.0.join("startup.txt")).unwrap());
}
