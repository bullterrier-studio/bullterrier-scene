//! `bullterrier_scene._native._core`: gives the Rust core to Python. Generic: it holds no logic of the core, only the mapping
//! of its calls, its arrays and its errors. A refusal of the core is raised as the matching class of
//! `bullterrier_scene.errors`, imported when the first one is raised: importing the extension imports nothing of the package.

mod error;
#[cfg(feature = "gpu")]
mod gpu;
mod kinds;

use std::borrow::Cow;
use std::path::PathBuf;

use numpy::{Element, PyArray1, PyArray2, PyArray3, PyArrayMethods, PyReadonlyArrayDyn, PyUntypedArrayMethods};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

use bullterrier_scene::{
    BleedFill, Blend, Deg, Fit, Map, Mask, Parameter, PdfOptions, Place, Px, Raster, Text, Underbase, Use, Value,
    Values,
};

/// bullterrier-scene's Rust core.
#[pymodule]
mod _core {
    #[cfg(feature = "gpu")]
    #[pymodule_export]
    use super::gpu::Gpu;
    #[pymodule_export]
    use super::kinds::{Snapshot, layer_kinds, register_layer, registry_restore, registry_snapshot};
    #[pymodule_export]
    use super::{Held, Layer, Plates, Scene, check_page, check_underbase, main};

    /// The most layers a scene holds.
    #[pymodule_export]
    const MAX_LAYERS: usize = bullterrier_scene::MAX_LAYERS;

    /// The deepest masks nest.
    #[pymodule_export]
    const MAX_DEPTH: usize = bullterrier_scene::MAX_DEPTH;
}

/// Runs the command line `bt-scene` with `argv` (the arguments after the program's name) and returns its exit code. Results go
/// to stdout and errors to stderr, as the process's own; `version` is what `--version` prints. Ctrl-C is not seen while it runs:
/// the caller restores the default signal handler first.
#[pyfunction]
#[expect(clippy::needless_pass_by_value, reason = "PyO3 extracts a list by value")]
fn main(py: Python<'_>, argv: Vec<String>, version: &str) -> i32 {
    py.detach(|| {
        let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
        bullterrier_scene_cli::main(&argv, version, &mut out, &mut err)
    })
}

/// The elements of `array` in C order, borrowed when it is C-contiguous (else copied once): a flat slice, which the compiler
/// turns into vector code where a walk over the elements of a strided view would not.
fn flat_of<'a, T: Element + Copy>(array: &'a PyReadonlyArrayDyn<'_, T>) -> Cow<'a, [T]> {
    match array.as_slice() {
        Ok(flat) => Cow::Borrowed(flat),
        Err(_) => Cow::Owned(
            array
                .as_array()
                .as_standard_layout()
                .into_owned()
                .into_raw_vec_and_offset()
                .0,
        ),
    }
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
    let flat = flat_of(array);
    let pixels = if channels == 4 {
        flat.iter().map(|value| to_level(*value)).collect()
    } else {
        let mut pixels = vec![1.0; height * width * 4];
        for (pixel, rgb) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(flat.as_chunks::<3>().0) {
            pixel[0] = to_level(rgb[0]);
            pixel[1] = to_level(rgb[1]);
            pixel[2] = to_level(rgb[2]);
        }
        pixels
    };
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
    Err(unsupported(array, "a picture array", "(height, width, 3 or 4)"))
}

/// A map as the core takes it: an array of `uint8`, `uint16`, `float32` or `float64`, (height, width), levels from 0 to 1.
fn map_of(py: Python<'_>, array: &Bound<'_, PyAny>) -> PyResult<Map> {
    fn grey<T: Element + Copy>(
        py: Python<'_>,
        array: &PyReadonlyArrayDyn<'_, T>,
        to_level: impl Fn(T) -> f32,
    ) -> PyResult<Map> {
        let shape = array.shape();
        let &[height, width] = shape else {
            return Err(error::invalid(
                py,
                format!("values: a map array is (height, width), got shape {shape:?}"),
            ));
        };
        let values = flat_of(array).iter().map(|value| to_level(*value)).collect();
        Map::new(width, height, values).map_err(|failure| error::raised(py, &failure))
    }
    if let Ok(found) = array.extract::<PyReadonlyArrayDyn<'_, u8>>() {
        return grey(py, &found, |value| f32::from(value) / 255.0);
    }
    if let Ok(found) = array.extract::<PyReadonlyArrayDyn<'_, u16>>() {
        return grey(py, &found, |value| f32::from(value) / 65535.0);
    }
    if let Ok(found) = array.extract::<PyReadonlyArrayDyn<'_, f32>>() {
        return grey(py, &found, |value| value);
    }
    if let Ok(found) = array.extract::<PyReadonlyArrayDyn<'_, f64>>() {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a level from 0 to 1 is held as a 32-bit float"
        )]
        return grey(py, &found, |value| value as f32);
    }
    Err(unsupported(array, "a map array", "(height, width)"))
}

/// The `TypeError` for an array that is none of the element types a picture or a map takes: what is wrong with it, and how to fix
/// it where that is known (the other byte order).
fn unsupported(array: &Bound<'_, PyAny>, what: &str, shaped: &str) -> PyErr {
    let dtype = array.getattr("dtype").ok();
    let text = |name: &str| {
        dtype
            .as_ref()
            .and_then(|dtype| dtype.getattr(name).ok())
            .and_then(|value| value.str().ok())
    };
    // `byteorder` is `>` or `<` only for an order that is not the machine's
    if text("byteorder").is_some_and(|order| matches!(order.to_string().as_str(), ">" | "<")) {
        return pyo3::exceptions::PyTypeError::new_err(format!(
            "{what} is in the other byte order than this machine's: convert it with \
             array.astype(array.dtype.newbyteorder(\"=\"))"
        ));
    }
    let got = text("name").map_or_else(
        || "something that is no numpy array".to_owned(),
        |name| format!("dtype {name}"),
    );
    let hint = if text("name").is_some_and(|name| name == "bool") {
        ": a bool array is converted with array.astype(np.float32)"
    } else {
        ""
    };
    pyo3::exceptions::PyTypeError::new_err(format!(
        "{what} holds uint8, uint16, float32 or float64 values, shaped {shaped}, got {got}{hint}"
    ))
}

/// A picture or a map a scene holds, as a parameter's value: it shares the pixels (no copy), `numpy.asarray(held)` makes the
/// array, and `with_values` takes it back.
#[pyclass(frozen, eq, name = "Held", module = "bullterrier_scene._native._core")]
#[derive(PartialEq)]
struct Held(Value);

#[pymethods]
impl Held {
    /// (height, width, 4) for a picture, (height, width) for a map.
    #[getter]
    fn shape<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        match &self.0 {
            Value::Picture(raster) => PyTuple::new(py, [raster.height(), raster.width(), 4]),
            Value::Map(map) => PyTuple::new(py, [map.height(), map.width()]),
            _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
                "a held value is a picture or a map",
            )),
        }
    }

    /// The pixels as a new float32 array, red, green, blue and alpha (straight, 0 to 1) for a picture.
    #[pyo3(signature = (dtype=None, copy=None))]
    fn __array__<'py>(
        &self,
        py: Python<'py>,
        dtype: Option<&Bound<'py, PyAny>>,
        copy: Option<bool>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = copy;
        let array = match &self.0 {
            Value::Picture(raster) => PyArray1::from_slice(py, raster.pixels())
                .reshape([raster.height(), raster.width(), 4])?
                .into_any(),
            Value::Map(map) => PyArray1::from_slice(py, map.values())
                .reshape([map.height(), map.width()])?
                .into_any(),
            _ => {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "a held value is a picture or a map",
                ));
            }
        };
        match dtype {
            Some(dtype) if !dtype.is_none() => array.call_method1("astype", (dtype,)),
            _ => Ok(array),
        }
    }

    fn __repr__(&self) -> String {
        match &self.0 {
            Value::Picture(raster) => format!("Held(picture {} x {})", raster.width(), raster.height()),
            Value::Map(map) => format!("Held(map {} x {})", map.width(), map.height()),
            _ => "Held()".to_owned(),
        }
    }
}

/// The Python object a parameter's value is: a number, bool or text as itself, a colour of three channels as a tuple, a path as a `pathlib.Path`, nothing as `None`,
/// a held picture or map as a `Held`.
fn to_python(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    Ok(match value {
        Value::Number(number) => PyFloat::new(py, *number).into_any().unbind(),
        Value::Bool(on) => PyBool::new(py, *on).to_owned().into_any().unbind(),
        Value::Text(text) => PyString::new(py, text).into_any().unbind(),
        Value::Rgb(channels) => PyTuple::new(py, channels)?.into_any().unbind(),
        Value::Picture(_) | Value::Map(_) => Py::new(py, Held(value.clone()))?.into_any(),
        Value::File(path) => py.import("pathlib")?.getattr("Path")?.call1((path,))?.unbind(),
        Value::Hole => py.None(),
        _ => {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "a value of a kind this binding does not know",
            ));
        }
    })
}

/// A parameter as the dict the facade makes a `Parameter` of.
fn described<'py>(py: Python<'py>, parameter: &Parameter) -> PyResult<Bound<'py, PyDict>> {
    let described = PyDict::new(py);
    described.set_item("address", &parameter.address)?;
    described.set_item("kind", parameter.kind.name())?;
    described.set_item("meaning", &parameter.meaning)?;
    described.set_item("default", to_python(py, &parameter.default)?)?;
    described.set_item("unit", parameter.unit.as_deref())?;
    described.set_item("min", parameter.min)?;
    described.set_item("max", parameter.max)?;
    described.set_item("cyclic", parameter.cyclic)?;
    described.set_item("choices", &parameter.choices)?;
    described.set_item("nullable", parameter.nullable)?;
    described.set_item("min_length", parameter.min_length)?;
    described.set_item("value", to_python(py, &parameter.value)?)?;
    Ok(described)
}

/// The value a Python object is to the core: `None` is a hole, a `bool` a switch, a `str` text, a `Held` its picture or map, a
/// path a file, a 2-dimensional array a map and another array a picture, a `float` or `int` a number.
fn value_of(py: Python<'_>, address: &str, value: &Bound<'_, PyAny>) -> PyResult<Value> {
    if value.is_none() {
        return Ok(Value::Hole);
    }
    if value.is_instance_of::<PyBool>() {
        return Ok(Value::Bool(value.extract()?));
    }
    if value.is_instance_of::<PyString>() {
        return Ok(Value::Text(value.extract()?));
    }
    let channels = value.is_instance_of::<PyTuple>()
        || value.is_instance_of::<PyList>()
        || value.getattr("ndim").ok().and_then(|ndim| ndim.extract::<usize>().ok()) == Some(1);
    if channels {
        return value.extract::<[f64; 3]>().map(Value::Rgb).map_err(|_| {
            error::invalid(
                py,
                format!("{address}: a colour is three numbers from 0 to 1, red, green and blue"),
            )
        });
    }
    if let Ok(held) = value.extract::<PyRef<'_, Held>>() {
        return Ok(held.0.clone());
    }
    if value.is_instance(&py.import("pathlib")?.getattr("PurePath")?)? {
        return Ok(Value::File(value.extract::<PathBuf>()?));
    }
    if let Some(dimensions) = value.getattr("ndim").ok().and_then(|ndim| ndim.extract::<usize>().ok()) {
        return Ok(if dimensions == 2 {
            Value::Map(map_of(py, value)?.into())
        } else {
            Value::Picture(raster_of(py, value)?.into())
        });
    }
    if value.is_instance_of::<PyFloat>() || value.is_instance_of::<PyInt>() {
        return value.extract::<f64>().map(Value::Number).map_err(|_| {
            error::invalid(
                py,
                format!("{address}: a number beyond the range of a float is no value"),
            )
        });
    }
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "{address}: a value is a number, a bool, a text, three numbers (a colour), a Path, an array, a held picture or None, got {}",
        value.get_type().name()?
    )))
}

/// One layer of a scene: a source, and how it joins the layers below it. It never changes; a call returns a new one.
#[pyclass(frozen, eq, name = "Layer", module = "bullterrier_scene._native._core")]
#[derive(PartialEq)]
struct Layer(bullterrier_scene::Layer);

/// A canvas and a stack of layers, bottom first. It never changes; a call returns a new one.
#[pyclass(frozen, eq, name = "Scene", module = "bullterrier_scene._native._core")]
#[derive(PartialEq)]
struct Scene(bullterrier_scene::Scene);

/// The plates of a scene: one plane of ink coverage for each ink. It never changes.
#[pyclass(frozen, eq, name = "Plates", module = "bullterrier_scene._native._core")]
#[derive(PartialEq)]
struct Plates(bullterrier_scene::Plates);

/// The print page of `dpi`, `bleed_mm`, the bleed's `fill` ("repeat" or "mirror") and the marks.
fn page_options(
    py: Python<'_>,
    (dpi, bleed_mm, fill, crop_marks, registration_marks): (f64, f64, String, bool, bool),
) -> PyResult<PdfOptions> {
    let fill = match fill.as_str() {
        "repeat" => BleedFill::Repeat,
        "mirror" => BleedFill::Mirror,
        other => {
            return Err(error::invalid(
                py,
                format!("bleed_fill: \"repeat\" or \"mirror\" is wanted, got {other:?}"),
            ));
        }
    };
    let options = PdfOptions::new(dpi).and_then(|options| options.bleed_mm(bleed_mm));
    Ok(wrap(py, options)?
        .bleed_fill(fill)
        .crop_marks(crop_marks)
        .registration_marks(registration_marks))
}

/// The underbase of (garment, choke, tolerance, feather, name), each checked by the core.
fn underbase_of(
    py: Python<'_>,
    (garment, choke, tolerance, feather, name): (String, f64, f64, f64, String),
) -> PyResult<Underbase> {
    wrap(
        py,
        Underbase::new(&garment)
            .and_then(|underbase| underbase.choke(choke))
            .and_then(|underbase| underbase.tolerance(tolerance))
            .and_then(|underbase| underbase.feather(feather))
            .and_then(|underbase| underbase.name(&name)),
    )
}

/// Raises the package's `InvalidError` for an underbase the core would refuse: what `Underbase(...)` asks at construction.
#[pyfunction]
fn check_underbase(py: Python<'_>, arguments: (String, f64, f64, f64, String)) -> PyResult<()> {
    underbase_of(py, arguments).map(|_| ())
}

/// Raises the package's `InvalidError` for a print page the core would refuse: what `PdfOptions(...)` asks at construction.
#[pyfunction]
fn check_page(py: Python<'_>, options: (f64, f64, String, bool, bool)) -> PyResult<()> {
    page_options(py, options).map(|_| ())
}

/// `result` with a refusal of the core raised as the package's exception.
fn wrap<T>(py: Python<'_>, result: Result<T, bullterrier_scene::Error>) -> PyResult<T> {
    result.map_err(|failure| error::raised(py, &failure))
}

/// The text layer of `content` in `text`, in `color`, with each member of the JSON object `options` as a vector text option.
fn typeset(py: Python<'_>, content: &str, text: Text, color: &str, options: &str) -> PyResult<Layer> {
    let serde_json::Value::Object(options) = serde_json::from_str(options)
        .map_err(|failure| error::invalid(py, format!("text: the options are not JSON: {failure}")))?
    else {
        return Err(error::invalid(py, "text: the options are a JSON object".to_owned()));
    };
    let text = options
        .into_iter()
        .fold(text.color(color), |text, (name, value)| text.option(&name, value));
    wrap(py, bullterrier_scene::Layer::text(content, &text)).map(Layer)
}

/// `layer` with `mask` set to read `uses` ("alpha" or "luminance"), blurred by `feather` design pixels and inverted if `invert`.
fn masked(
    layer: &bullterrier_scene::Layer,
    mask: Mask,
    uses: &str,
    feather: f64,
    invert: bool,
) -> Result<bullterrier_scene::Layer, bullterrier_scene::Error> {
    let mask = mask.using(uses.parse::<Use>()?).feather(feather);
    layer.clone().mask(if invert { mask.invert() } else { mask })
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

    /// `content` set in the bundled font `font`, in `color`, with the vector text options of the JSON object `options`.
    #[staticmethod]
    fn text(py: Python<'_>, content: &str, font: &str, color: &str, options: &str) -> PyResult<Self> {
        typeset(py, content, Text::new(font), color, options)
    }

    /// `content` set in the font file `path`, in `color`, with the vector text options of the JSON object `options`.
    #[staticmethod]
    fn text_file(py: Python<'_>, content: &str, path: PathBuf, color: &str, options: &str) -> PyResult<Self> {
        typeset(py, content, Text::from_file(path), color, options)
    }

    /// A layer of the registered kind `kind`, with `options` (JSON text of an object).
    #[staticmethod]
    fn make(py: Python<'_>, kind: &str, options: &str) -> PyResult<Self> {
        let options = serde_json::from_str(options)
            .map_err(|failure| error::invalid(py, format!("options: not JSON ({failure})")))?;
        wrap(py, kinds::current(py)?.make(kind, &options)).map(Self)
    }

    /// A flat colour over the whole canvas: `#rrggbb` or `#rrggbbaa`.
    #[staticmethod]
    fn fill(py: Python<'_>, color: &str) -> PyResult<Self> {
        wrap(py, bullterrier_scene::Layer::fill(color)).map(Self)
    }

    /// The scene `inner` as a layer: a group.
    #[staticmethod]
    fn scene(inner: &Scene) -> Self {
        Self(bullterrier_scene::Layer::scene(inner.0.clone()))
    }

    /// A reference to the layer called `name` of the scene it is added to.
    #[staticmethod]
    fn of(name: &str) -> Self {
        Self(bullterrier_scene::Layer::of(name))
    }

    /// A picture to be given later: a hole, which `with_values` fills.
    #[staticmethod]
    fn picture_hole() -> Self {
        Self(bullterrier_scene::Layer::picture_hole())
    }

    /// This picture layer with the array `values` (height, width; levels 0 to 1) as its map `name`.
    fn map(&self, py: Python<'_>, name: &str, values: &Bound<'_, PyAny>) -> PyResult<Self> {
        wrap(py, self.0.clone().map(name, map_of(py, values)?)).map(Self)
    }

    /// This picture layer with the grey picture in the file `path` as its map `name`, read when the scene renders.
    fn map_file(&self, py: Python<'_>, name: &str, path: PathBuf) -> PyResult<Self> {
        wrap(py, self.0.clone().map_file(name, path)).map(Self)
    }

    /// This picture layer with a hole for its map `name`: a render refuses it until parameters fill holes.
    fn map_hole(&self, py: Python<'_>, name: &str) -> PyResult<Self> {
        wrap(py, self.0.clone().map_hole(name)).map(Self)
    }

    /// This layer masked by `by`: a layer drawn on the canvas, or the name of one of this picture layer's maps.
    fn mask(&self, py: Python<'_>, by: &Bound<'_, PyAny>, uses: &str, feather: f64, invert: bool) -> PyResult<Self> {
        let mask = if let Ok(layer) = by.extract::<PyRef<'_, Layer>>() {
            Mask::new(layer.0.clone())
        } else if let Ok(name) = by.extract::<&str>() {
            Mask::new(name)
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "by: a Layer or the name of one of this layer's maps is wanted",
            ));
        };
        wrap(py, masked(&self.0, mask, uses, feather, invert)).map(Self)
    }

    /// This layer called `name`.
    fn named(&self, py: Python<'_>, name: &str) -> PyResult<Self> {
        wrap(py, self.0.clone().named(name)).map(Self)
    }

    /// This layer at `value` of its own alpha, from 0 to 1.
    fn opacity(&self, py: Python<'_>, value: f64) -> PyResult<Self> {
        wrap(py, self.0.clone().opacity(value)).map(Self)
    }

    /// This layer joined to what is below by the blend mode named `mode`, a CSS name such as "multiply" or "soft-light".
    fn blend(&self, py: Python<'_>, mode: &str) -> PyResult<Self> {
        let mode: Blend = wrap(py, mode.parse())?;
        Ok(Self(self.0.clone().blend(mode)))
    }

    /// This layer printed in the inks `names`, one for each plane its last look separates (one for a layer with no look).
    #[expect(clippy::needless_pass_by_value, reason = "pyo3 hands the extracted list over")]
    fn inks(&self, py: Python<'_>, names: Vec<String>) -> PyResult<Self> {
        wrap(py, self.0.clone().inks(names.iter().map(String::as_str))).map(Self)
    }

    /// This layer made light (`on`), or not.
    fn as_light(&self, on: bool) -> Self {
        Self(self.0.clone().as_light(on))
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

    /// This layer with the corners of its fitted box put on `corners`: four (x, y) points, top left, top right, bottom right,
    /// bottom left.
    fn perspective(&self, py: Python<'_>, corners: [[f64; 2]; 4]) -> PyResult<Self> {
        wrap(py, self.0.clone().perspective(corners)).map(Self)
    }

    /// This layer with the image look written as the recipe document `json`, its `px` in the unit named `px`.
    fn look(&self, py: Python<'_>, json: &str, px: &str) -> PyResult<Self> {
        let px: Px = wrap(py, px.parse())?;
        wrap(py, self.0.clone().look_in(px, json)).map(Self)
    }

    /// This layer with the look of the recipe file `path`, read now, its `px` in the unit named `px`.
    fn look_file(&self, py: Python<'_>, path: PathBuf, px: &str) -> PyResult<Self> {
        let px: Px = wrap(py, px.parse())?;
        wrap(py, self.0.clone().look_file_in(px, path)).map(Self)
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

    /// Every parameter of the scene, in order, each as a dict of the fields of the parameter format.
    fn parameters<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        PyList::new(
            py,
            self.0
                .parameters()
                .iter()
                .map(|parameter| described(py, parameter))
                .collect::<PyResult<Vec<_>>>()?,
        )
    }

    /// This scene with the values of the dict (address to value) set; nothing is set unless every one is accepted.
    fn with_values(&self, py: Python<'_>, values: &Bound<'_, PyDict>) -> PyResult<Self> {
        let mut given = Values::new();
        for (address, value) in values {
            let address: String = address.extract()?;
            let value = value_of(py, &address, &value)?;
            given = given.set(address, value);
        }
        wrap(py, self.0.with_values(&given)).map(Self)
    }

    /// This scene as JSON text.
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        wrap(py, self.0.to_json())
    }

    /// The scene a JSON document describes; a relative path in it is relative to `base`.
    #[staticmethod]
    fn from_json(py: Python<'_>, text: &str, base: PathBuf) -> PyResult<Self> {
        wrap(
            py,
            bullterrier_scene::Scene::from_json_with(
                text,
                &bullterrier_scene::Files::folder(base),
                &kinds::current(py)?,
            ),
        )
        .map(Self)
    }

    /// The picture as a new float32 array (height, width, 4): straight alpha, 0 to 1, rows from the top.
    fn render<'py>(&self, py: Python<'py>, size: i128) -> PyResult<Bound<'py, PyArray3<f32>>> {
        let raster = wrap(py, py.detach(|| self.0.render(size)))?;
        let (height, width) = (raster.height(), raster.width());
        PyArray1::from_vec(py, raster.into_pixels()).reshape([height, width, 4])
    }

    /// The picture as `render` gives it, composed on `gpu` and read back.
    #[cfg(feature = "gpu")]
    fn render_on<'py>(&self, py: Python<'py>, size: i128, gpu: &gpu::Gpu) -> PyResult<Bound<'py, PyArray3<f32>>> {
        gpu::render(py, &self.0, size, gpu)
    }

    /// The plates at `size`: one plane for each ink the layers name. `underbase` is (garment, choke, tolerance, feather, name) for
    /// a white plate first, or `None`.
    fn plates(
        &self,
        py: Python<'_>,
        size: i128,
        underbase: Option<(String, f64, f64, f64, String)>,
    ) -> PyResult<Plates> {
        let made = match underbase {
            None => py.detach(|| self.0.plates(size)),
            Some(arguments) => {
                let underbase = underbase_of(py, arguments)?;
                py.detach(|| self.0.plates_with_underbase(size, &underbase))
            }
        };
        wrap(py, made).map(Plates)
    }

    /// The picture a band of rows at a time: `sink(first_row, band)` is called with each band's first row and its float32 array
    /// (rows, width, 4), top to bottom. An exception of `sink` ends the render and is raised as it is.
    fn render_bands(&self, py: Python<'_>, size: i128, band_rows: usize, sink: &Bound<'_, PyAny>) -> PyResult<()> {
        let sink = sink.clone().unbind();
        let raised = std::sync::Mutex::new(None);
        let outcome = py.detach(|| {
            self.0.render_bands(size, band_rows, |rows, band| {
                // the GIL is taken for this call only: the render around it runs without it
                Python::attach(|py| {
                    let array = PyArray1::from_slice(py, band.pixels()).reshape([band.height(), band.width(), 4]);
                    array
                        .and_then(|array| sink.bind(py).call1((rows.start, array)))
                        .map(drop)
                        .map_err(|failure| {
                            *raised.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(failure);
                            bullterrier_scene::Error::Write {
                                path: "sink".to_owned(),
                                message: "the sink raised".to_owned(),
                            }
                        })
                })
            })
        });
        match raised.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner) {
            Some(failure) => Err(failure),
            None => wrap(py, outcome),
        }
    }
}

#[pymethods]
impl Plates {
    /// The inks, in plate order.
    #[getter]
    fn names(&self) -> Vec<String> {
        self.0.names().into_iter().map(str::to_owned).collect()
    }

    /// The number of columns.
    #[getter]
    fn width(&self) -> usize {
        self.0.width()
    }

    /// The number of rows.
    #[getter]
    fn height(&self) -> usize {
        self.0.height()
    }

    /// How much of the ink `name` lies on each pixel, as a new float32 array (height, width), 0 to 1.
    fn plane<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyArray2<f32>>> {
        let plane = self.0.plane(name).ok_or_else(|| {
            error::invalid(
                py,
                format!("name: no plate {name:?}; the plates are {}", self.0.names().join(", ")),
            )
        })?;
        PyArray1::from_slice(py, plane).reshape([self.0.height(), self.0.width()])
    }

    /// Writes each plate as a grey PNG, `<stem>-<ink>.png`, and returns the paths.
    #[expect(clippy::needless_pass_by_value, reason = "pyo3 extracts a path by value")]
    fn write_png(
        &self,
        py: Python<'_>,
        stem: PathBuf,
        options: (f64, f64, String, bool, bool),
    ) -> PyResult<Vec<PathBuf>> {
        let options = page_options(py, options)?;
        wrap(py, py.detach(|| self.0.write_png(&stem, &options)))
    }

    /// Writes the plates as one PDF with a page for each, named by its ink.
    #[expect(clippy::needless_pass_by_value, reason = "pyo3 extracts a path by value")]
    fn write_pdf(&self, py: Python<'_>, path: PathBuf, options: (f64, f64, String, bool, bool)) -> PyResult<()> {
        let options = page_options(py, options)?;
        wrap(py, py.detach(|| self.0.write_pdf(&path, &options)))
    }
}
