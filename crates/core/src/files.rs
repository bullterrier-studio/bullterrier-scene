//! A file a scene names: a picture, a map or a font. Where it is, the folder it is relative to, and the digest it was saved
//! with. One value, so that reading it, saving it and comparing it follow one rule.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::Error;
use crate::cache::Keyed;

/// `bytes` as lower-case hexadecimal digits.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut digits, byte| {
        // writing to a string cannot fail
        let _ = write!(digits, "{byte:02x}");
        digits
    })
}

/// The SHA-256 of `bytes`, as 64 lower-case hexadecimal digits.
pub(crate) fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// The SHA-256 of `bytes`, as 32 bytes.
pub(crate) fn digest_bytes(bytes: &[u8]) -> Fingerprint {
    Sha256::digest(bytes).into()
}

/// A SHA-256 digest.
pub(crate) type Fingerprint = [u8; 32];

/// The SHA-256 of the 32-bit floats `values`, as their little-endian bytes (a picture held in memory has no file: this is its
/// identity when two scenes are compared, and when the cache keys what it drew).
pub(crate) fn digest_floats(values: &[f32]) -> Fingerprint {
    let mut hash = Sha256::new();
    let mut bytes = [[0_u8; 4]; 2048];
    for chunk in values.chunks(2048) {
        for (slot, value) in bytes.iter_mut().zip(chunk) {
            *slot = value.to_le_bytes();
        }
        hash.update(bytes[..chunk.len()].as_flattened());
    }
    hash.finalize().into()
}

/// `digest` as 64 lower-case hexadecimal digits.
pub(crate) fn hex_of(digest: &Fingerprint) -> String {
    hex(digest)
}

/// A file as the scene names it.
#[derive(Debug, Clone)]
pub(crate) struct Located {
    /// The path as written: relative to `base`, or absolute.
    path: PathBuf,
    /// The folder a relative `path` is relative to: empty for the working folder.
    base: PathBuf,
    /// The digest the file had when the scene was saved; a scene made by calls has none.
    sha256: Option<String>,
}

impl Located {
    /// The file `path`, relative to the working folder if it is relative.
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            base: PathBuf::new(),
            sha256: None,
        }
    }

    /// The file `path` of a saved scene, relative to the folder `base`, as it was when it had the digest `sha256` (if known).
    pub(crate) fn saved(path: impl Into<PathBuf>, base: &Path, sha256: Option<String>) -> Self {
        Self {
            path: path.into(),
            base: base.to_owned(),
            sha256,
        }
    }

    /// The size and the modification time of the file as it is now: what tells a changed file from the one read before. `None`
    /// for a file that cannot be asked (missing, or on a system that keeps no time).
    fn stamp(&self) -> Option<(u64, u128)> {
        let found = std::fs::metadata(self.resolved()).ok()?;
        let since = found.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some((found.len(), since.as_nanos()))
    }

    /// Writes into `key` which file this is and what state it is in (where it is, its size and its modification time, and the
    /// digest the scene was saved with): a change that keeps the size and the time is not seen. Gives `false`, and writes
    /// nothing a read could be taken for, when the file cannot be asked: the caller reads it and gets the error.
    pub(crate) fn keyed(&self, key: &mut Keyed) -> bool {
        let Some((size, time)) = self.stamp() else {
            key.text("unreadable");
            return false;
        };
        key.text(&self.resolved().to_string_lossy())
            .number(usize::try_from(size).unwrap_or(usize::MAX))
            .text(&time.to_string())
            .text(self.sha256.as_deref().unwrap_or(""));
        true
    }

    /// The path as written.
    pub(crate) fn written(&self) -> &Path {
        &self.path
    }

    /// Where the file is: `path` under `base`.
    pub(crate) fn resolved(&self) -> PathBuf {
        self.base.join(&self.path)
    }

    /// The bytes of the file, if they are those of the digest it was saved with.
    ///
    /// # Errors
    /// [`Error::Read`] if the file cannot be read, or its digest is not the one the scene was saved with (the message names
    /// both).
    pub(crate) fn bytes(&self) -> Result<Vec<u8>, Error> {
        let path = self.resolved();
        let read = |message: String| Error::Read {
            path: path.display().to_string(),
            message,
        };
        let bytes = std::fs::read(&path).map_err(|error| read(error.to_string()))?;
        match &self.sha256 {
            Some(saved) if *saved != sha256(&bytes) => Err(read(format!(
                "the file's sha256 is {}, but the scene was saved with {saved}: the file changed",
                sha256(&bytes)
            ))),
            _ => Ok(bytes),
        }
    }

    /// Whether the file is still the one the scene was saved with: nothing is read for a scene that was not.
    ///
    /// # Errors
    /// As [`Located::bytes`].
    pub(crate) fn verified(&self) -> Result<(), Error> {
        if self.sha256.is_some() {
            self.bytes().map(|_| ())
        } else {
            Ok(())
        }
    }

    /// The digest of the file as it is now.
    ///
    /// # Errors
    /// [`Error::Read`] if the file cannot be read.
    pub(crate) fn digest(&self) -> Result<String, Error> {
        let path = self.resolved();
        std::fs::read(&path)
            .map(|bytes| sha256(&bytes))
            .map_err(|error| Error::Read {
                path: path.display().to_string(),
                message: error.to_string(),
            })
    }
}
