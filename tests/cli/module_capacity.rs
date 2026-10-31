//! A full module graph, including typed visits to every scene and an atomic failure.

use super::Project;
use std::fs;

fn write_scenes(project: &Project, count: usize) {
    fs::create_dir_all(project.0.join("src/rooms")).unwrap();
    project.write("src/stats.bfl", "export shared var visits: Int = 0\n");
    let mut main = String::from("import stats.visits\n");
    for index in 0..count {
        main.push_str(&format!("import rooms.r{index:04}.enter as room{index:04}\n"));
        project.write(
            &format!("src/rooms/r{index:04}.bfl"),
            "import stats.visits\nexport scene enter():\n    visits += 1\n",
        );
    }
    main.push_str("scene start():\n    val destinations = [\n");
    for index in 0..count {
        main.push_str(&format!("        ::room{index:04},\n"));
    }
    main.push_str(
        r#"    ]
    for destination in destinations:
        visit destination()
    prose:
        ${visits}
"#,
    );
    project.write("src/main.bfl", &main);
}

#[test]
fn complete_module_graph_fits_and_rejects_the_next_reachable_module() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = \"main.start\"\n");
    write_scenes(&project, 510);
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
    assert_eq!(program["modules"].as_object().unwrap().len(), 512);
    write_scenes(&project, 511);
    let rejected = project.run(&["build", ".", "--out", "startup.txt", "--clean"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("module count budget exceeded"));
    assert_eq!(completed, fs::read_to_string(project.0.join("startup.txt")).unwrap());
}
