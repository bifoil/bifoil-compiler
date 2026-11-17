//! Small checked runtime declarations, extracted without allocating whole bodies.
//! Only capture sites and resumable instruction positions remain in a package root.

use crate::model::{Instruction, Signature};
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Serialize)]
pub(crate) struct RuntimeHeader<'a> {
    pub(crate) module: &'a str,
    pub(crate) signature: Signature,
    pub(crate) init: bool,
    pub(crate) local: bool,
    pub(crate) instructions: usize,
    pub(crate) continuations: BTreeMap<usize, Instruction>,
    pub(crate) captures: Vec<Instruction>,
}

#[derive(Default)]
pub(super) struct CodeMetadata {
    pub(super) instructions: usize,
    pub(super) continuations: BTreeMap<usize, Instruction>,
    pub(super) captures: Vec<Instruction>,
}

#[derive(Deserialize)]
struct Keyword<'a> {
    #[serde(borrow)]
    op: &'a str,
}

pub(super) fn scan(code: &str) -> Result<CodeMetadata, String> {
    let mut decoder = serde_json::Deserializer::from_str(code);
    decoder
        .deserialize_seq(MetadataVisitor)
        .map_err(|error| error.to_string())
}

struct MetadataVisitor;
impl<'de> Visitor<'de> for MetadataVisitor {
    type Value = CodeMetadata;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("checked compiled instruction metadata")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> Result<CodeMetadata, A::Error> {
        let mut metadata = CodeMetadata::default();
        while let Some(raw) = values.next_element::<&RawValue>()? {
            let keyword: Keyword<'_> = serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
            let position = metadata.instructions;
            metadata.instructions += 1;
            if ["closure", "choose", "legacy_call", "stack_mark"].contains(&keyword.op) {
                let instruction = serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
                if keyword.op == "closure" {
                    metadata.captures.push(instruction);
                } else {
                    metadata.continuations.insert(position, instruction);
                }
            }
        }
        Ok(metadata)
    }
}
