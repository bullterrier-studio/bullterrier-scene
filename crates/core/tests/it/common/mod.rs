//! Helpers the test modules share.

pub(crate) mod counting;

// the one tolerance and its compare, shared with the command line's tests (`crates/cli/tests/it/common`)
mod looks;
pub(crate) use looks::levels_look_the_same;

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

/// A float colour value as the level 0 to 255 an 8-bit file keeps (`value * 255 + 0.5` cut), a value outside 0 to 1 cut to it.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0 to 255 first"
)]
fn level(value: f32) -> u8 {
    (value * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// `Ok` when float samples `got` look the same as `expected` ([`levels_look_the_same`] after rounding to 8 bits); a value
/// that is not a number is a difference.
pub(crate) fn looks_the_same(got: &[f32], expected: &[f32]) -> Result<(), String> {
    if let Some(at) = got.iter().chain(expected).position(|value| !value.is_finite()) {
        return Err(format!("not a number at {at}"));
    }
    levels_look_the_same(
        &got.iter().copied().map(level).collect::<Vec<_>>(),
        &expected.iter().copied().map(level).collect::<Vec<_>>(),
    )
}

/// Whether two pictures look the same ([`looks_the_same`]).
pub(crate) fn same_look(got: &Raster, expected: &Raster) -> bool {
    looks_the_same(got.pixels(), expected.pixels()).is_ok()
}

/// A `width` by `height` picture of one colour.
pub(crate) fn solid(width: usize, height: usize, rgba: [f32; 4]) -> Result<Raster, Error> {
    Raster::rgba(width, height, rgba.repeat(width * height))
}

/// The red, green, blue and alpha of the pixel at column `x`, row `y`.
pub(crate) fn pixel(raster: &Raster, x: usize, y: usize) -> [f32; 4] {
    let at = (y * raster.width() + x) * 4;
    let mut found = [0.0; 4];
    found.copy_from_slice(&raster.pixels()[at..at + 4]);
    found
}

/// A picture whose every pixel differs, opaque, with values that are whole multiples of 1/255 (so that a file keeps them).
pub(crate) fn varied(width: usize, height: usize) -> Result<Raster, Error> {
    let levels = (0..width * height).flat_map(|index| {
        let level = |step: usize| f32::from(u8::try_from(index * step % 256).unwrap_or(0)) / 255.0;
        [level(37), level(91), level(151), 1.0]
    });
    Raster::rgba(width, height, levels.collect())
}

/// A drawing document, in the form `Drawing.to_json()` writes: a frame of `frame` by `frame` design pixels holding one
/// rectangle of `color`, from (`x`, `y`) to (`right`, `bottom`).
pub(crate) fn rectangle(frame: (u32, u32), at: (f64, f64), to: (f64, f64), color: &str) -> String {
    let ((x, y), (right, bottom), (width, height)) = (at, to, frame);
    format!(
        r#"{{"root":[{{"stack":{{"items":[[{{"path":"M{x} {y} L{right} {y} L{right} {bottom} L{x} {bottom} Z"}}]],"size":[{width},{height}]}}}},{{"fill":{{"color":"{color}"}}}}],"version":1}}"#
    )
}

/// A picture whose every pixel differs, translucent in part, every value a multiple of 1/16 so that it is exact in a float
/// (at most 12 columns and rows).
pub(crate) fn numbered(width: usize, height: usize) -> Result<Raster, Error> {
    let values = (0..height).flat_map(|y| {
        (0..width).flat_map(move |x| {
            let level = |number: usize| f32::from(u8::try_from(number).unwrap_or(0)) / 16.0;
            [
                level(1 + x),
                level(1 + y),
                level((x * 3 + y * 5) % 8 + 1),
                level(4 + 4 * ((x + 2 * y) % 3) + 4) / 2.0,
            ]
        })
    });
    Raster::rgba(width, height, values.collect())
}

/// `layer` alone on a canvas of `width` by `height` design pixels at `size`.
pub(crate) fn render(layer: Layer, (width, height): (u32, u32), size: u32) -> Result<Raster, Error> {
    scene(width, height, [layer])?.render(size)
}

/// A raster of `width` by `height` pixels whose pixel (`x`, `y`) is `pixel_of(x, y)`.
pub(crate) fn made(width: usize, height: usize, pixel_of: impl Fn(usize, usize) -> [f32; 4]) -> Result<Raster, Error> {
    Raster::rgba(
        width,
        height,
        (0..height)
            .flat_map(|y| {
                (0..width).flat_map({
                    let pixel_of = &pixel_of;
                    move |x| pixel_of(x, y)
                })
            })
            .collect::<Vec<_>>(),
    )
}

/// The pixels per metre a PNG's `pHYs` chunk says, read from the bytes by hand: a chunk is a 4-byte length, a 4-byte type, the
/// data and a 4-byte checksum, and `pHYs` holds two 4-byte counts and a unit byte (1 is the metre).
pub(crate) fn physical(path: &Path) -> Result<Option<(u32, u32, u8)>, std::io::Error> {
    let bytes = std::fs::read(path)?;
    let mut at = 8;
    while at + 8 <= bytes.len() {
        let length = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize;
        let data = at + 8;
        if &bytes[at + 4..at + 8] == b"pHYs" {
            let count =
                |from: usize| u32::from_be_bytes([bytes[from], bytes[from + 1], bytes[from + 2], bytes[from + 3]]);
            return Ok(Some((count(data), count(data + 4), bytes[data + 8])));
        }
        at = data + length + 4;
    }
    Ok(None)
}
