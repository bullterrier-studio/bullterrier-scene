//! `bullterrier_scene._native._core`: gives the Rust core to Python. Generic: it holds no logic of the core, only the mapping of
//! its calls and errors.

use pyo3::prelude::*;

/// bullterrier-scene's Rust core.
#[pymodule]
mod _core {}
