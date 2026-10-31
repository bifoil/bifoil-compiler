//! Logical output positions without seeking or flushing a buffered writer.

use sha2::{Digest, Sha256};
use std::io::{self, Write};

pub(super) struct CountingWriter<W> {
    inner: W,
    position: u64,
    hash: Sha256,
}

impl<W: Write> CountingWriter<W> {
    pub(super) fn new(inner: W, position: u64) -> Self {
        Self {
            inner,
            position,
            hash: Sha256::new(),
        }
    }

    /// Finish one record without flushing the shared output buffer.
    pub(super) fn take_checksum(&mut self) -> [u8; 32] {
        self.hash.finalize_reset().into()
    }

    pub(super) fn position(&self) -> u64 {
        self.position
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.inner.write(bytes)?;
        self.position += count as u64;
        self.hash.update(&bytes[..count]);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufWriter;

    #[test]
    fn logical_positions_preserve_buffering_across_chunk_boundaries() {
        let mut stored = Vec::new();
        {
            let mut writer = CountingWriter::new(BufWriter::with_capacity(16, &mut stored), 7);
            writer.write_all(b"first").unwrap();
            assert_eq!(writer.position(), 12);
            assert!(writer.inner.get_ref().is_empty());
            writer.write_all(b"-second-third-fourth").unwrap();
            assert_eq!(writer.position(), 32);
            assert_eq!(
                writer.take_checksum(),
                <[u8; 32]>::from(Sha256::digest(b"first-second-third-fourth"))
            );
            writer.write_all(b"next").unwrap();
            assert_eq!(writer.take_checksum(), <[u8; 32]>::from(Sha256::digest(b"next")));
            writer.flush().unwrap();
        }
        assert_eq!(stored, b"first-second-third-fourthnext");
    }

    #[test]
    fn positions_count_only_bytes_accepted_before_a_write_error() {
        struct FailingWriter {
            remaining: usize,
        }
        impl Write for FailingWriter {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.remaining == 0 {
                    return Err(io::Error::other("storage exhausted"));
                }
                let count = bytes.len().min(self.remaining).min(2);
                self.remaining -= count;
                Ok(count)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut writer = CountingWriter::new(FailingWriter { remaining: 5 }, 11);
        let error = writer.write_all(b"incomplete").unwrap_err();
        assert_eq!(error.to_string(), "storage exhausted");
        assert_eq!(writer.position(), 16);
        assert_eq!(writer.take_checksum(), <[u8; 32]>::from(Sha256::digest(b"incom")));
    }
}
