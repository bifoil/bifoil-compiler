//! Imported nominal boundaries are checked again after the full graph is known.

use super::Project;
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn source_free_enum_bridges_reject_a_resigned_non_enum_dependency_before_replacing_output() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\nlegacy_path = \"legacy\"\n");
    fs::create_dir(project.0.join("legacy")).unwrap();
    project.write(
        "legacy/questions.txt",
        "*label echo\n*params requested\n*set answer requested\n*return\n",
    );
    project.write(
        "src/types.bfl",
        r#"export enum Mood:
    case Happy
    case Sad
export data class Record:
    val number: Int
"#,
    );
    project.write(
        "src/bridge.bfl",
        r#"import types.Mood
export legacy fun echo(value: Mood) -> Mood = "questions" label "echo" result "answer"
"#,
    );
    project.write(
        "src/main.bfl",
        r#"import types.Mood
import bridge.echo
scene start():
    val result = echo(Mood.Happy)
"#,
    );
    for name in ["types", "bridge"] {
        let source = format!("src/{name}.bfl");
        let artifact = format!("modules/{name}.bfm.json");
        let output = project.run(&[
            "module",
            &source,
            "--name",
            name,
            "--out",
            &artifact,
            "--module-path",
            "modules",
        ]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        fs::remove_file(project.0.join(source)).unwrap();
    }
    let arguments = ["build", ".", "--out", "startup.txt"];
    let good = project.run(&arguments);
    assert!(good.status.success(), "{}", String::from_utf8_lossy(&good.stderr));
    let previous_output = fs::read(project.0.join("startup.txt")).unwrap();

    let path = project.0.join("modules/bridge.bfm.json");
    let original = fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&original).unwrap();
    let previous_digest = parsed["digest"].as_str().unwrap();
    // Compact artifact text already has canonical typed field order. Change all
    // corresponding function/export types, then sign that canonical byte stream.
    let changed = original.replace("\"name\":\"types.Mood\"", "\"name\":\"types.Record\"");
    assert_ne!(changed, original);
    let canonical = changed.replacen(&format!("\"digest\":\"{previous_digest}\""), "\"digest\":\"\"", 1);
    let digest = Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    fs::write(
        &path,
        canonical.replacen("\"digest\":\"\"", &format!("\"digest\":\"{digest}\""), 1),
    )
    .unwrap();
    project.write(
        "src/main.bfl",
        r#"import types.Record
import bridge.echo
scene start():
    val result = echo(Record(number = 1))
"#,
    );
    let bad = project.run(&arguments);
    assert!(!bad.status.success());
    let diagnostic = String::from_utf8_lossy(&bad.stderr);
    assert!(diagnostic.contains("legacy interfaces"), "{diagnostic}");
    assert!(
        !diagnostic.contains("digest mismatch"),
        "the test must reach nominal boundary checking: {diagnostic}"
    );
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), previous_output);
    let bad_module = project.run(&[
        "module",
        "src/main.bfl",
        "--name",
        "main",
        "--out",
        "modules/client.bfm.json",
        "--module-path",
        "modules",
    ]);
    assert!(!bad_module.status.success());
    assert!(String::from_utf8_lossy(&bad_module.stderr).contains("legacy interfaces"));
    assert!(!project.0.join("modules/client.bfm.json").exists());
}
