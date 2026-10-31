//! Canonical streaming hashes and bounded atomic artifact output.
//!
//! Serialization must not create another whole-program buffer. Temporary files
//! are owned exclusively and replaced only after serialization and flushing pass.

mod buffered_hash;

use crate::model::MAX_ARTIFACT;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

// Internal module/group identities can include interfaces and summaries that
// are absent from the final program. Allow that overhead, while bounding a
// compact type graph's potentially much larger expanded JSON representation.
const MAX_CANONICAL_HASH_BYTES: u64 = MAX_ARTIFACT * 4;
struct HashWriter {
    hash: buffered_hash::BufferedHash,
    written: u64,
}
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.written = self
            .written
            .checked_add(bytes.len() as u64)
            .filter(|written| *written <= MAX_CANONICAL_HASH_BYTES)
            .ok_or_else(|| io::Error::other("canonical data size budget exceeded"))?;
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.hash.flush();
        Ok(())
    }
}
/// Private identities retain digest bytes; public formats use lowercase hex.
pub(crate) fn digest_bytes<T: Serialize>(value: &T) -> Result<[u8; 32], String> {
    let mut writer = HashWriter {
        hash: buffered_hash::BufferedHash::new(),
        written: 0,
    };
    serde_json::to_writer(&mut writer, value).map_err(|error| error.to_string())?;
    Ok(writer.hash.finalize())
}
pub(crate) fn digest<T: Serialize>(value: &T) -> Result<String, String> {
    use std::fmt::Write;
    let mut result = String::with_capacity(64);
    for byte in digest_bytes(value)? {
        write!(&mut result, "{byte:02x}").expect("formatting into a String cannot fail");
    }
    Ok(result)
}

struct BoundedWriter {
    inner: BufWriter<File>,
    written: u64,
    checked: u64,
    start: Instant,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .written
            .checked_add(bytes.len() as u64)
            .is_none_or(|size| size > MAX_ARTIFACT)
        {
            return Err(io::Error::other("output size budget exceeded"));
        }
        if self.written.saturating_sub(self.checked) >= 65536 {
            crate::budget(self.start).map_err(io::Error::other)?;
            self.checked = self.written;
        }
        let count = self.inner.write(bytes)?;
        self.written += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
fn temporary(parent: &Path) -> Result<(PathBuf, File), String> {
    loop {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".bifoil-{}-{id}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
}
fn atomic(path: &Path, start: Instant, write: impl FnOnce(&mut dyn Write) -> Result<(), String>) -> Result<(), String> {
    crate::budget(start)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let (temporary, file) = temporary(parent)?;
    let result = (|| {
        let mut writer = BoundedWriter {
            inner: BufWriter::with_capacity(65536, file),
            written: 0,
            checked: 0,
            start,
        };
        write(&mut writer)?;
        writer.flush().map_err(|error| error.to_string())?;
        drop(writer);
        crate::budget(start)?;
        fs::rename(&temporary, path).map_err(|error| format!("{}: {error}", path.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
pub(crate) fn write_text(path: &Path, text: &str, start: Instant) -> Result<(), String> {
    atomic(path, start, |writer| {
        writer.write_all(text.as_bytes()).map_err(|error| error.to_string())
    })
}
pub(crate) fn write_json<T: Serialize>(
    path: &Path,
    value: &T,
    prefix: &str,
    suffix: &str,
    start: Instant,
) -> Result<(), String> {
    atomic(path, start, |writer| {
        writer.write_all(prefix.as_bytes()).map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut *writer, value).map_err(|error| error.to_string())?;
        writer.write_all(suffix.as_bytes()).map_err(|error| error.to_string())
    })
}

/// Compare generated bytes and return their verified whole-file checksum in one pass.
/// A different or missing file can be replaced safely; no second encoded artifact is retained.
pub(crate) fn matching_json_checksum<T: Serialize>(
    path: &Path,
    value: &T,
    prefix: &str,
    suffix: &str,
    start: Instant,
) -> Result<Option<String>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    struct Compare {
        reader: BufReader<File>,
        hash: Sha256,
        equal: bool,
        written: u64,
        checked: u64,
        start: Instant,
    }
    impl Write for Compare {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.written = self
                .written
                .checked_add(bytes.len() as u64)
                .filter(|size| *size <= MAX_ARTIFACT)
                .ok_or_else(|| io::Error::other("output size budget exceeded"))?;
            if self.written.saturating_sub(self.checked) >= 65536 {
                crate::budget(self.start).map_err(io::Error::other)?;
                self.checked = self.written;
            }
            let mut remaining = bytes;
            while !remaining.is_empty() {
                let previous = self.reader.fill_buf()?;
                if previous.is_empty() {
                    self.equal = false;
                    return Err(io::Error::other("previous output differs"));
                }
                let count = previous.len().min(remaining.len());
                if previous[..count] != remaining[..count] {
                    self.equal = false;
                    return Err(io::Error::other("previous output differs"));
                }
                self.reader.consume(count);
                remaining = &remaining[count..];
            }
            self.hash.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut compare = Compare {
        reader: BufReader::with_capacity(65536, file),
        hash: Sha256::new(),
        equal: true,
        written: 0,
        checked: 0,
        start,
    };
    let compared = (|| {
        compare
            .write_all(prefix.as_bytes())
            .map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut compare, value).map_err(|error| error.to_string())?;
        compare.write_all(suffix.as_bytes()).map_err(|error| error.to_string())
    })();
    if !compare.equal {
        return Ok(None);
    }
    compared?;
    let ended = compare.reader.fill_buf().map_err(|error| error.to_string())?.is_empty();
    crate::budget(start)?;
    if !ended {
        return Ok(None);
    }
    Ok(Some(
        compare
            .hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    ))
}

/// A completed temporary output. Dropping it before commit preserves the old file.
#[must_use]
pub(crate) struct PreparedJson {
    temporary: Option<PathBuf>,
    output: PathBuf,
    checksum: String,
    start: Instant,
}
impl PreparedJson {
    pub(crate) fn commit(mut self, before_commit: impl FnOnce(&str) -> Result<(), String>) -> Result<(), String> {
        crate::budget(self.start)?;
        before_commit(&self.checksum)?;
        crate::budget(self.start)?;
        fs::rename(self.temporary.as_ref().unwrap(), &self.output)
            .map_err(|error| format!("{}: {error}", self.output.display()))?;
        self.temporary.take();
        Ok(())
    }
}
impl Drop for PreparedJson {
    fn drop(&mut self) {
        if let Some(path) = self.temporary.take() {
            let _ = fs::remove_file(path);
        }
    }
}

/// Complete bounded output before callers release borrowed analysis/code storage.
/// The later commit checks the deadline again before replacing the destination.
pub(crate) fn prepare_json<T: Serialize>(
    path: &Path,
    value: &T,
    prefix: &str,
    suffix: &str,
    start: Instant,
) -> Result<PreparedJson, String> {
    struct Checked<'a> {
        inner: &'a mut dyn Write,
        hash: Sha256,
    }
    impl Write for Checked<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let count = self.inner.write(bytes)?;
            self.hash.update(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }
    crate::budget(start)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let (temporary, file) = temporary(parent)?;
    let written = (|| {
        let mut writer = BoundedWriter {
            inner: BufWriter::with_capacity(65536, file),
            written: 0,
            checked: 0,
            start,
        };
        let checksum = {
            let mut checked = Checked {
                inner: &mut writer,
                hash: Sha256::new(),
            };
            checked
                .write_all(prefix.as_bytes())
                .map_err(|error| error.to_string())?;
            serde_json::to_writer(&mut checked, value).map_err(|error| error.to_string())?;
            checked
                .write_all(suffix.as_bytes())
                .map_err(|error| error.to_string())?;
            checked.flush().map_err(|error| error.to_string())?;
            checked
                .hash
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        };
        drop(writer);
        crate::budget(start)?;
        Ok::<String, String>(checksum)
    })();
    match written {
        Ok(checksum) => Ok(PreparedJson {
            temporary: Some(temporary),
            output: path.into(),
            checksum,
            start,
        }),
        Err(error) => {
            let _ = fs::remove_file(temporary);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streaming_digest_preserves_canonical_json_bytes() {
        let value = serde_json::json!({"a": [1, true, "😀"], "b": null});
        assert_eq!(
            digest(&value).unwrap(),
            "6a1e3325fef2140ae6546949935666e3f200b0ff9ae268b73f3a0e837ef4de48"
        );
    }
    #[test]
    fn canonical_fingerprints_match_independent_json_bytes_at_buffer_boundaries() {
        for count in [0, 1, 63, 64, 65, 511, 1023, 1024, 1025, 4096] {
            let value = serde_json::json!({
                "text": "α\\\"\n\0".repeat(count),
                "nested": [[], {"integer": -3, "present": true}, null],
            });
            let bytes = serde_json::to_vec(&value).unwrap();
            let expected: [u8; 32] = Sha256::digest(bytes).into();
            assert_eq!(digest_bytes(&value).unwrap(), expected, "repetitions {count}");
        }
    }

    #[test]
    fn canonical_hashing_rejects_oversized_expansion_without_a_complete_buffer() {
        use serde::ser::SerializeSeq;
        struct Repeated<'a> {
            chunk: &'a str,
            count: usize,
        }
        impl Serialize for Repeated<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut sequence = serializer.serialize_seq(Some(self.count))?;
                for _ in 0..self.count {
                    sequence.serialize_element(self.chunk)?;
                }
                sequence.end()
            }
        }
        let chunk = "x".repeat(4096);
        let count = MAX_CANONICAL_HASH_BYTES as usize / chunk.len() + 1;
        let failure = digest(&Repeated { chunk: &chunk, count }).unwrap_err();
        assert!(failure.contains("canonical data size budget exceeded"));
    }
    #[test]
    fn cache_update_failure_prevents_output_replacement() {
        let root = std::env::temp_dir().join(format!(
            "bifoil-commit-test-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        struct Directory(PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let directory = Directory(root);
        let path = directory.0.join("startup.txt");
        fs::write(&path, "previous valid output").unwrap();
        let error = prepare_json(
            &path,
            &serde_json::json!({"new": true}),
            "prefix",
            "suffix",
            Instant::now(),
        )
        .unwrap()
        .commit(|checksum| {
            assert_eq!(checksum.len(), 64);
            Err("cache update rejected".into())
        })
        .unwrap_err();
        assert!(error.contains("cache update rejected"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "previous valid output");
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }
    #[test]
    fn a_deadline_expiring_after_preparation_preserves_the_old_output_and_removes_its_temporary() {
        struct Deadline(bool);
        impl Drop for Deadline {
            fn drop(&mut self) {
                crate::INCREMENTAL_DEADLINE.set(self.0);
            }
        }
        struct Directory(PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _deadline = Deadline(crate::INCREMENTAL_DEADLINE.get());
        crate::INCREMENTAL_DEADLINE.set(false);
        let root = std::env::temp_dir().join(format!(
            "bifoil-prepared-test-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let directory = Directory(root);
        let path = directory.0.join("startup.txt");
        fs::write(&path, "previous valid output").unwrap();
        let expired = Instant::now().checked_sub(std::time::Duration::from_secs(10)).unwrap();
        let prepared = prepare_json(&path, &serde_json::json!({"new": true}), "prefix", "suffix", expired).unwrap();
        let temporary = prepared.temporary.as_ref().unwrap().clone();
        assert_eq!(fs::read_to_string(&temporary).unwrap(), r#"prefix{"new":true}suffix"#);
        assert_eq!(fs::read_to_string(&path).unwrap(), "previous valid output");
        // Compiler state can be released between these two operations. The
        // final check must catch elapsed time before any cache/root publication.
        crate::INCREMENTAL_DEADLINE.set(true);
        let error = prepared
            .commit(|_| panic!("expired output must not publish cache state"))
            .unwrap_err();
        assert!(error.contains("compilation time budget exhausted"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "previous valid output");
        assert!(!temporary.exists());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }
    #[test]
    fn failed_streamed_output_preserves_the_previous_artifact() {
        struct Directory(PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let root = std::env::temp_dir().join(format!(
            "bifoil-output-test-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let directory = Directory(root);
        let path = directory.0.join("startup.txt");
        fs::write(&path, "previous valid output").unwrap();
        let oversized = "x".repeat(MAX_ARTIFACT as usize + 1);
        let failure = write_json(&path, &oversized, "header\n", "", Instant::now()).unwrap_err();
        assert!(failure.contains("output size budget exceeded"));
        assert_eq!(fs::read_to_string(path).unwrap(), "previous valid output");
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }
    #[test]
    fn matching_output_checksum_covers_prefix_buffer_boundaries_suffix_and_eof() {
        struct Directory(PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let root = std::env::temp_dir().join(format!(
            "bifoil-output-checksum-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let directory = Directory(root);
        let path = directory.0.join("code.txt");
        let value = serde_json::json!({"paragraph": "x".repeat(70_000), "count": 3});
        let prefix = "*bifoil ";
        let suffix = "\n";
        assert!(
            matching_json_checksum(&path, &value, prefix, suffix, Instant::now())
                .unwrap()
                .is_none()
        );
        write_json(&path, &value, prefix, suffix, Instant::now()).unwrap();
        let expected = crate::input::hash(&path, MAX_ARTIFACT, None, Instant::now()).unwrap();
        assert_eq!(
            matching_json_checksum(&path, &value, prefix, suffix, Instant::now()).unwrap(),
            Some(expected)
        );
        let original = fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed[65_537] ^= 1;
        fs::write(&path, changed).unwrap();
        assert!(
            matching_json_checksum(&path, &value, prefix, suffix, Instant::now())
                .unwrap()
                .is_none()
        );
        let mut extra = original;
        extra.push(b' ');
        fs::write(&path, extra).unwrap();
        assert!(
            matching_json_checksum(&path, &value, prefix, suffix, Instant::now())
                .unwrap()
                .is_none()
        );
    }
}
