//! Extension collisions do not opt unrelated files into project compilation.
use super::Project;
use std::fs;

#[test]
fn explicit_projects_and_sources_do_not_claim_every_bfl_file() {
    let project = Project::new();
    project.write("bifoil.toml", "entry = 'main.start'\n");
    project.write("src/main.bfl", "scene start():\n    prose:\n        Original story.\n");
    let foreign = b"\0FLIGHT-LOG\xff\n";
    fs::write(project.0.join("src/flight.bfl"), foreign).unwrap();
    let built = project.run(&["build", ".", "--out", "startup.txt"]);
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    assert_eq!(fs::read(project.0.join("src/flight.bfl")).unwrap(), foreign);
    project.write("selected.txt", "export fun answer() -> Int = 7\n");
    let compiled = project.run(&[
        "module",
        "selected.txt",
        "--name",
        "selected",
        "--out",
        "selected.bfm.json",
    ]);
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let artifact = fs::read(project.0.join("selected.bfm.json")).unwrap();
    let rejected = project.run(&[
        "module",
        "src/flight.bfl",
        "--name",
        "selected",
        "--out",
        "selected.bfm.json",
    ]);
    assert!(!rejected.status.success());
    assert_eq!(fs::read(project.0.join("selected.bfm.json")).unwrap(), artifact);
    assert_eq!(fs::read(project.0.join("src/flight.bfl")).unwrap(), foreign);
}
