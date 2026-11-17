//! Typed bridge admission rejects each malformed interface before execution.
use crate::legacy::{validate_signature_with, value_type_with, variable};
use crate::model::{GenericParam, LegacyTarget, Param, SignatureData, Type};
use std::sync::Arc;

fn signature() -> crate::model::Signature {
    Arc::new(SignatureData {
        generics: vec![],
        id: "original.bridge".into(),
        params: vec![Param {
            name: "value".into(),
            ty: Type::Int,
            mutable: false,
            default: false,
            vararg: false,
        }]
        .into(),
        result: Type::Int,
        suspend: false,
        scene: false,
    })
}
fn target() -> LegacyTarget {
    LegacyTarget {
        transfer: false,
        scene: "helpers".into(),
        label: Some("echo".into()),
        result: Some("answer".into()),
    }
}

#[test]
fn coverage_legacy_signatures_require_return_storage_valid_names_and_explicit_transfer_effects() {
    let original = signature();
    let destination = target();
    let is_enum = |name: &str| name == "Mood";
    validate_signature_with(&original, &destination, &is_enum).unwrap();
    for field in ["scene", "generics", "parameter", "result"] {
        let mut changed = original.clone();
        let header = Arc::make_mut(&mut changed);
        match field {
            "scene" => header.scene = true,
            "generics" => header.generics.push(GenericParam {
                name: "T".into(),
                variance: 0,
                bounds: vec![],
            }),
            "parameter" => header.params_mut()[0].ty = Type::Any,
            "result" => header.result = Type::Any,
            _ => unreachable!(),
        }
        assert!(
            validate_signature_with(&changed, &destination, &is_enum).is_err(),
            "{field}"
        );
    }
    for field in [
        "result_missing",
        "result_invalid",
        "scene_empty",
        "label_empty",
        "label_space",
    ] {
        let mut changed = destination.clone();
        match field {
            "result_missing" => changed.result = None,
            "result_invalid" => changed.result = Some("9invalid".into()),
            "scene_empty" => changed.scene.clear(),
            "label_empty" => changed.label = Some(String::new()),
            "label_space" => changed.label = Some("two labels".into()),
            _ => unreachable!(),
        }
        assert!(
            validate_signature_with(&original, &changed, &is_enum).is_err(),
            "{field}"
        );
    }
    let mut transfer = destination.clone();
    transfer.transfer = true;
    for (suspend, result, storage) in [
        (false, Type::Nothing, None),
        (true, Type::Unit, None),
        (true, Type::Nothing, Some("answer".into())),
    ] {
        let mut header = original.clone();
        Arc::make_mut(&mut header).suspend = suspend;
        Arc::make_mut(&mut header).result = result;
        transfer.result = storage;
        assert!(validate_signature_with(&header, &transfer, &is_enum).is_err());
    }
    let mut header = original.clone();
    Arc::make_mut(&mut header).suspend = true;
    Arc::make_mut(&mut header).result = Type::Nothing;
    transfer.result = None;
    transfer.label = None;
    validate_signature_with(&header, &transfer, &is_enum).unwrap();
}

#[test]
fn coverage_legacy_values_keep_unit_result_only_and_enum_arguments_exact() {
    let is_enum = |name: &str| name == "Mood";
    for scalar in [Type::Int, Type::Float, Type::String, Type::Char, Type::Boolean] {
        assert!(value_type_with(&scalar, false, &is_enum));
        assert!(value_type_with(&scalar.nullable(), false, &is_enum));
    }
    assert!(value_type_with(&Type::Unit, true, &is_enum));
    assert!(!value_type_with(&Type::Unit, false, &is_enum));
    assert!(!value_type_with(&Type::Unit.nullable(), true, &is_enum));
    assert!(value_type_with(&Type::named("Mood", vec![]), false, &is_enum));
    for invalid in [
        Type::Any,
        Type::Nothing,
        Type::named("Other", vec![]),
        Type::named("Mood", vec![Type::Int]),
    ] {
        assert!(!value_type_with(&invalid, false, &is_enum));
    }
    for valid in ["a", "score2", "original_value"] {
        assert!(variable(valid));
    }
    for invalid in ["", "1score", "_score", "two names", "score-2", "é"] {
        assert!(!variable(invalid));
    }
}

struct OriginalLegacyProject(std::path::PathBuf);
impl OriginalLegacyProject {
    fn new() -> Self {
        static SERIAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("bifoil-legacy-contract-{}-{serial}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn sources(&self, relative: &str) -> Result<std::collections::BTreeMap<String, String>, String> {
        crate::legacy::sources(&self.0, relative, &mut 0, std::time::Instant::now())
    }
}
impl Drop for OriginalLegacyProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn coverage_legacy_source_inventory_retains_original_bytes_and_rejects_escaping_paths() {
    let project = OriginalLegacyProject::new();
    let legacy = project.0.join("legacy");
    std::fs::create_dir_all(legacy.join("nested")).unwrap();
    std::fs::write(legacy.join("Start.TXT"), "Original Ω\r\n*finish\r\n").unwrap();
    std::fs::write(legacy.join("nested/helper.txt"), "*label echo\n*return\n").unwrap();
    std::fs::write(legacy.join("ignored.md"), "Not a scene").unwrap();
    let sources = project.sources("legacy").unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources["start"], "Original Ω\r\n*finish\r\n");
    assert_eq!(sources["nested/helper"], "*label echo\n*return\n");
    for relative in ["../outside", "legacy/../legacy"] {
        assert!(project.sources(relative).unwrap_err().contains("project-relative"));
    }
    assert!(
        project
            .sources(legacy.to_str().unwrap())
            .unwrap_err()
            .contains("project-relative")
    );
    assert!(project.sources("missing").unwrap_err().contains("legacy_path"));
}

#[test]
fn coverage_legacy_source_inventory_rejects_the_first_scene_beyond_its_independent_limit() {
    let project = OriginalLegacyProject::new();
    let directory = project.0.join("legacy");
    std::fs::create_dir(&directory).unwrap();
    for index in 0..256 {
        std::fs::write(directory.join(format!("scene{index}.txt")), "Original.").unwrap();
    }
    assert_eq!(project.sources("legacy").unwrap().len(), 256);
    std::fs::write(directory.join("one_more.txt"), "Original.").unwrap();
    assert!(project.sources("legacy").unwrap_err().contains("scene budget"));
}

#[cfg(unix)]
#[test]
fn coverage_legacy_source_inventory_rejects_symbolic_links_and_case_collisions() {
    use std::os::unix::ffi::OsStringExt;
    let project = OriginalLegacyProject::new();
    let outside = OriginalLegacyProject::new();
    std::os::unix::fs::symlink(&outside.0, project.0.join("escape")).unwrap();
    assert!(project.sources("escape").unwrap_err().contains("stay inside"));
    let directory = project.0.join("legacy");
    std::fs::create_dir(&directory).unwrap();
    let link = directory.join("linked.txt");
    std::os::unix::fs::symlink(outside.0.join("missing.txt"), &link).unwrap();
    assert!(project.sources("legacy").unwrap_err().contains("symbolic links"));
    std::fs::remove_file(link).unwrap();
    let invalid = directory.join(std::ffi::OsString::from_vec(vec![0xff, b'.', b't', b'x', b't']));
    std::fs::write(&invalid, "Original.").unwrap();
    assert!(project.sources("legacy").unwrap_err().contains("UTF-8"));
    std::fs::remove_file(invalid).unwrap();
    std::fs::write(directory.join("Name.txt"), "First").unwrap();
    std::fs::write(directory.join("name.txt"), "Second").unwrap();
    if std::fs::read_dir(&directory).unwrap().count() == 2 {
        assert!(project.sources("legacy").unwrap_err().contains("case-insensitive"));
    }
}
