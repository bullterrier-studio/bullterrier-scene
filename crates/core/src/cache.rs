//! What a render keeps for the next: the raster each layer drew before it was placed, the planes of the masks, and the
//! pictures and maps read from files. One bounded cache lives in a [`Scene`](crate::Scene) and is shared by every scene made
//! from it (`add`, `with_values`), so changing one value redraws only what depends on it.
//!
//! A cached value is found by a [`Key`]: the digest of every input that reaches the value, written by whoever makes it
//! (`Source::keyed`, `Finish::keyed`, `Layer::keyed`). A result is the same with or without the cache, bit for bit, as long as
//! a key holds every input: `tests/it/cache.rs` and `tests/test_cache.py` change each address of the example scenes on a
//! used scene and compare the picture with a cold scene's.

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use sha2::{Digest, Sha256};

use crate::Error;
use crate::files::Fingerprint;

/// The most bytes of pixels, planes and decoded files the cache of one scene keeps: 2 GiB. Measured on a 15-layer A3 poster at
/// 300 dpi (`docs/design-decisions.md`, "The cache"): what its render can reuse is about 1.2 GiB, and a bound below that keeps
/// nothing useful (a render takes the layers in turn, so the least recently used is always the next one wanted;
/// a banded render spares what it has used, see `begin_render`), at 1.25 GiB the second render of the same poster
/// is served from the cache. An entry larger than the bound is not kept, and that is never an error.
pub(crate) const MAX_BYTES: usize = 1 << 31;

/// What a cached value is found by: a SHA-256 of everything that reaches it.
pub(crate) type Key = Fingerprint;

/// A key being written. Every field is written with its length, so that two different lists of fields never write the same
/// bytes.
#[derive(Clone)]
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
    /// Whether this entry is the first to go: a stage of a look chain, which a finished raster can be made from again.
    expendable: bool,
    /// The render that last used this entry (`Cache::begin_render`).
    render: u64,
}

#[derive(Default)]
struct Kept {
    entries: HashMap<Key, Entry>,
    bytes: usize,
    /// A count of uses: the least recently used entry is the one with the least `used`.
    clock: u64,
    /// How many renders have begun: what a render that scans has used is not pushed out by what it makes after (`keep`).
    render: u64,
    /// Whether the render in progress scans its layers once more for each band (`begin_render`).
    scan: bool,
    /// The finished values the render in progress has pushed out for room, and those the render before it pushed out
    /// (`begin_render`); a stage or the plane of a band is not one, so the set never outgrows the layers.
    evicted: HashSet<Key>,
    ghost: HashSet<Key>,
    /// Whether the render in progress is making a value the render before it pushed out: the scene holds more than the cache
    /// does and is read in turn (`keep`).
    repeats_a_scan: bool,
}

/// A bounded store of what renders drew, by [`Key`], shared by the scenes made from one another and by their threads. When it
/// is full, the least recently used entries go, those a render has not used first of all (see `keep`).
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
        if let Some(found) = self.get(key) {
            return Ok(found);
        }
        let (value, bytes) = make()?;
        Ok(self.put(*key, value, bytes, false))
    }

    /// [`fetch`](Self::fetch) for a value only the render that makes it will use again (the plane of a band, which no other band
    /// reads): kept only where it fits without costing the cache a finished value, and the first to leave (`put`'s `expendable`).
    ///
    /// # Errors
    /// What `make` refuses: nothing is kept then.
    pub(crate) fn fetch_expendable<T: Send + Sync + 'static>(
        &self,
        key: &Key,
        make: impl FnOnce() -> Result<(T, usize), Error>,
    ) -> Result<Arc<T>, Error> {
        if let Some(found) = self.get(key) {
            return Ok(found);
        }
        let (value, bytes) = make()?;
        Ok(self.put(*key, value, bytes, true))
    }

    /// A render begins. If it `scan`s (it reads its layers once more for each band), what it uses from here on is kept over
    /// what it makes after, where the cache is full (`put`): a scan of more than the cache holds would otherwise find the layer
    /// it wants next just gone. A render of one pass does not: a value it makes may be read again at once (a reference, a
    /// layer that is also a mask, a group used twice), and the least recently used leaves for it. A render and a band of one are
    /// a render; a scene drawn as a group inside it is not another. Renders at once on one cache share the count, which only
    /// makes each spare what the others use too: the picture never depends on it.
    ///
    /// A scene of more layers than the cache holds, drawn again, finds gone the layer the render before pushed out for the last
    /// one it made, and so on round: least recently used first would push out the layer wanted next each time. The values a
    /// render pushes out are kept by key (not the values) for the next, and a render that makes one of them is a repeated scan:
    /// from there it pushes out first what the render before used and it has not yet, the most recently used first (the one
    /// wanted last), and only then the least recently used of what it used. All but two of the layers are found each time.
    pub(crate) fn begin_render(&self, scan: bool) {
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.render += 1;
        kept.scan = scan;
        kept.ghost = std::mem::take(&mut kept.evicted);
        kept.repeats_a_scan = false;
    }

    /// The most bytes this cache keeps.
    #[cfg(feature = "gpu")]
    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }

    /// The value kept under `key`, made the most recently used.
    pub(crate) fn get<T: Send + Sync + 'static>(&self, key: &Key) -> Option<Arc<T>> {
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.clock += 1;
        let (clock, render) = (kept.clock, kept.render);
        let entry = kept.entries.get_mut(key)?;
        entry.used = clock;
        entry.render = render;
        Arc::clone(&entry.value).downcast().ok()
    }

    /// Keeps `value` of `bytes` bytes under `key` if it fits (see `keep`), and hands it back. An `expendable` value (a stage a
    /// finished value can be made from again) is kept only where it fits by making other expendable values leave, and is the
    /// first to leave when a value that is not expendable needs room: it never costs the cache a finished value. A finished
    /// value never pushes out one the render has used, which a later band or layer of it reads again: it is not kept then.
    pub(crate) fn put<T: Send + Sync + 'static>(&self, key: Key, value: T, bytes: usize, expendable: bool) -> Arc<T> {
        let value = Arc::new(value);
        let shared: Arc<dyn Any + Send + Sync> = Arc::clone(&value) as Arc<dyn Any + Send + Sync>;
        self.keep(key, shared, bytes, expendable);
        value
    }

    /// Keeps `value` of `bytes` bytes under `key`, after making room: expendable entries go first, then the least recently
    /// used of those this render has not used (the most recently used first, in a repeated scan: `begin_render`). A value larger than the whole cache, or that would need a value this render has
    /// used to go, is not kept: a render that holds more than the cache keeps scans it in turn, and the least recently used
    /// would be the next one wanted each time (nothing kept is read again, where sparing the used ones keeps all but the last).
    fn keep(&self, key: Key, value: Arc<dyn Any + Send + Sync>, bytes: usize, expendable: bool) {
        if bytes > self.capacity {
            return;
        }
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        let (render, scan) = (kept.render, kept.scan);
        if let Some(replaced) = kept.entries.remove(&key) {
            kept.bytes -= replaced.bytes;
        }
        // made again what the render before pushed out: the scene is a scan the cache cannot hold, and from here on what the
        // render before used and this one has not yet is what goes (the one it wants last), not the one it wants next
        kept.repeats_a_scan |= kept.ghost.contains(&key);
        let repeats = kept.repeats_a_scan;
        // what may go for this value: a stage, and (not for a stage) all but what a scanning render has used
        let spared = |entry: &Entry| scan && entry.render == render;
        let may_go = |entry: &Entry| entry.expendable || (!expendable && !spared(entry));
        if expendable {
            let others: usize = kept
                .entries
                .values()
                .filter(|entry| !entry.expendable)
                .map(|entry| entry.bytes)
                .sum();
            if others + bytes > self.capacity {
                return;
            }
        }
        let room: usize = kept
            .entries
            .values()
            .filter(|entry| may_go(entry))
            .map(|entry| entry.bytes)
            .sum();
        if kept.bytes - room + bytes > self.capacity {
            return;
        }
        while kept.bytes + bytes > self.capacity {
            let Some(oldest) = kept
                .entries
                .iter()
                .filter(|(_, entry)| may_go(entry))
                .min_by_key(|(_, entry)| {
                    let unused = entry.render != render;
                    // least recently used first; in a repeated scan, what this render has not used goes first, newest first
                    let rank = if repeats && unused {
                        u64::MAX - entry.used
                    } else {
                        entry.used
                    };
                    (!entry.expendable, repeats && !unused, rank)
                })
                .map(|(key, _)| *key)
            else {
                break;
            };
            if let Some(gone) = kept.entries.remove(&oldest) {
                kept.bytes -= gone.bytes;
                if !gone.expendable {
                    kept.evicted.insert(oldest);
                }
            }
        }
        kept.clock += 1;
        let used = kept.clock;
        kept.bytes += bytes;
        kept.entries.insert(
            key,
            Entry {
                value,
                bytes,
                used,
                expendable,
                render,
            },
        );
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
        cache.begin_render(false); // `a` and `b` were used by an earlier render
        made(&cache, "c", 40, &mut makes)?; // 120 > 100: `b`, not `a`, goes
        assert_eq!(makes, 3);
        made(&cache, "a", 40, &mut makes)?;
        assert_eq!(makes, 3, "`a` was used after `b`, so it stayed");
        made(&cache, "b", 40, &mut makes)?;
        assert_eq!(makes, 4, "`b` went");
        Ok(())
    }

    #[test]
    fn a_render_that_holds_more_than_the_cache_keeps_what_it_has_used_and_not_what_it_makes_after() -> Result<(), Error>
    {
        let cache = Cache::with_capacity(100);
        let mut makes = 0;
        cache.begin_render(true);
        // a scan of four values of 40 with room for two, each read again by the next band
        for _ in 0..3 {
            for name in ["a", "b", "c", "d"] {
                made(&cache, name, 40, &mut makes)?;
            }
        }
        assert_eq!(
            makes,
            4 + 2 * 2,
            "`a` and `b` are made once, `c` and `d` for each of the three bands"
        );
        // the next render finds `a` and `b`: they were not used by it yet, so a value it makes may take their room, the oldest first
        cache.begin_render(true);
        made(&cache, "a", 40, &mut makes)?;
        made(&cache, "c", 40, &mut makes)?; // `b` is the oldest of the two not used by this render
        assert_eq!(makes, 9, "`a` was found, `c` was made");
        made(&cache, "b", 40, &mut makes)?;
        assert_eq!(makes, 10, "`b` went for `c`");
        Ok(())
    }

    #[test]
    fn a_scan_the_cache_cannot_hold_keeps_all_but_two_of_its_values_for_the_render_after() -> Result<(), Error> {
        let cache = Cache::with_capacity(120);
        let mut makes = 0;
        let mut per_render = Vec::new();
        for _ in 0..3 {
            cache.begin_render(false);
            let before = makes;
            for name in ["a", "b", "c", "d"] {
                made(&cache, name, 40, &mut makes)?;
            }
            per_render.push(makes - before);
        }
        // the first render pushes `a` out for `d`; the second finds `a` gone, and from then on what the render before used
        // and it has not yet is what goes, newest first (`d`), so `b` and `c` are found; `d` then pushes out `a`, the least
        // recently used of those this render used
        assert_eq!(per_render, [4, 2, 2]);
        Ok(())
    }

    #[test]
    fn a_value_new_to_a_repeated_scan_pushes_out_what_the_render_has_not_used_and_not_what_it_has() -> Result<(), Error>
    {
        let cache = Cache::with_capacity(120);
        let mut makes = 0;
        cache.begin_render(false);
        for name in ["a", "b", "c", "d"] {
            made(&cache, name, 40, &mut makes)?; // `a` goes for `d`
        }
        cache.begin_render(false);
        // `a` is found gone, so this render repeats the scan: `d` leaves for it; `x` is new, and `c`, not yet used by this render, leaves for it
        for name in ["a", "x", "a"] {
            made(&cache, name, 40, &mut makes)?;
        }
        assert_eq!(makes, 4 + 2, "`a` and `x` are made, and `a` is found again");
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
    fn an_expendable_value_never_costs_the_cache_a_finished_one_and_is_the_first_to_go() {
        let cache = Cache::with_capacity(100);
        cache.put(key("stage1"), 1_u8, 40, true);
        cache.put(key("stage2"), 2_u8, 40, true);
        cache.put(key("done1"), 3_u8, 40, false); // 120 > 100: the oldest stage goes, not the finished value
        assert!(cache.get::<u8>(&key("stage1")).is_none());
        assert!(cache.get::<u8>(&key("stage2")).is_some());
        cache.put(key("done2"), 4_u8, 40, false); // 120 > 100 again: the other stage goes, though `done1` is older
        assert!(cache.get::<u8>(&key("stage2")).is_none());
        assert!(cache.get::<u8>(&key("done1")).is_some());
        // no room without a finished value leaving: the stage is not kept, and nothing leaves for it
        cache.put(key("stage3"), 5_u8, 40, true);
        assert!(cache.get::<u8>(&key("stage3")).is_none());
        assert!(cache.get::<u8>(&key("done1")).is_some() && cache.get::<u8>(&key("done2")).is_some());
        // a stage is kept where it fits beside the finished values
        cache.put(key("stage4"), 6_u8, 20, true);
        assert!(cache.get::<u8>(&key("stage4")).is_some());
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
