//! Wide source-flow regressions run with the production compiler and its memory guard.

use super::Project;

fn scene_with_values(count: usize) -> String {
    let mut source = String::from("scene start():\n");
    for index in 0..count {
        source.push_str(&format!("    val fact{index:04} = {index}\n"));
    }
    source.push_str("    val values = [\n");
    for index in 0..count {
        source.push_str(&format!("        fact{index:04},\n"));
    }
    source.push_str("    ]\n");
    source
}

#[test]
fn wide_horizontal_conditionals_fit_native_compilation_and_formatting() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    let mut source = String::from("scene start():\n    val selector = -1\n    var result = 0\n");
    for index in 0..8192 {
        let keyword = if index == 0 { "if" } else { "elif" };
        source.push_str(&format!(
            "    {keyword} selector == {index}:\n        result = {index}\n"
        ));
    }
    source.push_str("    else:\n        result = 8192\n    prose:\n        ${result}\n");
    project.write("src/main.bfl", &source);
    let built = project.run(&["build", ".", "--out", "startup.txt", "--clean"]);
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let checked = project.run(&["format", "src/main.bfl", "--check"]);
    assert!(checked.status.success(), "{}", String::from_utf8_lossy(&checked.stderr));
}

#[test]
fn ready_assignments_do_not_duplicate_thousands_of_visible_bindings() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    let mut source = scene_with_values(4000);
    source.push_str("    var total = 0\n    for item in values:\n        total += item\n    val selector = -1\n    var result = 0\n");
    for index in 0..128 {
        let keyword = if index == 0 { "if" } else { "elif" };
        source.push_str(&format!(
            "    {keyword} selector == {index}:\n        result = {index}\n"
        ));
    }
    source.push_str("    else:\n        result = 128\n    prose:\n        ${result}|${total}\n");
    project.write("src/main.bfl", &source);
    let built = project.run(&["build", ".", "--out", "startup.txt", "--clean"]);
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
}

#[test]
fn completed_paths_release_wide_definite_assignment_frames() {
    for kind in ["conditional", "when", "choice"] {
        let project = Project::new();
        project.write("bifoil.toml", "entry = \"main.start\"\n");
        let mut source = scene_with_values(4000);
        source.push_str("    val selector = -1\n    val result: Int\n");
        if kind == "conditional" {
            for index in 0..128 {
                let keyword = if index == 0 { "if" } else { "elif" };
                source.push_str(&format!(
                    "    {keyword} selector == {index}:\n        result = {index}\n"
                ));
            }
            source.push_str("    else:\n        result = 128\n");
        } else {
            source.push_str(if kind == "when" {
                "    when selector:\n"
            } else {
                "    choice:\n"
            });
            for index in 0..128 {
                let header = if kind == "when" {
                    format!("case {index}")
                } else {
                    format!("option \"Take {index}\"")
                };
                source.push_str(&format!("        {header}:\n            result = {index}\n"));
            }
            if kind == "when" {
                source.push_str("        else:\n            result = 128\n");
            }
        }
        source.push_str("    prose:\n        ${result}|${values.size}\n");
        project.write("src/main.bfl", &source);
        let built = project.run(&["build", ".", "--out", "startup.txt", "--clean"]);
        assert!(
            built.status.success(),
            "{kind}: {}",
            String::from_utf8_lossy(&built.stderr)
        );
    }
}

#[test]
fn loop_breaks_release_wide_definite_assignment_frames() {
    for kind in ["while", "for", "do", "labeled"] {
        let project = Project::new();
        project.write("bifoil.toml", "entry = \"main.start\"\n");
        let mut source = scene_with_values(4000);
        source.push_str("    val selector = -1\n    val result: Int\n");
        source.push_str(match kind {
            "for" => "    for passIndex in 0..0:\n",
            "do" => "    do:\n",
            "labeled" => "    outer@ while true:\n        for passIndex in 0..0:\n",
            _ => "    while true:\n",
        });
        let indentation = if kind == "labeled" { "            " } else { "        " };
        let transfer = if kind == "labeled" { "break@outer" } else { "break" };
        for index in 0..128 {
            let keyword = if index == 0 { "if" } else { "elif" };
            source.push_str(&format!("{indentation}{keyword} selector == {index}:\n{indentation}    result = {index}\n{indentation}    {transfer}\n"));
        }
        source.push_str(&format!(
            "{indentation}else:\n{indentation}    result = 128\n{indentation}    {transfer}\n"
        ));
        if kind == "do" {
            source.push_str("    while false\n");
        }
        source.push_str("    prose:\n        ${result}|${values.size}\n");
        project.write("src/main.bfl", &source);
        let built = project.run(&["build", ".", "--out", "startup.txt", "--clean"]);
        assert!(
            built.status.success(),
            "{kind}: {}",
            String::from_utf8_lossy(&built.stderr)
        );
    }
}

#[test]
fn continue_paths_share_unchanged_binding_storage() {
    for branches in [128, 1024] {
        let project = Project::new();
        project.write("bifoil.toml", "entry = \"main.start\"\n");
        let mut source = scene_with_values(4000);
        source.push_str("    val selector = -1\n    val result: Int\n    do:\n");
        for index in 0..branches {
            let keyword = if index == 0 { "if" } else { "elif" };
            source.push_str(&format!(
                "        {keyword} selector == {index}:\n            result = {index}\n            continue\n"
            ));
        }
        source.push_str(&format!("        else:\n            result = {branches}\n            continue\n    while false\n    prose:\n        ${{result}}|${{values.size}}\n"));
        project.write("src/main.bfl", &source);
        let built = project.run(&["build", ".", "--out", "startup.txt", "--clean"]);
        assert!(
            built.status.success(),
            "{branches} paths: {}",
            String::from_utf8_lossy(&built.stderr)
        );
    }
}
