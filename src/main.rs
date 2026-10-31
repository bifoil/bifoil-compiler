mod artifact;
mod cache;
mod code_store;
mod compiler;
mod declarations;
mod diagnostics;
mod function_catalog;
mod generics;
mod initialization;
mod input;
mod legacy;
mod lexer;
mod loader;
mod manifest;
mod model;
mod module_input;
mod module_validation;
mod package;
mod parser;
mod project;
mod resources;
mod scope;
mod shared_dependencies;
mod signature_sharing;
mod source_format;
mod syntax;
mod trace;
mod type_storage;
mod types;

use module_validation::verify;

use input::read;
use loader::Loader;
use model::PROFILE;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

// Aggregate source/artifact bytes; each file retains its independent guard.
const MAX_INPUT: u64 = 64 * 1024 * 1024;
const MAX_MODULES: usize = 512;

thread_local! {
    static INCREMENTAL_DEADLINE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
fn budget(start: Instant) -> Result<(), String> {
    if INCREMENTAL_DEADLINE.get() && start.elapsed().as_secs_f64() >= 9.5 {
        Err("compilation time budget exhausted".into())
    } else {
        Ok(())
    }
}
fn module_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|part| {
            let mut chars = part.chars();
            chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}
/// Infer the source root when a dotted module follows the source path layout.
/// An unrelated explicit filename retains its containing directory as the root.
fn source_root(input: &Path, name: &str) -> PathBuf {
    let parent = input.parent().unwrap_or(Path::new("."));
    let mut root = parent;
    let mut names = name.split('.').collect::<Vec<_>>();
    let file = names.pop().unwrap_or_default();
    if input.file_stem().and_then(|part| part.to_str()) != Some(file) {
        return parent.to_path_buf();
    }
    for part in names.into_iter().rev() {
        if root.file_name().and_then(|name| name.to_str()) != Some(part) {
            return parent.to_path_buf();
        }
        root = root.parent().unwrap_or(Path::new("."));
    }
    root.to_path_buf()
}
fn run(args: &[String]) -> Result<(), String> {
    let start = Instant::now();
    if args.len() == 1 && ["--help", "-h"].contains(&args[0].as_str()) {
        println!(
            "bifoilc module SOURCE --name MODULE --out FILE [--module-path DIR]\nbifoilc build PROJECT --out STARTUP.txt [--module-path DIR] [--clean] [--package]\nbifoilc format SOURCE [--out FILE | --check]\nProfile: {PROFILE}; native AOT compiler; extended ChoiceScript output."
        );
        return Ok(());
    }
    if args.len() < 2 {
        return Err("use bifoilc --help".into());
    }
    let command = &args[0];
    if command == "format" {
        return source_format::command(&args[1..], start);
    }
    let input = PathBuf::from(&args[1]);
    let mut options = BTreeMap::new();
    let mut i = 2;
    let mut clean = false;
    let mut packaged = false;
    while i < args.len() {
        if args[i] == "--clean" && !clean {
            clean = true;
            i += 1;
            continue;
        }
        if args[i] == "--package" && !packaged {
            packaged = true;
            i += 1;
            continue;
        }
        if !["--out", "--name", "--module-path"].contains(&args[i].as_str())
            || i + 1 >= args.len()
            || options.insert(args[i].clone(), args[i + 1].clone()).is_some()
        {
            return Err("unknown, repeated, or incomplete command option".into());
        }
        i += 2;
    }
    let out = PathBuf::from(options.remove("--out").ok_or("--out is required")?);
    match command.as_str() {
        "module" => {
            if clean || packaged {
                return Err("--clean and --package are only valid for project builds".into());
            }
            let name = options
                .remove("--name")
                .ok_or("--name is required for module compilation")?;
            let root = source_root(&input, &name);
            let artifacts = options
                .remove("--module-path")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("modules"));
            let mut loader = Loader::new(root, artifacts, start);
            loader.load(&name, Some(&input))?;
            let prepared = {
                let mut module = loader.modules.remove(&name).unwrap();
                let headers = std::mem::take(&mut module.functions);
                let stored = module.with_functions(loader.code.functions(&headers));
                artifact::prepare_json(&out, &stored, "", "", start)?
            };
            drop(loader);
            prepared.commit(|_| Ok(()))?;
        }
        "build" => {
            if options.contains_key("--name") {
                return Err("--name is only valid for module compilation".into());
            }
            let manifest::Manifest {
                entry,
                module_path,
                legacy_path,
            } = manifest::read(&input)?;
            let artifacts = options
                .remove("--module-path")
                .map(PathBuf::from)
                .unwrap_or_else(|| input.join(module_path.unwrap_or("modules".into())));
            project::build(project::Request {
                project: &input,
                output: &out,
                artifacts,
                entry,
                legacy_path,
                clean,
                packaged,
                start,
            })?;
        }
        _ => return Err("unknown command; use bifoilc --help".into()),
    }
    println!(
        "built {} in {:.3}s ({PROFILE})",
        out.display(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
fn main() {
    if let Err(error) = resources::initialize().and_then(|_| run(&std::env::args().skip(1).collect::<Vec<_>>())) {
        eprintln!("bifoilc: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests;
