//! Code packages preserve unchanged scenes and atomic completed roots.

use super::Project;
use serde_json::Value;
use std::fs::{self, File, FileTimes};
use std::time::{Duration, UNIX_EPOCH};

fn manifest(project: &Project) -> Value {
    let source = fs::read_to_string(project.0.join("startup.txt")).unwrap();
    let json = source.lines().find_map(|line| line.strip_prefix("*bifoil ")).unwrap();
    serde_json::from_str(json).unwrap()
}
fn setup() -> Project {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    project.write("src/first.bfl", "export fun first() -> Int = 1\n");
    project.write("src/second.bfl", "export fun second() -> Int = 2\n");
    project.write("src/main.bfl", "import first.first\nimport second.second\nscene start():\n    val answer = first() + second()\n    prose:\n        ${answer}\n");
    project
}
fn build(project: &Project) {
    let output = project.run(&["build", ".", "--out", "startup.txt", "--package"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn changed_packages_reuse_independent_and_recompiled_but_identical_code_scenes() {
    let project = setup();
    build(&project);
    let before = manifest(&project);
    assert_eq!(before["format"], "bifoil.package.v1");
    assert!(before["functions"].as_object().unwrap().is_empty());
    let fixed = UNIX_EPOCH + Duration::from_secs(1_000_000);
    for module in ["main", "second"] {
        let scene = before["chunks"][module]["scene"].as_str().unwrap();
        File::options()
            .write(true)
            .open(project.0.join(format!("{scene}.txt")))
            .unwrap()
            .set_times(FileTimes::new().set_modified(fixed))
            .unwrap();
    }
    project.write("src/first.bfl", "export fun first() -> Int = 3\n");
    build(&project);
    let after = manifest(&project);
    assert_ne!(before["build"], after["build"]);
    assert_ne!(before["chunks"]["first"]["scene"], after["chunks"]["first"]["scene"]);
    for module in ["main", "second"] {
        assert_eq!(before["chunks"][module]["scene"], after["chunks"][module]["scene"]);
        let scene = after["chunks"][module]["scene"].as_str().unwrap();
        assert_eq!(
            fs::metadata(project.0.join(format!("{scene}.txt")))
                .unwrap()
                .modified()
                .unwrap(),
            fixed
        );
    }
    assert_ne!(before["modules"]["main"], after["modules"]["main"]);
}

#[test]
fn package_rebuild_repairs_missing_or_changed_chunks_and_keeps_source_free_output() {
    let project = setup();
    build(&project);
    let before = fs::read(project.0.join("startup.txt")).unwrap();
    let root = manifest(&project);
    for module in ["first", "second"] {
        let scene = root["chunks"][module]["scene"].as_str().unwrap();
        let path = project.0.join(format!("{scene}.txt"));
        let original = fs::read(&path).unwrap();
        if module == "first" {
            fs::remove_file(&path).unwrap();
        } else {
            fs::write(&path, "corrupted generated scene").unwrap();
        }
        build(&project);
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), before);
    }
    let output = project.run(&[
        "module",
        "src/first.bfl",
        "--name",
        "first",
        "--out",
        "modules/first.bfm.json",
    ]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    fs::remove_file(project.0.join("src/first.bfl")).unwrap();
    build(&project);
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), before);
}

#[test]
fn failed_package_builds_preserve_the_completed_root_and_chunks() {
    let project = setup();
    build(&project);
    let before = fs::read(project.0.join("startup.txt")).unwrap();
    let root = manifest(&project);
    let chunks = root["chunks"]
        .as_object()
        .unwrap()
        .values()
        .map(|reference| {
            let path = project.0.join(format!("{}.txt", reference["scene"].as_str().unwrap()));
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect::<Vec<_>>();
    project.write("src/first.bfl", "export fun first() -> Int = \"wrong\"\n");
    assert!(
        !project
            .run(&["build", ".", "--out", "startup.txt", "--package"])
            .status
            .success()
    );
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), before);
    for (path, bytes) in chunks {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn package_and_inline_outputs_remain_distinct_when_switching_modes() {
    let project = setup();
    let args = ["build", ".", "--out", "startup.txt"];
    assert!(project.run(&args).status.success());
    let inline = fs::read(project.0.join("startup.txt")).unwrap();
    build(&project);
    assert_ne!(fs::read(project.0.join("startup.txt")).unwrap(), inline);
    assert!(project.run(&args).status.success());
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), inline);
    assert!(
        !project
            .run(&[
                "module",
                "src/first.bfl",
                "--name",
                "first",
                "--out",
                "bad.json",
                "--package"
            ])
            .status
            .success()
    );
    assert!(
        !project
            .run(&["build", ".", "--out", "startup.txt", "--package", "--package"])
            .status
            .success()
    );
}

#[test]
fn package_repair_handles_a_short_file_for_a_large_code_scene() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    project.write(
        "src/helpers.bfl",
        &format!("export fun text() -> String = {:?}\n", "x".repeat(300_000)),
    );
    project.write(
        "src/main.bfl",
        "import helpers.text\nscene start():\n    prose:\n        ${text().length}\n",
    );
    build(&project);
    let root = manifest(&project);
    let scene = root["chunks"]["helpers"]["scene"].as_str().unwrap();
    let path = project.0.join(format!("{scene}.txt"));
    let original = fs::read(&path).unwrap();
    assert!(original.len() > 300_000);
    fs::write(&path, "").unwrap();
    build(&project);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn completed_package_reuse_checks_chunk_bytes_cache_inputs_and_mode() {
    let project = setup();
    let args = ["build", ".", "--out", "startup.txt", "--package"];
    build(&project);
    let root = fs::read(project.0.join("startup.txt")).unwrap();
    let reused = project.run(&args);
    assert!(reused.status.success());
    assert!(String::from_utf8_lossy(&reused.stdout).contains("completed build reused"));
    let record = manifest(&project);
    let scene = record["chunks"]["first"]["scene"].as_str().unwrap();
    let path = project.0.join(format!("{scene}.txt"));
    let previous = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let mut corrupted = previous.clone();
    corrupted[0] = b'!';
    fs::write(&path, &corrupted).unwrap();
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(modified))
        .unwrap();
    let repaired = project.run(&args);
    assert!(
        repaired.status.success(),
        "{}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert!(!String::from_utf8_lossy(&repaired.stdout).contains("completed build reused"));
    assert_eq!(fs::read(&path).unwrap(), previous);
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), root);
    project.write(".bifoil/cache/v1/first.bfm.json", "damaged cached module");
    let repaired = project.run(&args);
    assert!(
        repaired.status.success(),
        "{}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert!(String::from_utf8_lossy(&repaired.stdout).contains("1 compiled, 2 reused"));
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), root);
    project.write(".bifoil/cache/v1/completed.json", "damaged receipt");
    let repaired = project.run(&args);
    assert!(
        repaired.status.success(),
        "{}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert!(!String::from_utf8_lossy(&repaired.stdout).contains("completed build reused"));
    let reused = project.run(&args);
    assert!(reused.status.success());
    assert!(String::from_utf8_lossy(&reused.stdout).contains("completed build reused"));
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), root);
}

#[test]
fn package_receipt_failure_does_not_replace_the_previous_completed_root() {
    let project = setup();
    build(&project);
    let previous = fs::read(project.0.join("startup.txt")).unwrap();
    let receipt = project.0.join(".bifoil/cache/v1/completed.json");
    fs::remove_file(&receipt).unwrap();
    fs::create_dir(&receipt).unwrap();
    project.write("src/first.bfl", "export fun first() -> Int = 3\n");
    let failed = project.run(&["build", ".", "--out", "startup.txt", "--package"]);
    assert!(!failed.status.success());
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), previous);
    fs::remove_dir(receipt).unwrap();
    build(&project);
    assert_ne!(fs::read(project.0.join("startup.txt")).unwrap(), previous);
}
