//! Append complete functions while retaining one buffered spool writer.

use super::counting_writer::CountingWriter;
use super::encoded::EncodedFunction;
use super::record_index::RecordIndex;
use super::{LegacyReference, Location};
use crate::model::{Function, Op};
use serde::Serialize;
use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::sync::Arc;

pub(super) struct CodeWriter<'a> {
    writer: CountingWriter<BufWriter<&'a mut File>>,
    entries: &'a mut RecordIndex,
    instructions: &'a mut usize,
    legacy: &'a mut Vec<LegacyReference>,
}

impl<'a> CodeWriter<'a> {
    pub(super) fn new(
        file: &'a mut File,
        entries: &'a mut RecordIndex,
        instructions: &'a mut usize,
        legacy: &'a mut Vec<LegacyReference>,
    ) -> Result<Self, String> {
        let start = file.seek(SeekFrom::End(0)).map_err(|error| error.to_string())?;
        Ok(Self {
            writer: CountingWriter::new(BufWriter::with_capacity(65536, file), start),
            entries,
            instructions,
            legacy,
        })
    }

    pub(super) fn record(&mut self, id: &str, function: &Function, collect_legacy: bool) -> Result<(), String> {
        self.append(id, function, function.code.len(), super::header_checksum(function)?)?;
        if collect_legacy {
            self.collect_legacy(id, function);
        }
        Ok(())
    }

    /// Analyze a typed view, then retain its unchanged canonical representation.
    pub(super) fn record_existing(
        &mut self,
        id: &str,
        encoded: &EncodedFunction,
        function: &Function,
    ) -> Result<(), String> {
        self.append(id, encoded, function.code.len(), super::header_checksum(function)?)?;
        self.collect_legacy(id, function);
        Ok(())
    }

    fn collect_legacy(&mut self, id: &str, function: &Function) {
        let mut owner = None;
        for instruction in &function.code {
            if let Op::LegacyCall { target, .. } = &instruction.op {
                let owner = owner.get_or_insert_with(|| Arc::<str>::from(id));
                self.legacy.push(LegacyReference {
                    function: Arc::clone(owner),
                    target: target.clone(),
                    span: instruction.span.clone(),
                });
            }
        }
    }

    pub(super) fn record_encoded(
        &mut self,
        id: &str,
        function: &EncodedFunction,
        instructions: usize,
        header_checksum: [u8; 32],
    ) -> Result<(), String> {
        self.append(id, function, instructions, header_checksum)
    }

    fn append(
        &mut self,
        id: &str,
        function: &impl Serialize,
        instructions: usize,
        header_checksum: [u8; 32],
    ) -> Result<(), String> {
        let vacant = self.entries.vacant(id.into())?;
        let offset = self.writer.position();
        serde_json::to_writer(&mut self.writer, function).map_err(|error| error.to_string())?;
        let end = self.writer.position();
        vacant.insert(Location {
            offset,
            length: end - offset,
            instructions,
            checksum: self.writer.take_checksum(),
            header_checksum,
        })?;
        *self.instructions += instructions;
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<(), String> {
        self.writer.flush().map_err(|error| error.to_string())?;
        self.entries.finish()
    }
}

/// Input parsing owns one writer and exposes no legacy-publication controls.
pub(crate) struct PendingWriter<'a>(CodeWriter<'a>);
impl<'a> PendingWriter<'a> {
    pub(super) fn new(writer: CodeWriter<'a>) -> Self {
        Self(writer)
    }
    pub(crate) fn record(&mut self, id: &str, function: &Function) -> Result<(), String> {
        self.0.record(id, function, false)
    }
    pub(crate) fn finish(self) -> Result<(), String> {
        self.0.finish()
    }
}
