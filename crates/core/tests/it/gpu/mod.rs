//! The GPU compositor (feature `gpu`), at small sizes. `BT_GPU` says what GPU the machine has: unset, the tests need one and
//! fail without it; `none`, each GPU test returns early and passes (Rust has no skip at run time), writing its name to the file
//! `BT_GPU_SKIPS` names, which `poe rust-test-gpu` counts aloud (`tools/gpu_skips.py`); any other value is part of the adapter's
//! name the tests must find (CI sets `llvmpipe`, Mesa's software Vulkan, with `BT_GPU_EXPERIMENTAL=1`, which Linux needs).
//!
//! The CPU render is the oracle: every GPU picture is compared with it within a bound named beside the test.

mod parity;
mod recipes;
mod render;
mod session;

use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use bullterrier_scene::{Error, Gpu, Raster, Scene};
use pollster::block_on;

/// The context the tests share, or none where `BT_GPU=none` says the machine has no GPU.
static SHARED: LazyLock<Option<Gpu>> = LazyLock::new(fresh_context);

/// One GPU test at a time: the Vulkan loader (Ubuntu's `libvulkan1`, under lavapipe in CI) crashed in a binary whose tests made
/// and used contexts at once from several threads (image's CI, 2026-11-22). The library holds no state between contexts; this
/// is the loader's race, kept out of the tests.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

thread_local! {
    /// How many contexts this thread's test holds, and the lock while it holds any: a test may hold two.
    static HELD: Cell<usize> = const { Cell::new(0) };
    static GUARD: RefCell<Option<MutexGuard<'static, ()>>> = const { RefCell::new(None) };
}

/// This thread's share of the lock: taken by the first, let go with the last.
struct Alone;

impl Alone {
    fn take() -> Self {
        if HELD.get() == 0 {
            let guard = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
            GUARD.with_borrow_mut(|held| *held = Some(guard));
        }
        HELD.set(HELD.get() + 1);
        Self
    }
}

impl Drop for Alone {
    fn drop(&mut self) {
        HELD.set(HELD.get() - 1);
        if HELD.get() == 0 {
            GUARD.with_borrow_mut(Option::take);
        }
    }
}

/// A context a test holds: no other GPU test runs until it is dropped (with the test's other contexts).
pub(crate) struct Held {
    gpu: Gpu,
    _alone: Alone,
}

impl Deref for Held {
    type Target = Gpu;
    fn deref(&self) -> &Gpu {
        &self.gpu
    }
}

fn held(make: impl FnOnce() -> Option<Gpu>) -> Option<Held> {
    let alone = Alone::take();
    make().map(|gpu| Held { gpu, _alone: alone })
}

/// The shared context, or none (after noting the skip) where the machine has no GPU. Its device cache is as the last test left
/// it: a test that counts transfers or fills the cache takes [`fresh`].
pub(crate) fn gpu() -> Option<Held> {
    let shared = held(|| SHARED.clone());
    if shared.is_none() {
        skipped();
    }
    shared
}

/// Notes that the running test skips for want of a GPU: its name (libtest names its thread so), one line in the file
/// `BT_GPU_SKIPS` names, if it names one.
///
/// # Panics
/// If that file cannot be written: a skip nobody counts is what this guards against.
fn skipped() {
    let Ok(path) = std::env::var("BT_GPU_SKIPS") else {
        return;
    };
    let test = std::thread::current().name().unwrap_or("a GPU test").to_owned();
    let written = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, format!("{test}\n").as_bytes()));
    assert!(
        written.is_ok(),
        "{test}: the skip could not be written to {path}: {written:?}"
    );
}

/// A context of its own, held, for a test that counts transfers or sets a budget; none where the machine has no GPU.
pub(crate) fn fresh() -> Option<Held> {
    let context = held(fresh_context);
    if context.is_none() {
        skipped();
    }
    context
}

/// A new context; none where the machine has no GPU. Made only under the lock ([`held`]).
///
/// # Panics
/// If there is no adapter and `BT_GPU` does not say `none`, or the adapter is not the one `BT_GPU` names.
#[expect(clippy::print_stderr, reason = "a skipped test says why, as BT_GPU=none asks")]
#[expect(clippy::panic, reason = "a missing GPU fails the tests that need it, with its error")]
fn fresh_context() -> Option<Gpu> {
    let expected = std::env::var("BT_GPU").unwrap_or_default();
    if expected == "none" {
        eprintln!("skipped: BT_GPU=none says this machine has no GPU");
        return None;
    }
    let gpu =
        block_on(Gpu::request()).unwrap_or_else(|error| panic!("{error} (set BT_GPU=none on a machine without a GPU)"));
    assert!(
        gpu.adapter().to_lowercase().contains(&expected.to_lowercase()),
        "the adapter is {}, not the {expected:?} BT_GPU names",
        gpu.adapter()
    );
    Some(gpu)
}

/// `scene` rendered at `size` on `gpu` and read back, blocking: what a native caller writes.
pub(crate) fn rendered(gpu: &Gpu, scene: &Scene, size: u32) -> Result<Raster, Error> {
    block_on(async {
        let held = gpu.render(scene, size).await?;
        gpu.download(&held).await
    })
}

/// The largest difference between `cpu` and `gpu` (pictures of one size), value by value, and where it is: the one gap helper
/// of every GPU comparison. Fails the test on a value that is not a number: a kernel that writes NaN must not pass because
/// `f32::max` drops it.
///
/// # Panics
/// If the sizes differ or any value of either picture is not finite.
pub(crate) fn largest_gap(cpu: &Raster, gpu: &Raster) -> (f32, usize) {
    assert_eq!(
        (cpu.width(), cpu.height()),
        (gpu.width(), gpu.height()),
        "the pictures are of one size"
    );
    let mut worst = (0.0_f32, 0_usize);
    for (at, (left, right)) in cpu.pixels().iter().zip(gpu.pixels()).enumerate() {
        assert!(
            left.is_finite() && right.is_finite(),
            "value {at} is not finite: cpu {left}, gpu {right}"
        );
        let gap = (left - right).abs();
        if gap > worst.0 {
            worst = (gap, at);
        }
    }
    worst
}

/// `scene` at `size` on the CPU and on `gpu`: the largest gap between them, which a test holds under its bound.
pub(crate) fn gap_to_cpu(gpu: &Gpu, scene: &Scene, size: u32) -> Result<(f32, usize), Error> {
    Ok(largest_gap(&scene.render(size)?, &rendered(gpu, scene, size)?))
}
