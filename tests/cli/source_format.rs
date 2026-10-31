use super::Project;
use std::fs;

#[test]
fn source_formatting_has_clean_stdout_check_mode_and_atomic_output() {
    let project = Project::new();
    let original = "scene start():\n\tUnit\n";
    let expected = "scene start():\n    Unit\n";
    project.write("src/main.bfl", original);
    let result = project.run(&["format", "src/main.bfl"]);
    assert!(result.status.success());
    assert_eq!(String::from_utf8(result.stdout).unwrap(), expected);
    let check = project.run(&["format", "src/main.bfl", "--check"]);
    assert!(!check.status.success());
    assert!(String::from_utf8_lossy(&check.stderr).contains("formatting differs"));
    let write = project.run(&["format", "src/main.bfl", "--out", "formatted.bfl"]);
    assert!(write.status.success(), "{}", String::from_utf8_lossy(&write.stderr));
    assert!(write.stdout.is_empty());
    assert_eq!(fs::read_to_string(project.0.join("src/main.bfl")).unwrap(), original);
    assert_eq!(fs::read_to_string(project.0.join("formatted.bfl")).unwrap(), expected);
    assert!(project.run(&["format", "formatted.bfl", "--check"]).status.success());
    project.write("src/main.bfl", "scene start():\n    val unfinished =\n");
    assert!(
        !project
            .run(&["format", "src/main.bfl", "--out", "formatted.bfl"])
            .status
            .success()
    );
    assert_eq!(fs::read_to_string(project.0.join("formatted.bfl")).unwrap(), expected);
    assert!(
        !project
            .run(&["format", "formatted.bfl", "--check", "--out", "other.bfl"])
            .status
            .success()
    );
    assert!(!project.0.join("other.bfl").exists());
}
