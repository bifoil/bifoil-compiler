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
    parts: FunctionParts,
    byte_len: usize,
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

/// These are the canonical writer's four fields, in serialization order.
/// Keeping the checked raw fields avoids reparsing a long literal merely to
/// retrieve a signature or serialize an already validated instruction array.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FunctionParts {
    signature: Box<RawValue>,
    code: Box<RawValue>,
    init: bool,
    #[serde(default)]
    local: bool,
}

impl EncodedFunction {
    pub(super) fn new(bytes: String, expected: usize) -> Result<Self, String> {
        let byte_len = bytes.len();
        let parts: FunctionParts = serde_json::from_str(&bytes).map_err(|error| error.to_string())?;
        let mut decoder = serde_json::Deserializer::from_str(parts.code.get());
        let count = decoder
            .deserialize_seq(InstructionCount)
            .map_err(|error| error.to_string())?;
        if count != expected {
            return Err("stored instruction count changed".into());
        }
        let header_checksum = crate::artifact::digest_bytes(&(&parts.signature, parts.init, parts.local))?;
        Ok(Self {
            parts,
            byte_len,
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
        let header = super::FunctionHeader {
            signature: serde_json::from_str(self.parts.signature.get()).map_err(|error| error.to_string())?,
            init: self.parts.init,
            local: self.parts.local,
        };
        let checksum = crate::artifact::digest_bytes(&(&header.signature, header.init, header.local))?;
        if checksum != self.header_checksum {
            return Err("stored header representation changed".into());
        }
        Ok(header)
    }

    pub(super) fn runtime_header<'a>(&self, module: &'a str) -> Result<super::RuntimeHeader<'a>, String> {
        let header = self.header()?;
        let metadata = super::runtime_header::scan(self.parts.code.get())?;
        if metadata.instructions != self.instructions {
            return Err("stored runtime instruction count changed".into());
        }
        Ok(super::RuntimeHeader {
            module,
            signature: header.signature,
            init: header.init,
            local: header.local,
            instructions: metadata.instructions,
            continuations: metadata.continuations,
            captures: metadata.captures,
        })
    }

    pub(super) fn byte_len(&self) -> usize {
        self.byte_len
    }

    pub(super) fn has_legacy_calls(&self) -> Result<bool, String> {
        super::legacy_scan::contains(self.parts.code.get())
    }

    pub(super) fn decoded(&self) -> Result<crate::model::Function, String> {
        Ok(crate::model::Function {
            signature: serde_json::from_str(self.parts.signature.get()).map_err(|error| error.to_string())?,
            code: serde_json::from_str(self.parts.code.get()).map_err(|error| error.to_string())?,
            init: self.parts.init,
            local: self.parts.local,
        })
    }

    pub(crate) fn serialize_code<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.parts.code.serialize(serializer)
    }
}

impl Serialize for EncodedFunction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.parts.serialize(serializer)
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

#[cfg(test)]
mod tests {
    use super::EncodedFunction;
    use std::collections::BTreeMap;
    use std::time::Instant;

    #[test]
    fn checked_parts_keep_canonical_function_bytes_and_reject_unrepresented_fields() {
        let module = crate::compiler::compile(
            crate::parser::parse(
                "export fun text() -> String = \"line \\\"quoted\\\" ${3.0}\"\n",
                "src/parts.bfl",
            )
            .unwrap(),
            "parts",
            "src/parts.bfl",
            &BTreeMap::new(),
            Instant::now(),
        )
        .unwrap();
        let function = &module.functions["parts.text"];
        let original = serde_json::to_string(function).unwrap();
        let encoded = EncodedFunction::new(original.clone(), function.code.len()).unwrap();
        assert_eq!(serde_json::to_string(&encoded).unwrap(), original);
        assert_eq!(serde_json::to_string(&encoded.decoded().unwrap()).unwrap(), original);
        assert_eq!(encoded.byte_len(), original.len());
        assert!(!encoded.has_legacy_calls().unwrap());
        assert_eq!(encoded.header().unwrap().signature, function.signature);
        assert_eq!(
            encoded.runtime_header("parts").unwrap().instructions,
            function.code.len()
        );

        for prefix in ["{\"unknown\":true,", "{\"init\":false,"] {
            let changed = prefix.to_owned() + &original[1..];
            assert!(EncodedFunction::new(changed, function.code.len()).is_err());
        }
    }
}
