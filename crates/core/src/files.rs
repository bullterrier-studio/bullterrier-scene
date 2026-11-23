//! A file a scene names: a picture, a map or a font. Where it is (a folder on disk, or bytes the caller gave), and the digest
//! it was saved with. One value, so that reading it, saving it and comparing it follow one rule.

use std::collections::BTreeMap;
use std::fmt::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

/// The floats of one piece of a large buffer: 2^20 of them (4 MiB), the unit of [`digest_floats`].
const PIECE: usize = 1 << 20;

/// The SHA-256 of the 32-bit floats `values`, as their little-endian bytes.
fn digest_piece(values: &[f32]) -> Fingerprint {
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

/// The identity of a picture or a map held in memory (it has no file): the SHA-256 of the digests of its pieces of 2^20 floats
/// in order, so that the pieces of a large one are hashed on every core and the digest is the same whatever the machine. A
/// picture of 12 megapixels is 192 MB: hashed once, when first asked for.
pub(crate) fn digest_floats(values: &[f32]) -> Fingerprint {
    let pieces: Vec<&[f32]> = values.chunks(PIECE).collect();
    let mut digests = vec![[0_u8; 32]; pieces.len()];
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let share = pieces.len().div_ceil(cores.min(pieces.len()).max(1)).max(1);
    std::thread::scope(|scope| {
        for (out, part) in digests.chunks_mut(share).zip(pieces.chunks(share)) {
            scope.spawn(move || {
                for (digest, piece) in out.iter_mut().zip(part) {
                    *digest = digest_piece(piece);
                }
            });
        }
    });
    let mut hash = Sha256::new();
    hash.update(values.len().to_le_bytes());
    for digest in &digests {
        hash.update(digest);
    }
    hash.finalize().into()
}

/// `digest` as 64 lower-case hexadecimal digits.
pub(crate) fn hex_of(digest: &Fingerprint) -> String {
    hex(digest)
}

/// Where the files a saved scene names are read from ([`Scene::from_json_with`](crate::Scene::from_json_with)): a folder on
/// disk, files given as bytes, or both (a given file is found first). A browser has no disk: it gives every file.
///
/// ```ignore
/// let files = Files::given().with("photo.png", png_bytes).with("look.toml", toml_bytes);
/// let scene = Scene::from_json_with(&text, &files, &LayerKinds::new())?;
/// ```
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct Files {
    /// The folder a relative path is read from; `None` reads nothing from disk.
    folder: Option<PathBuf>,
    /// The files given, by the path as the document writes it.
    given: BTreeMap<String, Given>,
}

/// The bytes of a given file and their SHA-256 (hexadecimal), made once when the file is given.
#[derive(Clone)]
struct Given {
    bytes: Arc<[u8]>,
    sha256: String,
}

impl fmt::Debug for Given {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} bytes, sha256 {}", self.bytes.len(), self.sha256)
    }
}

impl Files {
    /// The files on disk: a relative path is relative to the folder `base` (an empty path is the working folder).
    pub fn folder(base: impl Into<PathBuf>) -> Self {
        Self {
            folder: Some(base.into()),
            given: BTreeMap::new(),
        }
    }

    /// No files yet: nothing on disk is read, each file comes from [`with`](Files::with).
    pub fn given() -> Self {
        Self::default()
    }

    /// These files and `bytes` as the file the document names `path`, matched exactly as written (`./dog.png` is not
    /// `dog.png`).
    pub fn with(mut self, path: impl Into<String>, bytes: impl Into<Arc<[u8]>>) -> Self {
        let bytes = bytes.into();
        let sha256 = sha256(&bytes);
        self.given.insert(path.into(), Given { bytes, sha256 });
        self
    }

    /// The file the document names `path`, saved with the digest `sha256` (if known).
    ///
    /// # Errors
    /// [`Error::Read`] naming `path` as written if it is not given and there is no folder.
    pub(crate) fn locate(&self, path: &str, sha256: Option<String>) -> Result<Located, Error> {
        let at = match (self.given.get(path), &self.folder) {
            (Some(given), _) => At::Given(given.clone()),
            (None, Some(folder)) => At::Disk(folder.clone()),
            (None, None) => {
                return Err(Error::Read {
                    path: path.to_owned(),
                    message: "no file of this name was given".to_owned(),
                });
            }
        };
        Ok(Located {
            path: PathBuf::from(path),
            at,
            sha256,
        })
    }
}

/// Where a file's bytes are.
#[derive(Debug, Clone)]
enum At {
    /// On disk, relative to this folder (empty for the working folder) if the path is relative.
    Disk(PathBuf),
    /// Given by the caller.
    Given(Given),
}

/// A file as the scene names it.
#[derive(Debug, Clone)]
pub(crate) struct Located {
    /// The path as written: relative to the folder, or absolute; for a given file, its name.
    path: PathBuf,
    /// Where its bytes are.
    at: At,
    /// The digest the file had when the scene was saved; a scene made by calls has none.
    sha256: Option<String>,
}

impl Located {
    /// The file `path` on disk, relative to the working folder if it is relative.
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            at: At::Disk(PathBuf::new()),
            sha256: None,
        }
    }

    /// The size and the modification time of the file on disk as it is now: what tells a changed file from the one read
    /// before. `None` for a file that cannot be asked (missing, or on a system that keeps no time).
    fn stamp(path: &Path) -> Option<(u64, u128)> {
        let found = std::fs::metadata(path).ok()?;
        let since = found.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some((found.len(), since.as_nanos()))
    }

    /// Writes into `key` which file this is and what state it is in, and the digest the scene was saved with. On disk: where
    /// it is, its size and its modification time (a change that keeps the size and the time is not seen); gives `false`, and
    /// writes nothing a read could be taken for, when the file cannot be asked: the caller reads it and gets the error. A
    /// given file: the SHA-256 of its bytes, and `true`.
    pub(crate) fn keyed(&self, key: &mut Keyed) -> bool {
        match &self.at {
            At::Disk(_) => {
                let path = self.resolved();
                let Some((size, time)) = Self::stamp(&path) else {
                    key.text("unreadable");
                    return false;
                };
                key.text(&path.to_string_lossy())
                    .number(usize::try_from(size).unwrap_or(usize::MAX))
                    .text(&time.to_string());
            }
            At::Given(given) => {
                key.text("given").text(&given.sha256);
            }
        }
        key.text(self.sha256.as_deref().unwrap_or(""));
        true
    }

    /// The path as written.
    pub(crate) fn written(&self) -> &Path {
        &self.path
    }

    /// Where the file is: the path under its folder on disk; a given file's name.
    pub(crate) fn resolved(&self) -> PathBuf {
        match &self.at {
            At::Disk(base) => base.join(&self.path),
            At::Given(_) => self.path.clone(),
        }
    }

    /// The SHA-256 of a given file's bytes; `None` for a file on disk.
    pub(crate) fn given_digest(&self) -> Option<&str> {
        match &self.at {
            At::Disk(_) => None,
            At::Given(given) => Some(&given.sha256),
        }
    }

    /// The bytes of the file and their digest, as they are now.
    fn read(&self) -> Result<(Arc<[u8]>, String), Error> {
        match &self.at {
            At::Disk(_) => {
                let path = self.resolved();
                let bytes = std::fs::read(&path).map_err(|error| Error::Read {
                    path: path.display().to_string(),
                    message: format!("cannot read {}: {error}", self.path.display()),
                })?;
                let digest = sha256(&bytes);
                Ok((bytes.into(), digest))
            }
            At::Given(given) => Ok((Arc::clone(&given.bytes), given.sha256.clone())),
        }
    }

    /// The bytes of the file, if they are those of the digest it was saved with.
    ///
    /// # Errors
    /// [`Error::Read`] if the file cannot be read, or its digest is not the one the scene was saved with (the message names
    /// both).
    pub(crate) fn bytes(&self) -> Result<Arc<[u8]>, Error> {
        let (bytes, digest) = self.read()?;
        match &self.sha256 {
            Some(saved) if *saved != digest => Err(Error::Read {
                path: self.resolved().display().to_string(),
                message: format!(
                    "the file's sha256 is {digest}, but the scene was saved with {saved}: the file changed"
                ),
            }),
            _ => Ok(bytes),
        }
    }

    /// The digest of the file as it is now.
    ///
    /// # Errors
    /// [`Error::Read`] if the file cannot be read.
    pub(crate) fn digest(&self) -> Result<String, Error> {
        self.read().map(|(_, digest)| digest)
    }
}

#[cfg(test)]
mod tests {
    use super::{Files, Located};
    use crate::Error;
    use crate::cache::{Key, Keyed};

    /// The key `keyed` writes for `file`, and whether it says the file can be read.
    fn key_of(file: &Located) -> (bool, Key) {
        let mut key = Keyed::new("picture");
        let readable = file.keyed(&mut key);
        (readable, key.finish())
    }

    /// No public route shows a cache hit: a given file is keyed by its bytes' digest, so the same bytes are one key wherever
    /// they are given and other bytes another.
    #[test]
    fn a_given_file_is_keyed_by_the_digest_of_its_bytes() -> Result<(), Error> {
        let dog = Files::given().with("dog.png", b"dog".as_slice());
        let (readable, key) = key_of(&dog.locate("dog.png", None)?);
        assert!(readable);
        let again = Files::given().with("dog.png", b"dog".as_slice());
        assert_eq!(key_of(&again.locate("dog.png", None)?), (true, key));
        let cat = Files::given().with("dog.png", b"cat".as_slice());
        assert_ne!(key_of(&cat.locate("dog.png", None)?).1, key);
        // the digest the scene was saved with is in the key too: a file read under one is never taken for another
        assert_ne!(key_of(&dog.locate("dog.png", Some("0".repeat(64)))?).1, key);
        Ok(())
    }
}
