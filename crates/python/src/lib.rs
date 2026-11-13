//! `bullterrier_scene._native._core`: gives the Rust core to Python. Generic: it holds no logic of the core, only the mapping
//! of its calls, its arrays and its errors. A refusal of the core is raised as the matching class of
//! `bullterrier_scene.errors`, imported when the first one is raised: importing the extension imports nothing of the package.

mod error;

use std::path::PathBuf;

use numpy::{Element, PyArray1, PyArray3, PyArrayMethods, PyReadonlyArrayDyn, PyUntypedArrayMethods};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use bullterrier_scene::{Deg, Fit, Place, Raster};

/// bullterrier-scene's Rust core.
#[pymodule]
mod _core {
    #[pymodule_export]
    use super::{Layer, Scene};

    /// The most layers a scene holds.
    #[pymodule_export]
    const MAX_LAYERS: usize = bullterrier_scene::MAX_LAYERS;
}

/// The red, green, blue and alpha values of `array` (height, width, 3 or 4 channels) as floats from 0 to 1: `scale` maps its
/// element type to that range. Three channels are opaque.
fn levels<T: Element + Copy>(
    py: Python<'_>,
    array: &PyReadonlyArrayDyn<'_, T>,
    to_level: impl Fn(T) -> f32,
) -> PyResult<Raster> {
    let shape = array.shape();
    let &[height, width, channels @ (3 | 4)] = shape else {
        return Err(error::invalid(
            py,
            format!("pixels: a picture array is (height, width, 3 or 4), got shape {shape:?}"),
        ));
    };
    let values = array.as_array();
    let mut pixels = Vec::with_capacity(height * width * 4);
    for (index, value) in values.iter().enumerate() {
        pixels.push(to_level(*value));
        if channels == 3 && index % 3 == 2 {
            pixels.push(1.0);
        }
    }
    Raster::rgba(width, height, pixels).map_err(|failure| error::raised(py, &failure))
}

/// A picture as the core takes it: an array of `uint8`, `uint16`, `float32` or `float64`, (height, width, 3 or 4).
fn raster_of(py: Python<'_>, array: &Bound<'_, PyAny>) -> PyResult<Raster> {
    if let Ok(found) = array.extract::<PyReadonlyArrayDyn<'_, u8>>() {
        return levels(py, &found, |value| f32::from(value) / 255.0);
    }
    if let Ok(found) = array.extract::<PyReadonlyArrayDyn<'_, u16>>() {
        return levels(py, &found, |value| f32::from(value) / 65535.0);
    }
    if let Ok(found) = array.extract::<PyReadonlyArrayDyn<'_, f32>>() {
        return levels(py, &found, |value| value);
    }
    if let Ok(found) = array.extract::<PyReadonlyArrayDyn<'_, f64>>() {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a level from 0 to 1 is held as a 32-bit float"
        )]
        return levels(py, &found, |value| value as f32);
    }
    let dtype = array.getattr("dtype").ok();
    let text = |name: &str| {
        dtype
            .as_ref()
            .and_then(|dtype| dtype.getattr(name).ok())
            .and_then(|value| value.str().ok())
    };
    // `byteorder` is `>` or `<` only for an order that is not the machine's
    if text("byteorder").is_some_and(|order| matches!(order.to_string().as_str(), ">" | "<")) {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "a picture array is in the other byte order than this machine's: convert it with \
             array.astype(array.dtype.newbyteorder(\"=\"))",
        ));
    }
    let got = text("name").map_or_else(
        || "something that is no numpy array".to_owned(),
        |name| format!("dtype {name}"),
    );
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "a picture array holds uint8, uint16, float32 or float64 values, shaped (height, width, 3 or 4), got {got}"
    )))
}

/// One layer of a scene: a source, and how it joins the layers below it. It never changes; a call returns a new one.
#[pyclass(frozen, name = "Layer", module = "bullterrier_scene._native._core")]
struct Layer(bullterrier_scene::Layer);

/// A canvas and a stack of layers, bottom first. It never changes; a call returns a new one.
#[pyclass(frozen, name = "Scene", module = "bullterrier_scene._native._core")]
struct Scene(bullterrier_scene::Scene);

/// `result` with a refusal of the core raised as the package's exception.
fn wrap<T>(py: Python<'_>, result: Result<T, bullterrier_scene::Error>) -> PyResult<T> {
    result.map_err(|failure| error::raised(py, &failure))
}

#[pymethods]
impl Layer {
    /// A picture from an array (height, width, 3 or 4 channels) of `uint8`, `uint16`, `float32` or `float64`; four channels
    /// are straight alpha. A shape or a value outside 0 to 1 is an `InvalidError`.
    #[staticmethod]
    fn picture(py: Python<'_>, pixels: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(bullterrier_scene::Layer::picture(raster_of(py, pixels)?)))
    }

    /// A picture read from the file `path` when the scene renders.
    #[staticmethod]
    fn picture_file(path: PathBuf) -> Self {
        Self(bullterrier_scene::Layer::picture_file(path))
    }

    /// A vector drawing, given as its JSON document.
    #[staticmethod]
    fn drawing(py: Python<'_>, json: &str) -> PyResult<Self> {
        wrap(py, bullterrier_scene::Layer::drawing(json)).map(Self)
    }

    /// A flat colour over the whole canvas: `#rrggbb` or `#rrggbbaa`.
    #[staticmethod]
    fn fill(py: Python<'_>, color: &str) -> PyResult<Self> {
        wrap(py, bullterrier_scene::Layer::fill(color)).map(Self)
    }

    /// This layer called `name`.
    fn named(&self, py: Python<'_>, name: &str) -> PyResult<Self> {
        wrap(py, self.0.clone().named(name)).map(Self)
    }

    /// This layer at `value` of its own alpha, from 0 to 1.
    fn opacity(&self, py: Python<'_>, value: f64) -> PyResult<Self> {
        wrap(py, self.0.clone().opacity(value)).map(Self)
    }

    /// This layer's box fitted into `area` = (x, y, width, height) design pixels: `how` is "cover", "contain" or "stretch";
    /// `align` is two fractions from 0 to 1.
    fn fit(&self, py: Python<'_>, area: [f64; 4], how: &str, align: [f64; 2]) -> PyResult<Self> {
        let how: Fit = wrap(py, how.parse())?;
        wrap(py, self.0.clone().fit(area, how, align)).map(Self)
    }

    /// This layer with the placement fields given set and the others kept; a pair is `flip` (x, y), `skew` (x, y) or
    /// `anchor` (x, y).
    #[pyo3(signature = (*, dx=None, dy=None, scale=None, aspect=None, rotate=None, skew=None, flip=None, anchor=None))]
    #[expect(
        clippy::too_many_arguments,
        reason = "the fields of a placement, as keyword arguments"
    )]
    fn place(
        &self,
        py: Python<'_>,
        dx: Option<f64>,
        dy: Option<f64>,
        scale: Option<f64>,
        aspect: Option<f64>,
        rotate: Option<f64>,
        skew: Option<[f64; 2]>,
        flip: Option<[bool; 2]>,
        anchor: Option<[f64; 2]>,
    ) -> PyResult<Self> {
        let mut place = Place::new();
        place = dx.into_iter().fold(place, Place::dx);
        place = dy.into_iter().fold(place, Place::dy);
        place = scale.into_iter().fold(place, Place::scale);
        place = aspect.into_iter().fold(place, Place::aspect);
        place = rotate.into_iter().fold(place, |place, angle| place.rotate(Deg(angle)));
        if let Some([x, y]) = skew {
            place = place.skew_x(Deg(x)).skew_y(Deg(y));
        }
        if let Some([x, y]) = flip {
            place = place.flip_x(x).flip_y(y);
        }
        if let Some([x, y]) = anchor {
            place = place.anchor(x, y);
        }
        wrap(py, self.0.clone().place(place)).map(Self)
    }

    /// This layer with the image look written as the recipe document `json`.
    fn look(&self, py: Python<'_>, json: &str) -> PyResult<Self> {
        wrap(py, self.0.clone().look(json)).map(Self)
    }

    /// This layer with the look of the recipe file `path`, read now.
    fn look_file(&self, py: Python<'_>, path: PathBuf) -> PyResult<Self> {
        wrap(py, self.0.clone().look_file(path)).map(Self)
    }
}

#[pymethods]
impl Scene {
    /// An empty canvas of `width` by `height` design pixels.
    #[new]
    fn new(py: Python<'_>, width: i128, height: i128) -> PyResult<Self> {
        let canvas = bullterrier_scene::Scene::new(width, height);
        wrap(py, canvas).map(Self)
    }

    /// This scene with `layers` on top of its stack, the first lowest.
    #[expect(clippy::needless_pass_by_value, reason = "pyo3 hands the extracted list over")]
    fn add(&self, py: Python<'_>, layers: Vec<PyRef<'_, Layer>>) -> PyResult<Self> {
        wrap(py, self.0.add(layers.iter().map(|layer| layer.0.clone()))).map(Self)
    }

    /// The layers, bottom first.
    #[getter]
    fn layers<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        PyTuple::new(py, self.0.layers().iter().map(|layer| Layer(layer.clone())))
    }

    /// The canvas's width in design pixels.
    #[getter]
    fn width(&self) -> u32 {
        self.0.width()
    }

    /// The canvas's height in design pixels.
    #[getter]
    fn height(&self) -> u32 {
        self.0.height()
    }

    /// The picture as a new float32 array (height, width, 4): straight alpha, 0 to 1, rows from the top.
    fn render<'py>(&self, py: Python<'py>, size: i128) -> PyResult<Bound<'py, PyArray3<f32>>> {
        let raster = wrap(py, py.detach(|| self.0.render(size)))?;
        let (height, width) = (raster.height(), raster.width());
        PyArray1::from_vec(py, raster.into_pixels()).reshape([height, width, 4])
    }
}
