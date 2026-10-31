//! Independent module limits and a larger streamed aggregate package.

use super::Project;
use std::fs;

fn source(routines: usize, additions: usize) -> String {
    (0..routines)
        .map(|index| {
            let expression = format!("value + {index}") + &" + 0".repeat(additions - 1);
            format!("export fun f{index:05}(value: Int) -> Int = {expression}\n")
        })
        .collect()
}

#[test]
fn independently_bounded_modules_link_beyond_the_old_aggregate_code_limit() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    for name in ["first", "second"] {
        let source_path = format!("src/{name}.bfl");
        let artifact_path = format!("modules/{name}.bfm.json");
        project.write(&source_path, &source(1800, 14));
        let published = project.run(&["module", &source_path, "--name", name, "--out", &artifact_path]);
        assert!(
            published.status.success(),
            "{}",
            String::from_utf8_lossy(&published.stderr)
        );
        fs::remove_file(project.0.join(source_path)).unwrap();
    }
    project.write(
        "src/main.bfl",
        "import first.f00000\nimport second.f00000 as other\nscene start():\n    prose:\n        ${f00000(0)}|${other(0)}\n",
    );
    let built = project.run(&["build", ".", "--out", "startup.txt", "--package", "--clean"]);
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
        let count: usize = chunk["functions"]
            .as_object()
            .unwrap()
            .values()
            .map(|function| function["code"].as_array().unwrap().len())
            .sum();
        assert!(count <= 100_000);
        total += count;
    }
    assert!(total > 100_000 && total <= 500_000, "unexpected code size {total}");

    let completed = fs::read(project.0.join("modules/first.bfm.json")).unwrap();
    project.write("src/first.bfl", &source(3200, 16));
    let rejected = project.run(&[
        "module",
        "src/first.bfl",
        "--name",
        "first",
        "--out",
        "modules/first.bfm.json",
    ]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("module instruction budget exceeded"));
    assert_eq!(fs::read(project.0.join("modules/first.bfm.json")).unwrap(), completed);
}
