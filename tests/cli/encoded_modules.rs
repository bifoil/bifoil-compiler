use super::Project;
use std::fs;

#[test]
fn pretty_source_free_modules_preserve_identity_and_bad_bodies_preserve_output() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    project.write(
        "src/helpers.bfl",
        "export fun precise() -> Float = -1.0803725086404524e-296\n",
    );
    project.write(
        "src/main.bfl",
        "import helpers.precise\nscene start():\n    prose:\n        ${precise()}\n",
    );
    let compiled = project.run(&[
        "module",
        "src/helpers.bfl",
        "--name",
        "helpers",
        "--out",
        "modules/helpers.bfm.json",
    ]);
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    fs::remove_file(project.0.join("src/helpers.bfl")).unwrap();
    let arguments = [
        "build",
        ".",
        "--module-path",
        "modules",
        "--out",
        "startup.txt",
        "--clean",
    ];
    let first = project.run(&arguments);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let expected = fs::read(project.0.join("startup.txt")).unwrap();

    let path = project.0.join("modules/helpers.bfm.json");
    let mut module: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(&module).unwrap()).unwrap();
    let pretty = project.run(&arguments);
    assert!(pretty.status.success(), "{}", String::from_utf8_lossy(&pretty.stderr));
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), expected);

    module["functions"]["helpers.precise"]["code"][0]["op"] = "unknown-op".into();
    fs::write(&path, serde_json::to_vec_pretty(&module).unwrap()).unwrap();
    let invalid = project.run(&arguments);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("unknown variant"));
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), expected);
}
