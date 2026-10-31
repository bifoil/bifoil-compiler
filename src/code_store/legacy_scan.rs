//! Identify legacy operations without reconstructing their typed instruction bodies.
//!
//! Only validated pending functions enter this path. Real spool checksums and
//! expected headers are checked before scanning; this is not input validation.

use serde::de::{IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
use std::fmt;

#[derive(Deserialize)]
struct Operation<'a> {
    #[serde(borrow)]
    op: Cow<'a, str>,
}

pub(super) fn contains(code: &str) -> Result<bool, String> {
    struct LegacyCalls;
    impl<'de> Visitor<'de> for LegacyCalls {
        type Value = bool;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a stored instruction array")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> Result<bool, A::Error> {
            while let Some(instruction) = values.next_element::<Operation<'de>>()? {
                if instruction.op == "legacy_call" {
                    // Consume the remaining array so malformed JSON still fails.
                    while values.next_element::<IgnoredAny>()?.is_some() {}
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
    serde_json::Deserializer::from_str(code)
        .deserialize_seq(LegacyCalls)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn operation_tags_use_decoded_json_and_ignore_unrelated_strings() {
        assert!(!super::contains(r#"[]"#).unwrap());
        assert!(!super::contains(r#"[{"op":"constant","value":"legacy_call"}]"#).unwrap());
        assert!(super::contains(r#"[{"op":"legacy_call"},{"op":"return"}]"#).unwrap());
        assert!(super::contains(r#"[{"op":"legacy_\u0063all"}]"#).unwrap());
        assert!(super::contains(r#"[{"op":"legacy_call"},"#).is_err());
        assert!(super::contains(r#"[{"value":"legacy_call"}]"#).is_err());
    }
}
