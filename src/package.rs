//! Independently reusable code scenes, published before their small root manifest.
//! Chunk names depend on code and signatures, independent of dependency digests.
//! Valid immutable chunks remain reusable after failures; deleting them could
//! invalidate another concurrent build that has already published its root.

use crate::artifact;
use crate::code_store::{CodeStore, StoredFunctions};
use crate::function_catalog::{FunctionCatalog, HeaderDigest};
use crate::model::{PROFILE, Program};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

pub(crate) const FORMAT: &str = "bifoil.package.v1";
const CHUNK_FORMAT: &str = "bifoil.code.v1";
const CHUNK_PREFIX: &str = "*comment Bifoil compiled code scene\n*bifoil_module ";
const CHUNK_SUFFIX: &str = "\n*return\n";

pub(crate) struct Sources<'a> {
    pub(crate) code: &'a CodeStore,
    pub(crate) functions: &'a FunctionCatalog<HeaderDigest>,
}
#[derive(Serialize)]
struct Chunk<'a> {
    format: &'static str,
    profile: &'static str,
    module: &'a str,
    functions: StoredFunctions<'a, BTreeMap<String, HeaderDigest>>,
}
#[derive(Serialize)]
struct Reference {
    scene: String,
    module_digest: String,
}
#[derive(Serialize)]
struct Manifest<'a, F> {
    #[serde(flatten)]
    program: &'a Program<F>,
    chunks: &'a BTreeMap<String, Reference>,
}

pub(crate) fn prepare<F: Serialize>(
    output: &Path,
    program: &mut Program<F>,
    sources: Sources<'_>,
    start: Instant,
) -> Result<Prepared, String> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut references = BTreeMap::new();
    let mut checksums = BTreeMap::new();
    for (module, headers) in sources.functions.groups() {
        crate::budget(start)?;
        // At most one independently bounded module is cached. Every encoded
        // read still checks the spool; dropping the scope releases all entries.
        let _records = sources.code.record_scope(headers.keys().map(String::as_str))?;
        let _code_cache = sources.code.cache_scope(crate::model::MAX_ARTIFACT as usize)?;
        let chunk = Chunk {
            format: CHUNK_FORMAT,
            profile: PROFILE,
            module,
            functions: sources.code.functions(headers),
        };
        crate::trace::event(start, format_args!("hashing code {module}"));
        let scene = format!("bifoil_code_{}", artifact::digest(&chunk)?);
        let path = parent.join(format!("{scene}.txt"));
        let checksum = match artifact::matching_json_checksum(&path, &chunk, CHUNK_PREFIX, CHUNK_SUFFIX, start)? {
            Some(checksum) => checksum,
            None => {
                artifact::write_json(&path, &chunk, CHUNK_PREFIX, CHUNK_SUFFIX, start)?;
                crate::input::hash(&path, crate::model::MAX_ARTIFACT, None, start)?
            }
        };
        crate::trace::event(start, format_args!("published code {module}"));
        checksums.insert(format!("{scene}.txt"), checksum);
        references.insert(
            module.to_owned(),
            Reference {
                scene,
                module_digest: program
                    .modules
                    .get(module)
                    .ok_or("missing package module identity")?
                    .clone(),
            },
        );
    }
    crate::trace::event(start, format_args!("preparing root"));
    program.build = artifact::digest(&Manifest {
        program,
        chunks: &references,
    })?;
    let manifest = Manifest {
        program,
        chunks: &references,
    };
    let mut prefix = format!("*comment Bifoil AOT package; requires {FORMAT}\n");
    for reference in references.values() {
        prefix += &format!("*gosub_scene {}\n", reference.scene);
    }
    prefix += "*bifoil ";
    let root = match artifact::matching_json_checksum(output, &manifest, &prefix, "\n", start)? {
        Some(checksum) => Root::Existing(checksum),
        None => Root::Pending(artifact::prepare_json(output, &manifest, &prefix, "\n", start)?),
    };
    Ok(Prepared { root, checksums, start })
}

enum Root {
    Existing(String),
    Pending(artifact::PreparedJson),
}
#[must_use]
pub(crate) struct Prepared {
    root: Root,
    checksums: BTreeMap<String, String>,
    start: Instant,
}
impl Prepared {
    pub(crate) fn commit(
        self,
        before_commit: impl FnOnce(&str, BTreeMap<String, String>) -> Result<(), String>,
    ) -> Result<(), String> {
        match self.root {
            Root::Pending(file) => file.commit(|checksum| before_commit(checksum, self.checksums))?,
            Root::Existing(checksum) => {
                crate::budget(self.start)?;
                before_commit(&checksum, self.checksums)?;
                crate::budget(self.start)?;
            }
        }
        crate::trace::event(self.start, format_args!("root published"));
        Ok(())
    }
}
