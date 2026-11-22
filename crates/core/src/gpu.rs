//! The GPU compositor (feature `gpu`): [`Gpu`], scene's context on a wgpu device, composes a scene's layers there and keeps the
//! picture on the device.
//!
//! The CPU render is the reference (`Scene::render`, byte promises and all); a render on the GPU is the same scene held to a
//! bound, never to the byte (`docs/design-decisions.md`, "The GPU compositor"). It is a call on this context, the one place
//! that chooses the GPU, and is `async`: nothing here blocks except image's own readback in [`Gpu::download`].
//!
//! Layers are put together on the device in the compositor's own forms: a layer's raster as image holds a picture (straight
//! RGBA, `vec4<f32>` a pixel), the join premultiplied as the CPU's is. What a source draws (a drawing, text, a plugin kind, a
//! group, a picture with its looks) is drawn by the same code as on the CPU and sent to the device once, kept there under the
//! key the CPU cache uses, in a cache of its own (a CPU raster never feeds a render on the device, nor the reverse).

mod compose;
mod executor;

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bullterrier_image::{ErrorKind, Gpu as ImageGpu, GpuImage};

use crate::cache::Cache;
use crate::{Error, Raster, Scene};
use executor::Executor;

/// The device cache's default budget: 1 GiB of buffers.
pub const DEFAULT_CACHE_BYTES: usize = 1 << 30;

/// The scene's GPU context: a wgpu device and queue with the compositor's kernels compiled on it as they are first needed, the
/// image context made from the same device (the pictures of looks stay on it), a cache of what layers drew, kept on the device,
/// and a count of what crossed between the CPU and the device. Cloning it is cheap and shares all of that; it can be shared
/// between threads, which may render on it at once (submissions are serialised inside it).
///
/// Render with [`render`](Self::render), which keeps the picture on the device, and read it back with
/// [`download`](Self::download).
#[derive(Clone)]
pub struct Gpu {
    inner: Arc<Context>,
}

struct Context {
    executor: Executor,
    /// The image crate's context on the same device: the one place that decides which machines the GPU backend supports.
    image: ImageGpu,
    adapter: String,
    /// What layers drew, on the device, by the keys of the CPU cache; its budget is settable.
    cache: Mutex<Arc<Cache>>,
    /// Submissions are made one at a time: the device's error scopes belong to the work between them.
    submitting: Mutex<()>,
    uploads: AtomicU64,
    bytes: AtomicU64,
}

/// What has crossed between the CPU and a [`Gpu`] since it was made or last reset: rasters of drawn sources sent to the device
/// (by this context or by image's, for pictures it renders) and pictures read back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Transfers {
    /// Pictures sent to the device.
    pub uploads: u64,
    /// Pictures read back from it.
    pub downloads: u64,
    /// The bytes of both.
    pub bytes: u64,
}

impl fmt::Debug for Gpu {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Gpu")
            .field("adapter", &self.inner.adapter)
            .finish_non_exhaustive()
    }
}

impl PartialEq for Gpu {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for Gpu {}

/// Image's error as scene's: a machine that cannot stays one, any other is the caller's.
pub(crate) fn from_image(error: &bullterrier_image::Error) -> Error {
    if error.kind() == ErrorKind::Gpu {
        Error::Gpu {
            message: error.to_string(),
        }
    } else {
        Error::invalid("gpu", error.to_string())
    }
}

/// The failure of a wgpu call, as the core reports it.
pub(crate) fn failed(what: &str, error: impl fmt::Display) -> Error {
    Error::Gpu {
        message: format!("the GPU failed to {what}: {error}"),
    }
}

impl Gpu {
    /// A context on a device of its own: the machine's high-performance adapter, with its own limits on the size of a storage
    /// buffer (so the largest picture is what the adapter holds, not WebGPU's default of 128 MiB). The platforms the GPU
    /// backend supports are image's to say: its refusal is this one's.
    ///
    /// # Errors
    /// [`Error::Gpu`] if there is no adapter, it gives no device, or the platform is not supported.
    pub async fn request() -> Result<Self, Error> {
        Self::on(ImageGpu::request().await.map_err(|error| from_image(&error))?)
    }

    /// A context on the caller's `device` and `queue`: the compositor compiles its kernels on that device and keeps its pictures
    /// there, so a caller's own GPU work and scene's share them. The caller keeps the device; no handler is set on it.
    ///
    /// # Errors
    /// [`Error::Gpu`] naming the limit, if the device allows less than the kernels use, or the platform is not supported.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, Error> {
        Self::on(ImageGpu::new(device, queue).map_err(|error| from_image(&error))?)
    }

    /// The context on image's, whose device the compositor compiles its kernels on.
    fn on(image: ImageGpu) -> Result<Self, Error> {
        let adapter = image.adapter().to_owned();
        let executor =
            Executor::new(image.device().clone(), image.queue().clone()).map_err(|message| Error::Gpu { message })?;
        Ok(Self {
            inner: Arc::new(Context {
                executor,
                image,
                adapter,
                cache: Mutex::new(Arc::new(Cache::with_capacity(DEFAULT_CACHE_BYTES))),
                submitting: Mutex::new(()),
                uploads: AtomicU64::new(0),
                bytes: AtomicU64::new(0),
            }),
        })
    }

    /// The adapter the device is on, as its driver names it: name, backend, kind and driver.
    #[must_use]
    pub fn adapter(&self) -> &str {
        &self.inner.adapter
    }

    /// What has crossed between the CPU and this context since it was made or last [reset](Self::reset_transfers).
    #[must_use]
    pub fn transfers(&self) -> Transfers {
        let image = self.inner.image.transfers();
        Transfers {
            uploads: image.uploads + self.inner.uploads.load(Ordering::Relaxed),
            downloads: image.downloads,
            bytes: image.bytes + self.inner.bytes.load(Ordering::Relaxed),
        }
    }

    /// Sets the count of [`transfers`](Self::transfers) back to zero.
    pub fn reset_transfers(&self) {
        self.inner.image.reset_transfers();
        self.inner.uploads.store(0, Ordering::Relaxed);
        self.inner.bytes.store(0, Ordering::Relaxed);
    }

    /// The most bytes of drawn layers, masks and averaged rasters this context keeps on the device between renders
    /// ([`DEFAULT_CACHE_BYTES`] unless set).
    #[must_use]
    pub fn cache_budget(&self) -> usize {
        self.cache().capacity()
    }

    /// Sets the budget of the device cache to `bytes` (a phone sets less). What is kept is dropped: the next render draws and
    /// sends again what it needs.
    pub fn set_cache_budget(&self, bytes: usize) {
        *self.inner.cache.lock().unwrap_or_else(PoisonError::into_inner) = Arc::new(Cache::with_capacity(bytes));
    }

    fn cache(&self) -> Arc<Cache> {
        Arc::clone(&self.inner.cache.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// The scene at `size` on its long side (as [`Scene::render`] takes it), composed on the device and left there: image's
    /// [`GpuImage`], straight alpha, the levels [`Scene::render`] gives. A scene's looks, masks, warps, blends and groups run on
    /// the device; what a source draws (a drawing, text, a plugin kind) is drawn on the CPU once and sent. Within the bounds
    /// of `docs/design-decisions.md`, "The GPU compositor", not to the byte of the CPU render.
    ///
    /// # Errors
    /// [`Error::Gpu`] if the picture is larger than the device holds (naming that the CPU can) or the device fails; whatever
    /// [`Scene::render`] refuses (naming `size`, or a layer's look, file or drawing).
    pub async fn render(&self, scene: &Scene, size: impl Into<i128>) -> Result<GpuImage, Error> {
        let (picture, rows, columns) = compose::render(self, scene, size.into()).await?;
        self.inner
            .image
            .adopt(picture, rows, columns, true)
            .map_err(|error| from_image(&error))
    }

    /// `image` read back as a [`Raster`]: what [`Scene::render`] returns. Image's `download` waits for the device on native
    /// targets (the one wait, named in `docs/design-decisions.md`).
    ///
    /// # Errors
    /// [`Error::Gpu`] if the device fails while reading it, or `image` is another context's.
    pub async fn download(&self, image: &GpuImage) -> Result<Raster, Error> {
        let image = self
            .inner
            .image
            .download(image)
            .await
            .map_err(|error| from_image(&error))?;
        let (height, width) = (image.height(), image.width());
        let channels = image.channels();
        let values = image.into_values();
        let pixels = if channels == 4 {
            values
        } else {
            values
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 1.0])
                .collect()
        };
        Ok(Raster::made(width, height, pixels))
    }

    /// The sizes the device holds, as an error: `columns` by `rows` pixels of `vec4<f32>`.
    pub(crate) fn fits(&self, columns: usize, rows: usize) -> Result<(), Error> {
        let executor = &self.inner.executor;
        let bytes = (columns as u64).saturating_mul(rows as u64).saturating_mul(16);
        if bytes > executor.largest_binding() || columns.max(rows) > executor.longest_side() {
            return Err(Error::Gpu {
                message: format!(
                    "a picture of {columns} x {rows} pixels is larger than the GPU holds (at most {} MiB in a buffer, {} a side \
                     on {}): render it on the CPU",
                    executor.largest_binding() >> 20,
                    executor.longest_side(),
                    self.inner.adapter
                ),
            });
        }
        Ok(())
    }

    /// `work` done with the device's validation and out-of-memory errors caught, so a failure is an [`Error::Gpu`] naming
    /// `what` instead of wgpu's panic, and with no other submission at once.
    pub(crate) async fn guarded<T>(&self, what: &str, work: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
        let (value, validation, memory) = {
            let _alone = self.inner.submitting.lock().unwrap_or_else(PoisonError::into_inner);
            let device = &self.inner.executor.device;
            let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
            let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let value = work();
            // popped before anything is awaited: a scope's guard stays on its thread
            (value, validation.pop(), memory.pop())
        };
        let (validation, memory) = (validation.await, memory.await);
        let value = value?;
        if let Some(error) = validation.or(memory) {
            return Err(failed(what, error));
        }
        Ok(value)
    }

    fn count_upload(&self, bytes: usize) {
        self.inner.uploads.fetch_add(1, Ordering::Relaxed);
        self.inner.bytes.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

/// The answer of `future`, which a device answers at once: the calls of image's context that wait for nothing but the device's
/// error scopes, and on native those are ready when asked. Where the answer is not there, the machine drives its device from
/// elsewhere (a browser), and the compositor's recording, which is synchronous, cannot wait for it.
///
/// # Errors
/// [`Error::Gpu`] naming that, for a future that is not ready at its first poll.
pub(crate) fn ready<T>(future: impl std::future::Future<Output = T>) -> Result<T, Error> {
    use std::task::{Context, Poll, Waker};
    match std::pin::pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Ok(value),
        Poll::Pending => Err(Error::Gpu {
            message: "this device answers asynchronously, which a render with looks on the device does not wait for yet: render on the CPU"
                .to_owned(),
        }),
    }
}
