//! Bounded source reads and streaming artifact decoding from an opened file.
//!
//! Limits apply to the opened handle, rather than a separate path metadata query.
//! Reading one extra byte detects files that grow during loading without accepting
//! a truncated prefix or allocating an unbounded input buffer.

use serde::de::{DeserializeOwned, DeserializeSeed};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read, Take};
use std::path::Path;
use std::time::Instant;

struct Observed<R> {
    inner: R,
    hash: Sha256,
}
impl<R: Read> Read for Observed<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let count = self.inner.read(bytes)?;
        self.hash.update(&bytes[..count]);
        Ok(count)
    }
}
struct Input {
    reader: BufReader<Observed<Take<File>>>,
    size: u64,
}
impl Input {
    fn open(path: &Path, limit: u64, aggregate: Option<&mut u64>) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let size = file.metadata().map_err(|error| error.to_string())?.len();
        account(path, size, limit, aggregate)?;
        Ok(Self {
            reader: BufReader::with_capacity(
                65536,
                Observed {
                    inner: file.take(size + 1),
                    hash: Sha256::new(),
                },
            ),
            size,
        })
    }
    fn unchanged_size(&self, path: &Path) -> Result<(), String> {
        let consumed = self.size + 1 - self.reader.get_ref().inner.limit();
        if consumed == self.size {
            Ok(())
        } else {
            Err(format!(
                "{}: input size changed while reading; retry compilation",
                path.display()
            ))
        }
    }
}
/// Charge a previously bounded byte buffer exactly as an opened input file.
pub(crate) fn account(path: &Path, size: u64, limit: u64, aggregate: Option<&mut u64>) -> Result<(), String> {
    if size > limit {
        return Err(format!("{}: input size budget exceeded", path.display()));
    }
    if let Some(total) = aggregate {
        if total.checked_add(size).is_none_or(|value| value > crate::MAX_INPUT) {
            return Err(format!("{}: input size budget exceeded", path.display()));
        }
        *total += size;
    }
    Ok(())
}

pub(crate) fn bytes(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut input = Input::open(path, limit, None)?;
    let mut bytes = Vec::with_capacity(input.size as usize);
    input
        .reader
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    input.unchanged_size(path)?;
    Ok(bytes)
}

pub(crate) fn read(path: &Path, limit: u64, aggregate: &mut u64) -> Result<String, String> {
    let mut input = Input::open(path, limit, Some(aggregate))?;
    let mut text = String::new();
    input
        .reader
        .read_to_string(&mut text)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    input.unchanged_size(path)?;
    Ok(text)
}
pub(crate) fn json<T: DeserializeOwned>(path: &Path, limit: u64, aggregate: Option<&mut u64>) -> Result<T, String> {
    json_hashed(path, limit, aggregate).map(|(value, _)| value)
}
/// The checksum covers exactly the bytes decoded and validated, including whitespace.
pub(crate) fn json_hashed<T: DeserializeOwned>(
    path: &Path,
    limit: u64,
    aggregate: Option<&mut u64>,
) -> Result<(T, String), String> {
    let mut input = Input::open(path, limit, aggregate)?;
    let value = serde_json::from_reader(&mut input.reader).map_err(|error| format!("{}: {error}", path.display()))?;
    input.unchanged_size(path)?;
    let checksum = input
        .reader
        .into_inner()
        .hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((value, checksum))
}
/// A caller-owned seed can spool individual values while the bounded reader
/// advances. Finish the JSON document before accepting its checksum or contents.
pub(crate) fn json_seed_hashed<'de, S: DeserializeSeed<'de>>(
    path: &Path,
    limit: u64,
    aggregate: Option<&mut u64>,
    seed: S,
) -> Result<(S::Value, String), String> {
    let mut input = Input::open(path, limit, aggregate)?;
    let mut deserializer = serde_json::Deserializer::from_reader(&mut input.reader);
    let value = seed
        .deserialize(&mut deserializer)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    deserializer
        .end()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    input.unchanged_size(path)?;
    let checksum = input
        .reader
        .into_inner()
        .hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((value, checksum))
}
/// Decode an immutable, bounded preloaded input with the same seed and whole-byte checksum.
pub(crate) fn json_seed_bytes_hashed<'de, S: DeserializeSeed<'de>>(
    bytes: &'de [u8],
    limit: u64,
    seed: S,
) -> Result<(S::Value, String), String> {
    if bytes.len() as u64 > limit {
        return Err("input size budget exceeded".into());
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = seed.deserialize(&mut deserializer).map_err(|error| error.to_string())?;
    deserializer.end().map_err(|error| error.to_string())?;
    let checksum = Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect();
    Ok((value, checksum))
}

pub(crate) fn hash(path: &Path, limit: u64, aggregate: Option<&mut u64>, start: Instant) -> Result<String, String> {
    let mut input = Input::open(path, limit, aggregate)?;
    let mut buffer = [0u8; 65536];
    loop {
        crate::budget(start)?;
        let count = input
            .reader
            .read(&mut buffer)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
    }
    input.unchanged_size(path)?;
    Ok(input
        .reader
        .into_inner()
        .hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Temporary(PathBuf);
    impl Drop for Temporary {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    fn input(bytes: &[u8]) -> Temporary {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bifoil-input-test-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
        file.write_all(bytes).unwrap();
        Temporary(path)
    }
    #[test]
    fn json_artifact_input_preserves_finite_binary64_bits() {
        let mut values = vec![0.0, -0.0, f64::MIN_POSITIVE, f64::MAX, -f64::MAX, f64::from_bits(1)];
        let mut state = 0x6a09_e667_f3bc_c909_u64;
        for _ in 0..8192 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let value = f64::from_bits(state);
            if value.is_finite() {
                values.push(value);
            }
        }
        let bytes = serde_json::to_vec(&values).unwrap();
        let file = input(&bytes);
        let decoded: Vec<f64> = json(&file.0, 1024 * 1024, None).unwrap();
        assert_eq!(decoded.len(), values.len());
        for (expected, actual) in values.iter().zip(decoded) {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "artifact changed {expected:?} to {actual:?}"
            );
        }
    }

    #[test]
    fn streamed_json_preserves_unicode_and_aggregate_accounting() {
        let file = input("{\"text\":\"😀\",\"value\":2}  \n".as_bytes());
        let mut total = 5;
        let value: serde_json::Value = json(&file.0, 1024, Some(&mut total)).unwrap();
        assert_eq!(value["text"], "😀");
        assert_eq!(value["value"], 2);
        assert_eq!(total, 5 + fs::metadata(&file.0).unwrap().len());
    }
    #[test]
    fn checksums_cover_the_exact_decoded_bytes() {
        let bytes = "{\"text\": \"😀\"}  \n".as_bytes();
        let file = input(bytes);
        let expected = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let (_, observed): (serde_json::Value, String) = json_hashed(&file.0, 1024, None).unwrap();
        assert_eq!(observed, expected);
        assert_eq!(hash(&file.0, 1024, None, Instant::now()).unwrap(), expected);
    }
    #[test]
    fn source_and_artifact_readers_enforce_size_and_encoding() {
        let file = input(b"{\"value\": 2}");
        assert!(
            json::<serde_json::Value>(&file.0, 3, None)
                .unwrap_err()
                .contains("size budget exceeded")
        );
        let mut total = crate::MAX_INPUT;
        assert!(
            read(&file.0, 1024, &mut total)
                .unwrap_err()
                .contains("size budget exceeded")
        );
        let invalid = input(b"\"\xff\"");
        assert!(json::<serde_json::Value>(&invalid.0, 1024, None).is_err());
        assert!(read(&invalid.0, 1024, &mut 0).is_err());
    }
    #[test]
    fn aggregate_accounting_accepts_the_boundary_and_rejects_overflow_without_charging() {
        let path = Path::new("source.bfl");
        let mut total = crate::MAX_INPUT - 7;
        account(path, 7, 7, Some(&mut total)).unwrap();
        assert_eq!(total, crate::MAX_INPUT);
        assert!(account(path, 1, 7, Some(&mut total)).is_err());
        assert_eq!(total, crate::MAX_INPUT);
        let mut overflow = u64::MAX - 1;
        assert!(account(path, 2, 7, Some(&mut overflow)).is_err());
        assert_eq!(overflow, u64::MAX - 1);
        assert!(account(path, 8, 7, None).is_err());
    }
}
