//! Layer kinds by name, for Python: the one process-wide registry (what plugins add), the kind that calls a Python function,
//! and the functions the `bullterrier_scene.plugin` module builds the decorator and the loader on.

use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value as Json;

use bullterrier_scene::{Error, LayerKind, LayerKindFactory, LayerKinds, MAX_PIXELS, Raster};

use super::{error, raster_of};

/// The core's refusal of a value, as the core writes it.
fn invalid(address: &str, message: String) -> Error {
    Error::Invalid {
        address: address.to_owned(),
        message,
    }
}

fn locked() -> MutexGuard<'static, LayerKinds> {
    static REGISTRY: OnceLock<Mutex<LayerKinds>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| Mutex::new(LayerKinds::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// The registry as it is now, after the plugins of the entry-point group are loaded (the first call loads them: the
/// package's `plugin._load` keeps the state). A copy, so that no lock is held while a kind draws.
pub(crate) fn current(py: Python<'_>) -> PyResult<LayerKinds> {
    py.import("bullterrier_scene.plugin")?.call_method0("_load")?;
    Ok(locked().clone())
}

/// A Python function registered as a kind.
struct PythonFactory {
    name: String,
    identity: String,
    function: Py<PyAny>,
    by_position: bool,
}

impl LayerKindFactory for PythonFactory {
    fn name(&self) -> &str {
        &self.name
    }

    fn identity(&self) -> &str {
        &self.identity
    }

    fn create(&self, options: &Json) -> Result<Arc<dyn LayerKind>, Error> {
        if !matches!(options, Json::Object(_) | Json::Null) {
            return Err(invalid(
                "options",
                format!("the options of a Python kind are an object (they are its keyword arguments), got {options}"),
            ));
        }
        Ok(Arc::new(PythonKind {
            name: self.name.clone(),
            options: options.clone(),
            function: Python::attach(|py| self.function.clone_ref(py)),
            by_position: self.by_position,
        }))
    }
}

/// A Python function with its options.
struct PythonKind {
    name: String,
    options: Json,
    function: Py<PyAny>,
    by_position: bool,
}

impl LayerKind for PythonKind {
    fn name(&self) -> &str {
        &self.name
    }

    fn options(&self) -> Json {
        self.options.clone()
    }

    fn by_position(&self) -> bool {
        self.by_position
    }

    fn draw(&self, area: [f64; 4], scale: [f64; 2]) -> Result<Raster, Error> {
        let fail = |what: String| invalid("make", format!("the kind {:?} {what}", self.name));
        // the GIL is taken for this call only: the render around it runs without it, on its own threads
        Python::attach(|py| {
            let keywords = if self.options.is_null() {
                PyDict::new(py)
            } else {
                py.import("json")
                    .and_then(|json| json.call_method1("loads", (self.options.to_string(),)))
                    .and_then(|found| Ok(found.cast_into::<PyDict>()?))
                    .map_err(|failure| fail(format!("has options that are no keyword arguments: {failure}")))?
            };
            let arguments = (area[0], area[1], area[2], area[3], scale[0], scale[1]);
            let array = self
                .function
                .bind(py)
                .call(arguments, Some(&keywords))
                .map_err(|raised| {
                    fail(format!(
                        "raised {}: {raised}",
                        raised
                            .get_type(py)
                            .name()
                            .map_or_else(|_| "an exception".to_owned(), |name| name.to_string())
                    ))
                })?;
            // the size is read before the pixels are: an array of a billion pixels is refused, not copied
            let shape: Vec<usize> = array
                .getattr("shape")
                .and_then(|shape| shape.extract())
                .unwrap_or_default();
            if let [rows, columns, ..] = shape[..]
                && rows.saturating_mul(columns) > MAX_PIXELS
            {
                return Err(fail(format!(
                    "drew {rows} x {columns} pixels; at most {MAX_PIXELS} pixels (2^27) are drawn"
                )));
            }
            raster_of(py, &array).map_err(|failure| fail(format!("drew an array the scene refuses: {failure}")))
        })
    }
}

/// Registers `function` as the kind `name`. `identity` says which function it is (a module loaded again replaces itself);
/// `by_position` is `LayerKind::by_position`. Refusals as `LayerKinds::register`.
#[pyfunction]
pub(crate) fn register_layer(
    py: Python<'_>,
    function: Py<PyAny>,
    name: String,
    identity: String,
    by_position: bool,
) -> PyResult<()> {
    let factory = PythonFactory {
        name,
        identity,
        function,
        by_position,
    };
    locked()
        .register(Arc::new(factory))
        .map_err(|failure| error::raised(py, &failure))
}

/// The names of every registered kind, sorted.
#[pyfunction]
pub(crate) fn layer_kinds(py: Python<'_>) -> PyResult<Vec<String>> {
    Ok(current(py)?.names().into_iter().map(str::to_owned).collect())
}

/// The registry at one moment, to put back with `registry_restore`.
#[pyclass(frozen, module = "bullterrier_scene._native._core")]
pub(crate) struct Snapshot(LayerKinds);

/// The registry as it is now.
#[pyfunction]
pub(crate) fn registry_snapshot() -> Snapshot {
    Snapshot(locked().clone())
}

/// Puts back `snapshot`, or an empty registry for `None`.
#[pyfunction]
pub(crate) fn registry_restore(snapshot: Option<PyRef<'_, Snapshot>>) {
    *locked() = snapshot.map_or_else(LayerKinds::new, |kept| kept.0.clone());
}
