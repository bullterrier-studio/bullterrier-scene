//! Helpers the test modules share.

use std::fs;
use std::path::{Path, PathBuf};

use bullterrier_scene::{Error, Layer, Raster, Scene};

/// Every `.rs` file under `dir`, sorted.
pub(crate) fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).unwrap_or_else(|error| unreachable!("{}: {error}", dir.display())) {
        let path = entry.unwrap_or_else(|error| unreachable!("{error}")).path();
        if path.is_dir() {
            found.extend(rust_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// What a test returns so that `?` ends it with the error shown.
pub(crate) type Outcome = Result<(), Box<dyn std::error::Error>>;

/// A scene of `width` by `height` design pixels with `layers` on it, bottom first.
pub(crate) fn scene(width: u32, height: u32, layers: impl IntoIterator<Item = Layer>) -> Result<Scene, Error> {
    Scene::new(width, height)?.add(layers)
}

/// Every pixel of `raster`, row after row.
pub(crate) fn pixels(raster: &Raster) -> impl Iterator<Item = [f32; 4]> + '_ {
    raster.pixels().as_chunks::<4>().0.iter().copied()
}

/// Whether `got` is `expected` bit for bit, in every channel (a float compared exactly, on purpose).
pub(crate) fn exactly(got: [f32; 4], expected: [f32; 4]) -> bool {
    got.map(f32::to_bits) == expected.map(f32::to_bits)
}

/// Whether every pixel of `raster` is `expected`, bit for bit.
pub(crate) fn all_exactly(raster: &Raster, expected: [f32; 4]) -> bool {
    pixels(raster).all(|pixel| exactly(pixel, expected))
}

/// Whether `got` is `expected` in every channel, to within `tolerance`.
pub(crate) fn close(got: [f32; 4], expected: [f32; 4], tolerance: f32) -> bool {
    got.iter()
        .zip(expected)
        .all(|(got, expected)| (got - expected).abs() <= tolerance)
}
