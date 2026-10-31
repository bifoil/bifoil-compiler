//! Typed interfaces and unchanged ChoiceScript source collection.
//!
//! Legacy expressions are never parsed as Bifoil. The pinned interpreter executes
//! them; these declarations specify only checked values crossing the boundary.

use crate::model::{LegacyTarget, Signature, Type, TypeDef};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

pub(crate) fn enum_definition(definition: &TypeDef) -> bool {
    definition.kind == "enum"
        && definition.parameters.is_empty()
        && definition.fields.is_empty()
        && definition.parent.is_none()
}
fn catalog_enum(name: &str) -> bool {
    crate::declarations::definition(name).is_some_and(|definition| enum_definition(&definition))
}
pub(crate) fn value_type(ty: &Type, result: bool) -> bool {
    value_type_with(ty, result, &catalog_enum)
}
pub(crate) fn value_type_with(ty: &Type, result: bool, is_enum: &impl Fn(&str) -> bool) -> bool {
    match ty {
        Type::Int | Type::Float | Type::Boolean | Type::String | Type::Char => true,
        Type::Unit => result,
        Type::Nullable { inner } => value_type_with(inner, false, is_enum),
        Type::Named { name, args } => args.is_empty() && is_enum(name),
        _ => false,
    }
}

pub(crate) fn variable(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        && name.as_bytes()[0].is_ascii_alphabetic()
}

pub(crate) fn validate_signature(signature: &Signature, target: &LegacyTarget) -> Result<(), String> {
    validate_signature_with(signature, target, &catalog_enum)
}
pub(crate) fn validate_signature_with(
    signature: &Signature,
    target: &LegacyTarget,
    is_enum: &impl Fn(&str) -> bool,
) -> Result<(), String> {
    if !signature.generics.is_empty() || signature.scene {
        return Err("legacy functions cannot declare generics or scenes".into());
    }
    if !(value_type_with(&signature.result, true, is_enum) || (target.transfer && signature.result == Type::Nothing))
        || signature.params.iter().any(|p| !value_type_with(&p.ty, false, is_enum))
    {
        return Err("legacy interfaces require primitive or nullable primitive values, or plain enums".into());
    }
    if target.transfer && (!signature.suspend || signature.result != Type::Nothing || target.result.is_some()) {
        return Err("legacy transfer bridges require suspend, Nothing, and no result binding".into());
    }
    if !target.transfer && signature.result != Type::Unit && target.result.is_none() {
        return Err("a value-returning legacy function requires an explicit result binding".into());
    }
    if target.result.as_ref().is_some_and(|name| !variable(name)) {
        return Err("invalid legacy result binding".into());
    }
    if target.scene.is_empty()
        || target
            .label
            .as_ref()
            .is_some_and(|label| label.is_empty() || label.chars().any(char::is_whitespace))
    {
        return Err("legacy call requires a scene and a valid optional label".into());
    }
    Ok(())
}

pub(crate) fn sources(
    project: &Path,
    relative: &str,
    total: &mut u64,
    started: Instant,
) -> Result<BTreeMap<String, String>, String> {
    let directory = Path::new(relative);
    if directory.is_absolute()
        || directory
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err("legacy_path must be a project-relative directory".into());
    }
    let root = project
        .join(directory)
        .canonicalize()
        .map_err(|error| format!("legacy_path: {error}"))?;
    let project = project.canonicalize().map_err(|error| error.to_string())?;
    if !root.starts_with(project) {
        return Err("legacy_path must stay inside the project".into());
    }
    let mut pending = vec![root.clone()];
    let mut sources = BTreeMap::new();
    while let Some(directory) = pending.pop() {
        crate::budget(started)?;
        for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            if file_type.is_symlink() {
                return Err("legacy sources cannot contain symbolic links".into());
            }
            if file_type.is_dir() {
                pending.push(entry.path());
                continue;
            }
            let path = entry.path();
            if !file_type.is_file()
                || !path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
            {
                continue;
            }
            let relative = path.strip_prefix(&root).unwrap().with_extension("");
            let name = relative
                .to_str()
                .ok_or("legacy scene names must be UTF-8")?
                .replace('\\', "/")
                .to_lowercase();
            let source = crate::read(&path, crate::model::MAX_SOURCE as u64, total)?;
            if sources.insert(name, source).is_some() {
                return Err("duplicate case-insensitive legacy scene name".into());
            }
            if sources.len() > 256 {
                return Err("legacy scene budget exceeded".into());
            }
        }
    }
    Ok(sources)
}

pub(crate) fn validate_targets(
    references: &[crate::code_store::LegacyReference],
    sources: &BTreeMap<String, String>,
    start: std::time::Instant,
) -> Result<(), String> {
    let mut labels = BTreeMap::new();
    for (index, reference) in references.iter().enumerate() {
        if index % 256 == 0 {
            crate::budget(start)?;
        }
        let target = &reference.target;
        let span = &reference.span;
        let source = sources
            .get(&target.scene.to_lowercase())
            .ok_or_else(|| format!("{}:{}: missing legacy scene '{}'", span.file, span.line, target.scene))?;
        if let Some(label) = &target.label {
            let scene = target.scene.to_lowercase();
            if !labels.contains_key(&scene) {
                let mut found = std::collections::BTreeSet::new();
                for (index, line) in source.lines().enumerate() {
                    if index % 256 == 0 {
                        crate::budget(start)?;
                    }
                    let mut words = line.split_whitespace();
                    if words.next().is_some_and(|word| word.eq_ignore_ascii_case("*label"))
                        && let Some(name) = words.next()
                        && words.next().is_none()
                    {
                        found.insert(name.to_ascii_lowercase());
                    }
                }
                labels.insert(scene.clone(), found);
            }
            let exists = labels[&scene].contains(&label.to_ascii_lowercase());
            if !exists {
                return Err(format!(
                    "{}:{}: missing legacy label '{}' in '{}'",
                    span.file, span.line, label, target.scene
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_store::LegacyReference;
    use crate::model::{LegacyTarget, Span};
    use std::time::{Duration, Instant};

    fn reference(label: &str) -> LegacyReference {
        LegacyReference {
            function: "main.bridge".into(),
            target: LegacyTarget {
                scene: "Helpers".into(),
                transfer: false,
                label: Some(label.into()),
                result: None,
            },
            span: Span {
                file: "src/main.bfl".into(),
                line: 7,
            },
        }
    }

    #[test]
    fn repeated_targets_share_the_scene_index_and_keep_label_matching_rules() {
        let source = "*comment filler\n".repeat(5000) + "  *LaBeL FiNiSh\n*label extra arguments\n";
        let sources = BTreeMap::from([("helpers".into(), source)]);
        let references = (0..1000).map(|_| reference("FINISH")).collect::<Vec<_>>();
        validate_targets(&references, &sources, Instant::now()).unwrap();
        let error = validate_targets(&[reference("extra")], &sources, Instant::now()).unwrap_err();
        assert!(error.contains("src/main.bfl:7: missing legacy label"), "{error}");
    }

    #[test]
    fn legacy_target_indexing_observes_the_job_deadline() {
        let sources = BTreeMap::from([("helpers".into(), "*label finish\n".into())]);
        let previous = crate::INCREMENTAL_DEADLINE.replace(true);
        let result = validate_targets(
            &[reference("finish")],
            &sources,
            Instant::now() - Duration::from_secs(10),
        );
        crate::INCREMENTAL_DEADLINE.set(previous);
        assert!(result.unwrap_err().contains("time budget"));
    }
}
