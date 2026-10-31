//! Graph composition and artifact compatibility must fail before root publication.

use super::Project;
use std::fs;

fn success(project: &Project, arguments: &[&str]) {
    let result = project.run(arguments);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
}

#[test]
fn cross_kind_runtime_collisions_reject_sources_and_independent_artifacts() {
    for storage in [
        "export shared val C: Int = 7\n",
        "export legacy var C: Int = \"count\"\n",
    ] {
        for packaged in [false, true] {
            let project = Project::new();
            fs::create_dir(project.0.join("src/a")).unwrap();
            fs::create_dir(project.0.join("modules/a")).unwrap();
            project.write("bifoil.toml", "entry = \"main.start\"\n");
            project.write(
                "src/a.bfl",
                r#"export sealed class B:
    case C(value: Int)
"#,
            );
            project.write("src/a/B.bfl", storage);
            project.write(
                "src/main.bfl",
                r#"scene start():
    Unit
"#,
            );
            let mut arguments = vec!["build", ".", "--out", "startup.txt", "--clean"];
            if packaged {
                arguments.push("--package");
            }
            success(&project, &arguments);
            let previous = fs::read(project.0.join("startup.txt")).unwrap();
            for module in ["a", "a.B"] {
                success(
                    &project,
                    &[
                        "module",
                        &format!("src/{}.bfl", module.replace('.', "/")),
                        "--name",
                        module,
                        "--out",
                        &format!("modules/{}.bfm.json", module.replace('.', "/")),
                    ],
                );
            }
            project.write(
                "src/main.bfl",
                r#"import a.B
import a.B.C as count
scene start():
    val made: B = B.C(3)
    prose:
        ${count}
"#,
            );
            for source_free in [false, true] {
                if source_free {
                    fs::remove_file(project.0.join("src/a.bfl")).unwrap();
                    fs::remove_file(project.0.join("src/a/B.bfl")).unwrap();
                }
                let result = project.run(&arguments);
                assert!(!result.status.success());
                let error = String::from_utf8_lossy(&result.stderr);
                assert!(error.contains("linked runtime identity 'a.B.C' collides"), "{error}");
                assert!(error.contains("callable in a") && error.contains("in a.B"), "{error}");
                assert_eq!(previous, fs::read(project.0.join("startup.txt")).unwrap());
            }
        }
    }
}

#[test]
fn source_cache_and_source_free_discovery_produce_identical_roots() {
    for packaged in [false, true] {
        let project = Project::new();
        project.write("bifoil.toml", "entry = \"main.start\"\n");
        project.write("src/a.bfl", "export shared val first: Int = 1\n");
        project.write("src/z.bfl", "export shared val last: Int = 2\n");
        project.write(
            "src/main.bfl",
            r#"import z.last
import a.first
scene start():
    prose:
        ${first}|${last}
"#,
        );
        let mut arguments = vec!["build", ".", "--out", "startup.txt"];
        if packaged {
            arguments.push("--package");
        }
        success(&project, &arguments);
        let expected = fs::read(project.0.join("startup.txt")).unwrap();
        for receipt in ["missing", "corrupt"] {
            let path = project.0.join(".bifoil/cache/v1/completed.json");
            if receipt == "missing" {
                fs::remove_file(path).unwrap();
            } else {
                fs::write(path, "broken receipt").unwrap();
            }
            success(&project, &arguments);
            assert_eq!(expected, fs::read(project.0.join("startup.txt")).unwrap());
        }
        let mut clean = arguments.clone();
        clean.push("--clean");
        success(&project, &clean);
        assert_eq!(expected, fs::read(project.0.join("startup.txt")).unwrap());
        for module in ["a", "z"] {
            success(
                &project,
                &[
                    "module",
                    &format!("src/{module}.bfl"),
                    "--name",
                    module,
                    "--out",
                    &format!("modules/{module}.bfm.json"),
                ],
            );
            fs::remove_file(project.0.join(format!("src/{module}.bfl"))).unwrap();
        }
        success(&project, &clean);
        assert_eq!(expected, fs::read(project.0.join("startup.txt")).unwrap());
    }
}

#[test]
fn old_typed_artifacts_are_rejected_without_replacing_completed_output() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    project.write("src/helpers.bfl", "export fun value() -> Int = 1\n");
    project.write(
        "src/main.bfl",
        r#"import helpers.value
scene start():
    val number = value()
"#,
    );
    success(
        &project,
        &[
            "module",
            "src/helpers.bfl",
            "--name",
            "helpers",
            "--out",
            "modules/helpers.bfm.json",
        ],
    );
    let arguments = ["build", ".", "--out", "startup.txt", "--clean"];
    success(&project, &arguments);
    let previous = fs::read(project.0.join("startup.txt")).unwrap();
    fs::remove_file(project.0.join("src/helpers.bfl")).unwrap();
    let path = project.0.join("modules/helpers.bfm.json");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("bifoil.module.v3"));
    fs::write(path, text.replace("bifoil.module.v3", "bifoil.module.v2")).unwrap();
    let result = project.run(&arguments);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("recompile"));
    assert_eq!(previous, fs::read(project.0.join("startup.txt")).unwrap());
}
