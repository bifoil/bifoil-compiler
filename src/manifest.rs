//! Typed project configuration using standard TOML string and comment rules.

use serde::Deserialize;
use std::path::Path;

const MAX_MANIFEST_BYTES: u64 = 16 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub(crate) entry: String,
    pub(crate) module_path: Option<String>,
    pub(crate) legacy_path: Option<String>,
}

pub(crate) fn read(project: &Path) -> Result<Manifest, String> {
    let path = project.join("bifoil.toml");
    let mut total = 0;
    let source = crate::input::read(&path, MAX_MANIFEST_BYTES, &mut total)?;
    let manifest: Manifest = toml::from_str(&source).map_err(|error| format!("{}: {error}", path.display()))?;
    if !crate::module_name(&manifest.entry) || !manifest.entry.contains('.') {
        return Err(format!("{}: entry must be module.scene", path.display()));
    }
    Ok(manifest)
}
