//! Batch small serializer writes without retaining a complete artifact.

use sha2::{Digest, Sha256};

const BUFFER_BYTES: usize = 1024;

pub(super) struct BufferedHash {
    inner: Sha256,
    pending: [u8; BUFFER_BYTES],
    used: usize,
}

impl BufferedHash {
    pub(super) fn new() -> Self {
        Self {
            inner: Sha256::new(),
            pending: [0; BUFFER_BYTES],
            used: 0,
        }
    }

    pub(super) fn update(&mut self, bytes: &[u8]) {
        // Raw code/string fragments are already large enough to hash directly.
        if bytes.len() >= BUFFER_BYTES {
            self.flush();
            self.inner.update(bytes);
            return;
        }
        let first = bytes.len().min(BUFFER_BYTES - self.used);
        self.pending[self.used..self.used + first].copy_from_slice(&bytes[..first]);
        self.used += first;
        if self.used == BUFFER_BYTES {
            self.flush();
        }
        let remaining = &bytes[first..];
        if !remaining.is_empty() {
            self.pending[..remaining.len()].copy_from_slice(remaining);
            self.used = remaining.len();
        }
    }

    pub(super) fn flush(&mut self) {
        if self.used != 0 {
            self.inner.update(&self.pending[..self.used]);
            self.used = 0;
        }
    }

    pub(super) fn finalize(mut self) -> [u8; 32] {
        self.flush();
        self.inner.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partitioning_empty_writes_and_flushes_preserve_the_digest() {
        let bytes: Vec<u8> = (0..BUFFER_BYTES * 7 + 31).map(|index| (index % 251) as u8).collect();
        let expected: [u8; 32] = Sha256::digest(&bytes).into();
        for width in [1, 31, 63, 64, 65, 511, 1023, 1024, 1025, 2048, 4096] {
            for flushing in [false, true] {
                let mut hash = BufferedHash::new();
                hash.update(&[]);
                for (index, part) in bytes.chunks(width).enumerate() {
                    hash.update(part);
                    hash.update(&[]);
                    if flushing && index % 3 == 0 {
                        hash.flush();
                    }
                }
                assert_eq!(hash.finalize(), expected, "width {width}, flush {flushing}");
            }
        }
        assert_eq!(BufferedHash::new().finalize(), <[u8; 32]>::from(Sha256::digest([])));
    }
}
