//! Checked canonical function bytes for serialization without typed decoding.
//!
//! These bytes are produced by our own writer, then checksum-verified by the
//! store. Keep instruction counts checked independently of record metadata.

use serde::de::{IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;
use std::fmt;
use std::sync::{Arc, OnceLock, Weak};

pub(crate) struct EncodedFunction {
    value: Box<RawValue>,
    instructions: usize,
    header_checksum: [u8; 32],
    checked_header: OnceLock<CheckedHeader>,
}

struct CheckedHeader {
    signature: Weak<crate::model::SignatureData>,
    init: bool,
    local: bool,
}
impl CheckedHeader {
    fn matches(&self, header: &crate::model::Function) -> bool {
        self.init == header.init
            && self.local == header.local
            && self
                .signature
                .upgrade()
                .is_some_and(|known| Arc::ptr_eq(&known, &header.signature))
    }
}

#[derive(Deserialize)]
struct Body<'a> {
    #[serde(borrow)]
    signature: &'a RawValue,
    #[serde(borrow)]
    code: &'a RawValue,
    init: bool,
    #[serde(default)]
    local: bool,
}

impl EncodedFunction {
    pub(super) fn new(bytes: String, expected: usize) -> Result<Self, String> {
        let value = RawValue::from_string(bytes).map_err(|error| error.to_string())?;
        let body: Body<'_> = serde_json::from_str(value.get()).map_err(|error| error.to_string())?;
        let mut decoder = serde_json::Deserializer::from_str(body.code.get());
        let count = decoder
            .deserialize_seq(InstructionCount)
            .map_err(|error| error.to_string())?;
        if count != expected {
            return Err("stored instruction count changed".into());
        }
        let header_checksum = crate::artifact::digest_bytes(&(body.signature, body.init, body.local))?;
        Ok(Self {
            value,
            instructions: count,
            header_checksum,
            checked_header: OnceLock::new(),
        })
    }

    pub(super) fn check_record(&self, id: &str, instructions: usize, header_checksum: [u8; 32]) -> Result<(), String> {
        if self.instructions != instructions {
            return Err(format!("stored function {id}: stored instruction count changed"));
        }
        if self.header_checksum != header_checksum {
            return Err(format!("stored function header changed: {id}"));
        }
        Ok(())
    }

    pub(super) fn check_checksum(&self, id: &str, expected: &[u8; 32]) -> Result<(), String> {
        if self.header_checksum != *expected {
            return Err(format!("stored function header changed: {id}"));
        }
        Ok(())
    }

    pub(super) fn check_header(&self, id: &str, header: &crate::model::Function) -> Result<(), String> {
        if header.signature.id != id {
            return Err(format!("stored function header changed: {id}"));
        }
        if self.checked_header.get().is_some_and(|known| known.matches(header)) {
            return Ok(());
        }
        if super::header_checksum(header)? != self.header_checksum {
            return Err(format!("stored function header changed: {id}"));
        }
        // Weak references cannot keep parameter/type graphs alive. They also
        // prevent allocation reuse; copy-on-write invalidates an old witness.
        let _ = self.checked_header.set(CheckedHeader {
            signature: Arc::downgrade(&header.signature),
            init: header.init,
            local: header.local,
        });
        Ok(())
    }

    pub(super) fn header(&self) -> Result<super::FunctionHeader, String> {
        let header: super::FunctionHeader =
            serde_json::from_str(self.value.get()).map_err(|error| error.to_string())?;
        let checksum = crate::artifact::digest_bytes(&(&header.signature, header.init, header.local))?;
        if checksum != self.header_checksum {
            return Err("stored header representation changed".into());
        }
        Ok(header)
    }

    pub(super) fn byte_len(&self) -> usize {
        self.value.get().len()
    }

    pub(super) fn has_legacy_calls(&self) -> Result<bool, String> {
        let body = self.body().map_err(|error| error.to_string())?;
        super::legacy_scan::contains(body.code.get())
    }

    pub(super) fn decoded(&self) -> Result<crate::model::Function, String> {
        serde_json::from_str(self.value.get()).map_err(|error| error.to_string())
    }

    fn body(&self) -> Result<Body<'_>, serde_json::Error> {
        serde_json::from_str(self.value.get())
    }

    pub(crate) fn serialize_code<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.body()
            .map_err(serde::ser::Error::custom)?
            .code
            .serialize(serializer)
    }
}

impl Serialize for EncodedFunction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

struct InstructionCount;
impl<'de> Visitor<'de> for InstructionCount {
    type Value = usize;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a stored instruction array")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> Result<usize, A::Error> {
        let mut count = 0;
        while values.next_element::<IgnoredAny>()?.is_some() {
            count += 1;
        }
        Ok(count)
    }
}
