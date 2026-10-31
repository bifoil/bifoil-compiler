use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
#[path = "cli/code_capacity.rs"]
mod code_capacity;
#[path = "cli/encoded_modules.rs"]
mod encoded_modules;
#[path = "cli/flow_memory.rs"]
mod flow_memory;
#[path = "cli/function_capacity.rs"]
mod function_capacity;
#[path = "cli/legacy_enums.rs"]
mod legacy_enums;
#[path = "cli/manifests.rs"]
mod manifests;
#[path = "cli/module_capacity.rs"]
mod module_capacity;
#[path = "cli/packages.rs"]
mod packages;
#[path = "cli/source_format.rs"]
mod source_format;

#[path = "cli/audit_repairs.rs"]
mod audit_repairs;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "bifoil-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("src")).unwrap();
        fs::create_dir_all(path.join("modules")).unwrap();
        Self(path)
    }
    fn write(&self, path: &str, text: &str) {
        fs::write(self.0.join(path), text).unwrap()
    }
    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_bifoilc"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn independently_compiled_module_survives_source_removal() {
    let p = Project::new();
    p.write(
        "src/helpers.bfl",
        "export fun add(value: Int, amount: Int = 1) -> Int = value + amount\n",
    );
    p.write(
        "src/main.bfl",
        "import helpers.add\nscene start():\n    val score = add(3)\n    prose:\n        Score ${score}.\n",
    );
    p.write("bifoil.toml", "entry = \"main.start\"\n");
    let out = p.run(&[
        "module",
        "src/helpers.bfl",
        "--name",
        "helpers",
        "--out",
        "modules/helpers.bfm.json",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    fs::remove_file(p.0.join("src/helpers.bfl")).unwrap();
    let args = ["build", ".", "--out", "startup.txt"];
    let out = p.run(&args);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let first = fs::read(p.0.join("startup.txt")).unwrap();
    assert!(p.run(&args).status.success());
    assert_eq!(first, fs::read(p.0.join("startup.txt")).unwrap());
    let path = p.0.join("modules/helpers.bfm.json");
    let text = fs::read_to_string(&path).unwrap().replace("\"value\":1", "\"value\":2");
    fs::write(path, text).unwrap();
    let out = p.run(&args);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("digest mismatch"));
    assert_eq!(first, fs::read(p.0.join("startup.txt")).unwrap());
}
#[test]
fn shared_state_initializers_and_imported_mutation() {
    let p = Project::new();
    p.write("src/state.bfl", "export shared var score: Int = 5\n");
    p.write(
        "src/main.bfl",
        "import state.score\nscene start():\n    score += 1\n    prose:\n        ${score}\n",
    );
    p.write("bifoil.toml", "entry = \"main.start\"\n");
    let out = p.run(&["build", ".", "--out", "startup.txt"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = fs::read_to_string(p.0.join("startup.txt")).unwrap();
    assert!(text.contains("\"init\":[\"state.$init\"]"));
}
#[test]
fn invalid_projects_and_resource_envelope_fail_explicitly() {
    let p = Project::new();
    p.write("bifoil.toml", "entry = \"main.start\"\n");
    p.write("src/main.bfl", "scene start():\n    val wrong: String = 1\n");
    let out = p.run(&["build", ".", "--out", "startup.txt"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("src/main.bfl:2"));
    assert!(!p.0.join("startup.txt").exists());
    p.write("src/main.bfl", &"# oversized\n".repeat(100000));
    let out = p.run(&["build", ".", "--out", "startup.txt"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("budget exceeded"));
}
#[test]
fn cyclic_scene_interfaces_compile_and_cache_as_a_group() {
    let p = Project::new();
    p.write("bifoil.toml", "entry = \"main.start\"\n");
    p.write(
        "src/main.bfl",
        "import other.next\nexport scene start():\n    visit next()\n",
    );
    p.write(
        "src/other.bfl",
        "import main.start\nexport scene next():\n    if false:\n        goto start()\n",
    );
    let args = ["build", ".", "--out", "startup.txt"];
    let first = p.run(&args);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    assert!(String::from_utf8_lossy(&first.stdout).contains("2 compiled, 0 reused"));
    let original = fs::read(p.0.join("startup.txt")).unwrap();
    let repeat = p.run(&args);
    assert!(repeat.status.success(), "{}", String::from_utf8_lossy(&repeat.stderr));
    assert!(String::from_utf8_lossy(&repeat.stdout).contains("0 compiled, 2 reused"));
    assert_eq!(original, fs::read(p.0.join("startup.txt")).unwrap());
    p.write(
        "src/other.bfl",
        "import main.start\nexport scene next():\n    prose:\n        Changed.\n    if false:\n        goto start()\n",
    );
    let changed = p.run(&args);
    assert!(changed.status.success(), "{}", String::from_utf8_lossy(&changed.stderr));
    assert!(String::from_utf8_lossy(&changed.stdout).contains("2 compiled, 0 reused"));
    assert_ne!(original, fs::read(p.0.join("startup.txt")).unwrap());
}

#[test]
fn cyclic_modules_infer_anchored_returns_but_reject_unresolved_returns() {
    let p = Project::new();
    p.write("bifoil.toml", "entry = \"main.start\"\n");
    p.write(
        "src/main.bfl",
        "import other.number\nexport fun anchor() = 3\nscene start():\n    val answer: Int = number()\n",
    );
    p.write("src/other.bfl", "import main.anchor\nexport fun number() = anchor()\n");
    let args = ["build", ".", "--out", "startup.txt"];
    let out = p.run(&args);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let original = fs::read(p.0.join("startup.txt")).unwrap();
    p.write(
        "src/main.bfl",
        "import other.number\nexport fun anchor() = number()\nscene start():\n    number()\n",
    );
    let out = p.run(&args);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot infer recursive result"));
    assert_eq!(original, fs::read(p.0.join("startup.txt")).unwrap());
}

#[test]
fn source_free_cyclic_artifacts_reject_mixed_group_versions() {
    let p = Project::new();
    p.write("bifoil.toml", "entry = \"main.start\"\n");
    p.write(
        "src/main.bfl",
        "import a.number\nscene start():\n    val answer: Int = number()\n",
    );
    p.write("src/a.bfl", "import b.base\nexport fun number() = base()\n");
    p.write("src/b.bfl", "import a.number\nexport fun base() -> Int = 2\n");
    for name in ["a", "b"] {
        let source = format!("src/{name}.bfl");
        let artifact = format!("modules/{name}.bfm.json");
        let out = p.run(&[
            "module",
            &source,
            "--name",
            name,
            "--module-path",
            "modules",
            "--out",
            &artifact,
        ]);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }
    let old_b = fs::read(p.0.join("modules/b.bfm.json")).unwrap();
    p.write("src/b.bfl", "import a.number\nexport fun base() -> Int = 3\n");
    for name in ["a", "b"] {
        let source = format!("src/{name}.bfl");
        let artifact = format!("modules/{name}.bfm.json");
        assert!(
            p.run(&["module", &source, "--name", name, "--out", &artifact])
                .status
                .success()
        );
    }
    fs::remove_file(p.0.join("src/a.bfl")).unwrap();
    fs::remove_file(p.0.join("src/b.bfl")).unwrap();
    let args = ["build", ".", "--out", "startup.txt"];
    let out = p.run(&args);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let original = fs::read(p.0.join("startup.txt")).unwrap();
    fs::write(p.0.join("modules/b.bfm.json"), old_b).unwrap();
    let out = p.run(&args);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("group has changed"));
    assert_eq!(original, fs::read(p.0.join("startup.txt")).unwrap());
}

#[test]
fn cyclic_imports_still_reject_shared_initialization_cycles() {
    let p = Project::new();
    p.write("bifoil.toml", "entry = \"main.start\"\n");
    p.write(
        "src/main.bfl",
        "import other.b\nexport shared val a: Int = b + 1\nscene start():\n    prose:\n        ${a}\n",
    );
    p.write("src/other.bfl", "import main.a\nexport shared val b: Int = a + 1\n");
    let out = p.run(&["build", ".", "--out", "startup.txt"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("shared initialization dependency cycle"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn incremental_cache_tracks_content_dependencies_and_corruption() {
    let p = Project::new();
    p.write("bifoil.toml", "entry = \"main.start\"\n");
    p.write("src/values.bfl", "export fun number() -> Int = 1\n");
    p.write("src/other.bfl", "export fun label() -> String = \"value\"\n");
    p.write(
        "src/main.bfl",
        "import values.number\nimport other.label\nscene start():\n    prose:\n        ${label()} ${number()}\n",
    );
    let args = ["build", ".", "--out", "startup.txt"];
    let first = p.run(&args);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    assert!(String::from_utf8_lossy(&first.stdout).contains("3 compiled, 0 reused"));
    let original = fs::read(p.0.join("startup.txt")).unwrap();
    let repeat = p.run(&args);
    assert!(repeat.status.success());
    assert!(String::from_utf8_lossy(&repeat.stdout).contains("0 compiled, 3 reused"));
    assert_eq!(original, fs::read(p.0.join("startup.txt")).unwrap());
    // Same byte length and timestamp must not conceal an actual source change.
    let modified = fs::metadata(p.0.join("src/values.bfl")).unwrap().modified().unwrap();
    p.write("src/values.bfl", "export fun number() -> Int = 2\n");
    fs::File::options()
        .write(true)
        .open(p.0.join("src/values.bfl"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let changed = p.run(&args);
    assert!(changed.status.success(), "{}", String::from_utf8_lossy(&changed.stderr));
    assert!(String::from_utf8_lossy(&changed.stdout).contains("2 compiled, 1 reused"));
    let updated = fs::read(p.0.join("startup.txt")).unwrap();
    assert_ne!(original, updated);
    p.write(".bifoil/cache/v1/values.bfm.json", "corrupt");
    let repaired = p.run(&args);
    assert!(
        repaired.status.success(),
        "{}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert!(String::from_utf8_lossy(&repaired.stdout).contains("1 compiled, 2 reused"));
    assert_eq!(updated, fs::read(p.0.join("startup.txt")).unwrap());
    // Valid JSON with invalid instructions must fall back before touching the spool.
    let cache_path = p.0.join(".bifoil/cache/v1/values.bfm.json");
    let mut cached: serde_json::Value = serde_json::from_slice(&fs::read(&cache_path).unwrap()).unwrap();
    cached["module"]["functions"]["values.number"]["code"][0]["op"] = "unknown-op".into();
    fs::write(&cache_path, serde_json::to_vec_pretty(&cached).unwrap()).unwrap();
    let repaired = p.run(&args);
    assert!(
        repaired.status.success(),
        "{}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert!(String::from_utf8_lossy(&repaired.stdout).contains("1 compiled, 2 reused"));
    assert_eq!(updated, fs::read(p.0.join("startup.txt")).unwrap());
    let clean = p.run(&["build", ".", "--out", "startup.txt", "--clean"]);
    assert!(clean.status.success(), "{}", String::from_utf8_lossy(&clean.stderr));
    assert_eq!(updated, fs::read(p.0.join("startup.txt")).unwrap());
}
#[test]
fn legacy_artifacts_preserve_original_source_bytes() {
    let p = Project::new();
    fs::create_dir_all(p.0.join("legacy")).unwrap();
    p.write("bifoil.toml", "entry = \"main.start\"\nlegacy_path = \"legacy\"\n");
    p.write(
        "src/helpers.bfl",
        "export legacy fun award(value: Int) -> Int = \"native\" label \"award\" result \"score\"\n",
    );
    p.write(
        "src/main.bfl",
        "import helpers.award\nscene start():\n    val score = award(2)\n",
    );
    let source = "*label award\r\n*params value\r\n*set score + value\r\n*return\r\n";
    p.write("legacy/native.txt", source);
    assert!(
        p.run(&[
            "module",
            "src/helpers.bfl",
            "--name",
            "helpers",
            "--out",
            "modules/helpers.bfm.json"
        ])
        .status
        .success()
    );
    fs::remove_file(p.0.join("src/helpers.bfl")).unwrap();
    let result = p.run(&["build", ".", "--out", "startup.txt"]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let text = fs::read_to_string(p.0.join("startup.txt")).unwrap();
    let line = text.lines().find_map(|line| line.strip_prefix("*bifoil ")).unwrap();
    let program: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(program["legacy_sources"]["native"], source);
    p.write("legacy/native.txt", "*label different\n*return\n");
    let failed = p.run(&["build", ".", "--out", "startup.txt"]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("missing legacy label"));
    assert_eq!(text, fs::read_to_string(p.0.join("startup.txt")).unwrap());
}

#[test]
fn dotted_module_artifacts_resolve_source_roots_and_precompiled_dependencies() {
    let p = Project::new();
    fs::create_dir_all(p.0.join("src/pkg")).unwrap();
    fs::create_dir_all(p.0.join("modules/pkg")).unwrap();
    p.write("bifoil.toml", "entry = \"main.start\"\n");
    p.write("src/pkg/base.bfl", "export fun number() -> Int = 7\n");
    p.write(
        "src/pkg/helpers.bfl",
        "import pkg.base.number\nexport fun answer() -> Int = number()\n",
    );
    p.write(
        "src/main.bfl",
        "import pkg.helpers.answer\nscene start():\n    val value = answer()\n",
    );
    for name in ["base", "helpers"] {
        let source = format!("src/pkg/{name}.bfl");
        let module = format!("pkg.{name}");
        let artifact = format!("modules/pkg/{name}.bfm.json");
        let result = p.run(&[
            "module",
            &source,
            "--name",
            &module,
            "--out",
            &artifact,
            "--module-path",
            "modules",
        ]);
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        fs::remove_file(p.0.join(source)).unwrap();
    }
    let result = p.run(&["build", ".", "--out", "startup.txt"]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert!(
        fs::read_to_string(p.0.join("startup.txt"))
            .unwrap()
            .contains("pkg.base.number")
    );
}

#[test]
fn module_identity_and_owned_types_are_independent_of_other_loaded_modules() {
    let p = Project::new();
    p.write("bifoil.toml", "entry = \"main.start\"\n");
    p.write(
        "src/a.bfl",
        "export data class Record:\n    val value: Int\nexport fun make() -> Record = Record(3)\n",
    );
    p.write("src/b.bfl", "export fun two() -> Int = 2\n");
    p.write(
        "src/main.bfl",
        "import a.make\nimport b.two\nscene start():\n    prose:\n        ${make().value + two()}\n",
    );
    let args = ["build", ".", "--out", "startup.txt"];
    let out = p.run(&args);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let linked = fs::read_to_string(p.0.join("startup.txt")).unwrap();
    let program: serde_json::Value =
        serde_json::from_str(linked.lines().find_map(|line| line.strip_prefix("*bifoil ")).unwrap()).unwrap();
    let out = p.run(&["module", "src/b.bfl", "--name", "b", "--out", "modules/b.bfm.json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let artifact: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(p.0.join("modules/b.bfm.json")).unwrap()).unwrap();
    assert_eq!(program["modules"]["b"], artifact["digest"]);
    assert!(artifact.get("types").is_none());
    p.write("src/a.bfl", "export data class Record:\n    val value: Int\n    val label: String = \"new\"\nexport fun make() -> Record = Record(3)\n");
    p.write(
        "src/main.bfl",
        "import a.make\nimport b.two\nscene start():\n    prose:\n        ${make().label}|${two()}\n",
    );
    let changed = p.run(&args);
    assert!(changed.status.success(), "{}", String::from_utf8_lossy(&changed.stderr));
    assert!(String::from_utf8_lossy(&changed.stdout).contains("2 compiled, 1 reused"));
    let linked = fs::read_to_string(p.0.join("startup.txt")).unwrap();
    let program: serde_json::Value =
        serde_json::from_str(linked.lines().find_map(|line| line.strip_prefix("*bifoil ")).unwrap()).unwrap();
    assert_eq!(program["types"]["a.Record"]["fields"]["label"]["ty"]["kind"], "string");
}

#[test]
fn completed_build_reuse_validates_output_cache_and_source_bytes() {
    let p = Project::new();
    p.write("bifoil.toml", "entry = \"main.start\"\n");
    p.write("src/main.bfl", "scene start():\n    prose:\n        Original.\n");
    let args = ["build", ".", "--out", "startup.txt"];
    let first = p.run(&args);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let original = fs::read(p.0.join("startup.txt")).unwrap();
    let modified = fs::metadata(p.0.join("startup.txt")).unwrap().modified().unwrap();
    let repeat = p.run(&args);
    assert!(repeat.status.success(), "{}", String::from_utf8_lossy(&repeat.stderr));
    assert!(String::from_utf8_lossy(&repeat.stdout).contains("completed build reused after byte validation"));
    assert_eq!(
        modified,
        fs::metadata(p.0.join("startup.txt")).unwrap().modified().unwrap()
    );
    let corrupted = String::from_utf8(original.clone())
        .unwrap()
        .replace("Original.", "Modified.");
    assert_ne!(corrupted.as_bytes(), original);
    fs::write(p.0.join("startup.txt"), corrupted).unwrap();
    fs::File::options()
        .write(true)
        .open(p.0.join("startup.txt"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let repaired = p.run(&args);
    assert!(
        repaired.status.success(),
        "{}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert!(!String::from_utf8_lossy(&repaired.stdout).contains("completed build reused"));
    assert_eq!(original, fs::read(p.0.join("startup.txt")).unwrap());
    p.write(".bifoil/cache/v1/main.bfm.json", "corrupted cache");
    let repaired = p.run(&args);
    assert!(
        repaired.status.success(),
        "{}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert!(String::from_utf8_lossy(&repaired.stdout).contains("1 compiled, 0 reused"));
    let source_time = fs::metadata(p.0.join("src/main.bfl")).unwrap().modified().unwrap();
    p.write("src/main.bfl", "scene start():\n    prose:\n        Modified.\n");
    fs::File::options()
        .write(true)
        .open(p.0.join("src/main.bfl"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(source_time))
        .unwrap();
    let changed = p.run(&args);
    assert!(changed.status.success(), "{}", String::from_utf8_lossy(&changed.stderr));
    assert!(String::from_utf8_lossy(&changed.stdout).contains("1 compiled, 0 reused"));
    assert_ne!(original, fs::read(p.0.join("startup.txt")).unwrap());
    p.write(".bifoil/cache/v1/completed.json", "broken build record");
    let recovered = p.run(&args);
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert!(String::from_utf8_lossy(&recovered.stdout).contains("0 compiled, 1 reused"));
}

#[test]
fn completed_build_reuse_rechecks_source_precedence_and_native_inventory() {
    let p = Project::new();
    p.write("bifoil.toml", "entry = \"main.start\"\nlegacy_path = \"legacy\"\n");
    fs::create_dir(p.0.join("legacy")).unwrap();
    p.write("legacy/helpers.txt", "*return\n");
    p.write("src/helpers.bfl", "export fun value() -> Int = 1\n");
    p.write(
        "src/main.bfl",
        "import helpers.value\nscene start():\n    prose:\n        ${value()}\n",
    );
    let compiled = p.run(&[
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
    fs::remove_file(p.0.join("src/helpers.bfl")).unwrap();
    let args = ["build", ".", "--out", "startup.txt"];
    let first = p.run(&args);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    assert!(String::from_utf8_lossy(&p.run(&args).stdout).contains("completed build reused"));
    p.write("src/helpers.bfl", "export fun value() -> Int = 2\n");
    let changed = p.run(&args);
    assert!(changed.status.success(), "{}", String::from_utf8_lossy(&changed.stderr));
    assert!(String::from_utf8_lossy(&changed.stdout).contains("2 compiled, 0 reused"));
    let before_native = fs::read(p.0.join("startup.txt")).unwrap();
    p.write("legacy/extra.txt", "*return\n");
    let changed = p.run(&args);
    assert!(changed.status.success(), "{}", String::from_utf8_lossy(&changed.stderr));
    assert!(!String::from_utf8_lossy(&changed.stdout).contains("completed build reused"));
    assert_ne!(before_native, fs::read(p.0.join("startup.txt")).unwrap());
    let linked = fs::read_to_string(p.0.join("startup.txt")).unwrap();
    let program: serde_json::Value =
        serde_json::from_str(linked.lines().find_map(|line| line.strip_prefix("*bifoil ")).unwrap()).unwrap();
    assert_eq!(program["legacy_sources"]["extra"], "*return\n");
}

#[test]
fn diagnostics_retain_one_precise_source_location_and_preserve_output() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    project.write("startup.txt", "previous complete output");
    for (source, expected) in [
        (
            "scene start():\n    val wrong = 1 2\n",
            "src/main.bfl:2: unsupported or unexpected trailing token number 2",
        ),
        (
            "scene start():\n    val wrong = )\n",
            "src/main.bfl:2: expected an expression",
        ),
        (
            "scene start():\n    val action = (value: Int) =>:\n        val wrong: String = value\n        wrong\n",
            "src/main.bfl:3: expected String, found Int",
        ),
        (
            "fun <T> wrong(value: T) = value + 1\nscene start():\n    wrong(3)\n",
            "src/main.bfl:1: operator '+' cannot combine T and Int",
        ),
    ] {
        project.write("src/main.bfl", source);
        let result = project.run(&["build", ".", "--out", "startup.txt"]);
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(!result.status.success());
        assert!(error.contains(expected), "{error}");
        assert_eq!(error.matches("src/main.bfl:").count(), 1, "{error}");
        assert_eq!(
            fs::read_to_string(project.0.join("startup.txt")).unwrap(),
            "previous complete output"
        );
    }
}

#[test]
fn changed_imports_reinfer_local_method_interfaces_and_preserve_failed_outputs() {
    fn program(project: &Project) -> serde_json::Value {
        let source = fs::read_to_string(project.0.join("startup.txt")).unwrap();
        serde_json::from_str(source.lines().find_map(|line| line.strip_prefix("*bifoil ")).unwrap()).unwrap()
    }
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    project.write(
        "src/main.bfl",
        r#"import helpers.external
data class Box:
    val number: Int = 0
    fun fetch() = external() - 1
scene start():
    prose:
        ${Box().fetch()}
"#,
    );
    let args = ["build", ".", "--out", "startup.txt"];
    for (source, expected) in [
        ("export fun external() -> Int = 7\n", "int"),
        ("export fun external() -> Float = 7.5\n", "float"),
    ] {
        project.write("src/helpers.bfl", source);
        let result = project.run(&args);
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        assert!(String::from_utf8_lossy(&result.stdout).contains("2 compiled, 0 reused"));
        let linked = program(&project);
        let method = &linked["types"]["main.Box"]["methods"]["fetch"][0];
        assert_eq!(method["result"]["kind"], expected);
        let id = method["id"].as_str().unwrap();
        assert_eq!(linked["functions"][id]["signature"]["result"]["kind"], expected);
    }
    let completed = fs::read(project.0.join("startup.txt")).unwrap();
    project.write(
        "src/helpers.bfl",
        r#"export fun external() -> String = "seven"
"#,
    );
    assert!(!project.run(&args).status.success());
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), completed);
    project.write("src/helpers.bfl", "export fun external() -> Float = 7.5\n");
    let recovered = project.run(&args);
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert!(String::from_utf8_lossy(&recovered.stdout).contains("1 compiled, 1 reused"));
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), completed);
}

#[test]
fn wide_default_branches_fit_the_native_memory_guard() {
    for calling in [false, true] {
        let project = Project::new();
        project.write("bifoil.toml", "entry = \"main.start\"\n");
        let mut source = String::from("fun initial() -> Int = 1\ndata class Record:\n");
        for number in 0..2048 {
            let default = if calling { " = initial()" } else { "" };
            source.push_str(&format!("    val field{number:04}: Int{default}\n"));
        }
        source.push_str("scene start():\n    Unit\n");
        project.write("src/main.bfl", &source);
        let result = project.run(&["build", ".", "--out", "startup.txt", "--clean"]);
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        let compiled = fs::read_to_string(project.0.join("startup.txt")).unwrap();
        let payload = compiled.lines().find_map(|line| line.strip_prefix("*bifoil ")).unwrap();
        let program: serde_json::Value = serde_json::from_str(payload).unwrap();
        let copy = &program["functions"]["main.$type:Record:copy"];
        assert_eq!(copy["signature"]["params"].as_array().unwrap().len(), 2048);
        assert_eq!(
            copy["code"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|instruction| instruction["op"] == "default")
                .count(),
            2048
        );
    }
}

#[test]
fn long_forward_aliases_compile_and_cycles_preserve_completed_output() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    let count = 16384;
    let mut source = String::new();
    for index in 0..count - 1 {
        source.push_str(&format!("typealias A{index:05} = A{:05}\n", index + 1));
    }
    source.push_str(&format!("typealias A{:05} = Int\n", count - 1));
    source.push_str("scene start():\n    val value: A00000 = 7\n    prose:\n        ${value}\n");
    project.write("src/main.bfl", &source);
    let built = project.run(&["build", ".", "--out", "startup.txt", "--clean"]);
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let completed = fs::read(project.0.join("startup.txt")).unwrap();
    let cyclic = source.replace(
        &format!("typealias A{:05} = Int", count - 1),
        &format!("typealias A{:05} = A00000", count - 1),
    );
    project.write("src/main.bfl", &cyclic);
    let failed = project.run(&["build", ".", "--out", "startup.txt", "--clean"]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("type alias expansion cycle"));
    assert_eq!(fs::read(project.0.join("startup.txt")).unwrap(), completed);
}
