//! What a render keeps for the next: the raster each layer drew before it was placed, the planes of the masks, and the
//! pictures and maps read from files. One bounded cache lives in a [`Scene`](crate::Scene) and is shared by every scene made
//! from it (`add`, `with_values`), so changing one value redraws only what depends on it.
//!
//! A cached value is found by a [`Key`]: the digest of every input that reaches the value, written by whoever makes it
//! (`Source::keyed`, `Finish::keyed`, `Layer::keyed`). A result is the same with or without the cache, bit for bit, as long as
//! a key holds every input: `tests/it/cache.rs` and `tests/test_cache.py` change each address of the example scenes on a
//! used scene and compare the picture with a cold scene's.

use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use sha2::{Digest, Sha256};

use crate::Error;
use crate::files::Fingerprint;

/// The most bytes of pixels, planes and decoded files the cache of one scene keeps: 2 GiB. Measured on a 15-layer A3 poster at
/// 300 dpi (`docs/design-decisions.md`, "The cache"): what its render can reuse is about 1.2 GiB, and a bound below that keeps
/// nothing useful (a render takes the layers in turn, so the least recently used is always the next one wanted: 5.2 s again),
/// at 1.25 GiB the second render of the same poster takes 0.39 s. An entry larger than the bound is not kept, and that is never
/// an error.
pub(crate) const MAX_BYTES: usize = 1 << 31;

/// What a cached value is found by: a SHA-256 of everything that reaches it.
pub(crate) type Key = Fingerprint;

/// A key being written. Every field is written with its length, so that two different lists of fields never write the same
/// bytes.
pub(crate) struct Keyed(Sha256);

impl Keyed {
    /// A key of the kind `kind` (a layer's raster, a mask's plane...): two kinds never share a key.
    pub(crate) fn new(kind: &str) -> Self {
        let mut key = Self(Sha256::new());
        key.text(kind);
        key
    }

    /// `text`.
    pub(crate) fn text(&mut self, text: &str) -> &mut Self {
        self.number(text.len());
        self.0.update(text.as_bytes());
        self
    }

    /// `count`, a length or a size.
    pub(crate) fn number(&mut self, count: usize) -> &mut Self {
        self.0.update(count.to_le_bytes());
        self
    }

    /// The digest of a picture, a map or a document.
    pub(crate) fn digest(&mut self, digest: &Fingerprint) -> &mut Self {
        self.0.update(digest);
        self
    }

    /// `value` as its `Debug` text, which writes every field of a plain value (a number is the shortest text that reads back as
    /// the same number): a field added later is in the key without anyone remembering to add it.
    pub(crate) fn debug(&mut self, value: &impl fmt::Debug) -> &mut Self {
        self.text(&format!("{value:?}"))
    }

    /// The finished key.
    pub(crate) fn finish(self) -> Key {
        self.0.finalize().into()
    }
}

/// One kept value, how many bytes it holds, and when it was last used.
struct Entry {
    value: Arc<dyn Any + Send + Sync>,
    bytes: usize,
    used: u64,
}

#[derive(Default)]
struct Kept {
    entries: HashMap<Key, Entry>,
    bytes: usize,
    /// A count of uses: the least recently used entry is the one with the least `used`.
    clock: u64,
}

/// A bounded store of what renders drew, by [`Key`], shared by the scenes made from one another and by their threads. When it
/// is full, the least recently used entries go.
pub(crate) struct Cache {
    capacity: usize,
    kept: Mutex<Kept>,
}

impl fmt::Debug for Cache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        write!(
            formatter,
            "Cache {{ entries: {}, bytes: {} }}",
            kept.entries.len(),
            kept.bytes
        )
    }
}

impl Cache {
    /// An empty cache of [`MAX_BYTES`].
    pub(crate) fn new() -> Self {
        Self::with_capacity(MAX_BYTES)
    }

    /// An empty cache that keeps at most `capacity` bytes.
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            kept: Mutex::new(Kept::default()),
        }
    }

    /// The value kept under `key`, or the one `make` makes (and how many bytes it holds), kept for the next call if it fits.
    /// Two threads that miss together both make it: the two are equal, and the later is the one kept. `make` runs with the
    /// cache unlocked, so it may use the cache itself.
    ///
    /// # Errors
    /// What `make` refuses: nothing is kept then.
    pub(crate) fn fetch<T: Send + Sync + 'static>(
        &self,
        key: &Key,
        make: impl FnOnce() -> Result<(T, usize), Error>,
    ) -> Result<Arc<T>, Error> {
        if let Some(found) = self.found(key) {
            return Ok(found);
        }
        let (value, bytes) = make()?;
        let value = Arc::new(value);
        let shared: Arc<dyn Any + Send + Sync> = Arc::clone(&value) as Arc<dyn Any + Send + Sync>;
        self.keep(*key, shared, bytes);
        Ok(value)
    }

    /// The value kept under `key`, made the most recently used.
    fn found<T: Send + Sync + 'static>(&self, key: &Key) -> Option<Arc<T>> {
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.clock += 1;
        let clock = kept.clock;
        let entry = kept.entries.get_mut(key)?;
        entry.used = clock;
        Arc::clone(&entry.value).downcast().ok()
    }

    /// Keeps `value` of `bytes` bytes under `key`, after making room: the least recently used entries go first. A value larger
    /// than the whole cache is not kept.
    fn keep(&self, key: Key, value: Arc<dyn Any + Send + Sync>, bytes: usize) {
        if bytes > self.capacity {
            return;
        }
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(replaced) = kept.entries.remove(&key) {
            kept.bytes -= replaced.bytes;
        }
        while kept.bytes + bytes > self.capacity {
            let Some(oldest) = kept
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| *key)
            else {
                break;
            };
            if let Some(gone) = kept.entries.remove(&oldest) {
                kept.bytes -= gone.bytes;
            }
        }
        kept.clock += 1;
        let used = kept.clock;
        kept.bytes += bytes;
        kept.entries.insert(key, Entry { value, bytes, used });
    }
}

// Inline, because the bound is 1 GiB and no public route fills it cheaply: the order things leave in and the value too large
// to keep.
#[cfg(test)]
mod tests {
    use super::{Cache, Key, Keyed};
    use crate::Error;

    fn key(name: &str) -> Key {
        Keyed::new("test").text(name).finish_by_ref()
    }

    impl Keyed {
        fn finish_by_ref(&mut self) -> Key {
            std::mem::replace(self, Self::new("spent")).finish()
        }
    }

    fn made(cache: &Cache, name: &str, bytes: usize, makes: &mut usize) -> Result<usize, Error> {
        cache
            .fetch(&key(name), || {
                *makes += 1;
                Ok((bytes, bytes))
            })
            .map(|found| *found)
    }

    #[test]
    fn a_value_is_made_once_while_it_is_kept_and_the_least_recently_used_goes_first() -> Result<(), Error> {
        let cache = Cache::with_capacity(100);
        let mut makes = 0;
        for name in ["a", "b", "a"] {
            made(&cache, name, 40, &mut makes)?;
        }
        assert_eq!(makes, 2, "`a` is kept");
        made(&cache, "c", 40, &mut makes)?; // 120 > 100: `b`, not `a`, goes
        assert_eq!(makes, 3);
        made(&cache, "a", 40, &mut makes)?;
        assert_eq!(makes, 3, "`a` was used after `b`, so it stayed");
        made(&cache, "b", 40, &mut makes)?;
        assert_eq!(makes, 4, "`b` went");
        Ok(())
    }

    #[test]
    fn a_value_larger_than_the_whole_cache_is_not_kept_and_is_no_error() -> Result<(), Error> {
        let cache = Cache::with_capacity(100);
        let mut makes = 0;
        assert_eq!(made(&cache, "big", 101, &mut makes)?, 101);
        assert_eq!(made(&cache, "big", 101, &mut makes)?, 101);
        assert_eq!(makes, 2);
        made(&cache, "small", 100, &mut makes)?;
        made(&cache, "small", 100, &mut makes)?;
        assert_eq!(makes, 3, "a value exactly as large as the cache is kept");
        Ok(())
    }

    #[test]
    fn what_make_refuses_is_not_kept() {
        let cache = Cache::with_capacity(100);
        let refuse = || -> Result<(u8, usize), Error> { Err(Error::invalid("x", "no")) };
        assert!(cache.fetch(&key("a"), refuse).is_err());
        assert_eq!(cache.fetch(&key("a"), || Ok((7_u8, 1))).map(|found| *found), Ok(7));
    }

    #[test]
    fn two_lists_of_fields_never_write_one_key() {
        let split = Keyed::new("k").text("ab").text("c").finish_by_ref();
        let joined = Keyed::new("k").text("a").text("bc").finish_by_ref();
        assert_ne!(split, joined);
        assert_ne!(Keyed::new("one").finish_by_ref(), Keyed::new("two").finish_by_ref());
    }
}
