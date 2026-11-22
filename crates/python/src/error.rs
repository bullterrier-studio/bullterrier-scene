//! The core's errors raised as the package's exceptions of `bullterrier_scene.errors`.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use bullterrier_scene::Error;

/// The Python exception for `error`: the class of `bullterrier_scene.errors` that stands for its variant, with the core's
/// message. The module is imported when the first error is raised: importing the extension imports nothing of the package.
pub(crate) fn raised(py: Python<'_>, error: &Error) -> PyErr {
    let name = match error {
        Error::Invalid { .. } => "InvalidError",
        Error::Format { .. } => "FormatError",
        Error::Read { .. } => "ReadError",
        Error::Write { .. } => "WriteError",
        Error::Gpu { .. } => "GpuError",
        // the enum is non-exhaustive: a variant added to the core without a class here is a bug to see
        _ => return PyRuntimeError::new_err(error.to_string()),
    };
    instance(py, name, error.to_string()).unwrap_or_else(|failure| failure)
}

/// An `InvalidError` for a value the core never sees: an array whose shape is not a picture's.
pub(crate) fn invalid(py: Python<'_>, message: String) -> PyErr {
    instance(py, "InvalidError", message).unwrap_or_else(|failure| failure)
}

/// `bullterrier_scene.errors.<name>(message)` as an error to raise.
fn instance(py: Python<'_>, name: &str, message: String) -> PyResult<PyErr> {
    let class = py.import("bullterrier_scene.errors")?.getattr(name)?;
    Ok(PyErr::from_value(class.call1((message,))?))
}
