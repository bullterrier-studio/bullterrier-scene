//! The GPU compositor (feature `gpu`): `Gpu` and `Scene.render_on`. Python's calls block at this edge, on the core's async calls.

use numpy::{PyArray1, PyArray3, PyArrayMethods};
use pyo3::prelude::*;

use crate::wrap;

/// A GPU context: a device, with the compositor's kernels and a cache of what layers drew, kept on it.
#[pyclass(frozen, name = "Gpu", module = "bullterrier_scene._native._core")]
pub(crate) struct Gpu(bullterrier_scene::Gpu);

#[pymethods]
impl Gpu {
    /// A context on this machine's GPU. `GpuError` where it has none.
    #[new]
    fn new(py: Python<'_>) -> PyResult<Self> {
        wrap(py, py.detach(|| pollster::block_on(bullterrier_scene::Gpu::request()))).map(Self)
    }

    /// The adapter the device is on.
    #[getter]
    fn adapter(&self) -> String {
        self.0.adapter().to_owned()
    }

    /// `(uploads, downloads, bytes)` since the context was made or last reset.
    fn transfers(&self) -> (u64, u64, u64) {
        let counted = self.0.transfers();
        (counted.uploads, counted.downloads, counted.bytes)
    }

    /// Sets the count of transfers back to zero.
    fn reset_transfers(&self) {
        self.0.reset_transfers();
    }

    /// The bytes of layers the device cache keeps between renders.
    #[getter]
    fn cache_budget(&self) -> usize {
        self.0.cache_budget()
    }

    /// Sets the budget of the device cache (what is kept is dropped).
    #[setter]
    fn set_cache_budget(&self, bytes: usize) {
        self.0.set_cache_budget(bytes);
    }
}

/// `scene` at `size`, composed on `gpu` and read back as a float32 array (height, width, 4).
pub(crate) fn render<'py>(
    py: Python<'py>,
    scene: &bullterrier_scene::Scene,
    size: i128,
    gpu: &Gpu,
) -> PyResult<Bound<'py, PyArray3<f32>>> {
    let raster = wrap(
        py,
        py.detach(|| {
            pollster::block_on(async {
                let held = gpu.0.render(scene, size).await?;
                gpu.0.download(&held).await
            })
        }),
    )?;
    let (height, width) = (raster.height(), raster.width());
    PyArray1::from_vec(py, raster.into_pixels()).reshape([height, width, 4])
}
