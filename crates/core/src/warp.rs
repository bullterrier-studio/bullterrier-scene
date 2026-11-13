//! The warp: a raster put on the output by an affine map, the one resample of a layer.
//!
//! Backward mapping: each output pixel looks up where its centre falls in the raster. The kernels are convex (bilinear and
//! area average, no negative weight), so no value leaves the range of the values it mixes and the render needs no clamp.
//! Colour is mixed premultiplied, in `f32`; positions are `f64`.
//!
//! * **Pixel centres.** Raster pixel `(i, j)` spans `[i, i + 1] x [j, j + 1]` and its value sits at its middle; an output
//!   pixel is likewise a unit square and is sampled at its middle.
//! * **Exact where it can be.** An output pixel whose centre falls on the middle of a raster pixel copies that pixel, colour
//!   and alpha untouched: a move by whole pixels, a turn by a multiple of 90 degrees and a flip, with the pivot on the pixel
//!   grid, are permutations of the raster's pixels bit for bit.
//! * **Between pixels.** Bilinear: a half pixel is the average of two neighbours. Colour comes from the clamped edge, so a
//!   raster stretched or turned does not fade at its border.
//! * **Edges.** The alpha of an output pixel is the raster's times its coverage by the clip rectangle: the length of the
//!   pixel's extent (one pixel across the edge) that lies inside it, so an edge between two pixels is soft in proportion
//!   and one on the pixel grid is hard. Outside the clip nothing is drawn.
//! * **Reduction.** A raster axis the map shrinks (the norm of its column of the map is below 1) is first averaged by area
//!   to the pixels it will cover, so a reduction does not alias and no threshold decides it.

use std::borrow::Cow;

use crate::Raster;
use crate::geometry::{Rect, whole_up};

/// A map from raster pixels to output pixels: `x' = a x + b y + c` and `y' = d x + e y + f`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Affine {
    pub(crate) a: f64,
    pub(crate) b: f64,
    pub(crate) c: f64,
    pub(crate) d: f64,
    pub(crate) e: f64,
    pub(crate) f: f64,
}

impl Affine {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 0.0,
        e: 1.0,
        f: 0.0,
    };

    /// The point `(x, y)` mapped.
    fn at(self, x: f64, y: f64) -> (f64, f64) {
        (self.a * x + self.b * y + self.c, self.d * x + self.e * y + self.f)
    }

    /// The map back. Exact when the determinant is 1 and the entries are small whole or half numbers: a division by 1.
    fn inverse(self) -> Self {
        let determinant = self.a * self.e - self.b * self.d;
        Self {
            a: self.e / determinant,
            b: -self.b / determinant,
            c: (self.b * self.f - self.c * self.e) / determinant,
            d: -self.d / determinant,
            e: self.a / determinant,
            f: (self.c * self.d - self.a * self.f) / determinant,
        }
    }
}

/// The part of `source` inside `clip` (a rectangle in raster pixels) put on an output of `columns` by `rows` pixels by
/// `map`; transparent where it does not reach. The map must be invertible and its entries finite.
pub(crate) fn warp(source: &Raster, map: Affine, clip: Rect, columns: usize, rows: usize) -> Raster {
    let whole = Rect {
        x: 0.0,
        y: 0.0,
        width: lossless(source.width()),
        height: lossless(source.height()),
    };
    if map == Affine::IDENTITY && clip == whole && (source.width(), source.height()) == (columns, rows) {
        return source.clone();
    }
    let (source, map, clip) = prefiltered(source, map, clip);
    let (width, height) = (source.width(), source.height());
    let pixels = source.pixels();
    let inverse = map.inverse();
    // raster pixels per output pixel along the gradient of each raster axis: the extent of an output pixel across an edge
    let (across_x, across_y) = (inverse.a.hypot(inverse.b), inverse.d.hypot(inverse.e));
    let (low_x, high_x) = (clip.x, clip.x + clip.width);
    let (low_y, high_y) = (clip.y, clip.y + clip.height);
    let (first_row, last_row, first_column, last_column) = reach(map, clip, columns, rows);

    let mut output = vec![0.0_f32; columns * rows * 4];
    for row in first_row..last_row {
        let centre_y = lossless(row) + 0.5;
        for column in first_column..last_column {
            let centre_x = lossless(column) + 0.5;
            let (x, y) = inverse.at(centre_x, centre_y);
            let cover = share((x - low_x) / across_x, (high_x - x) / across_x)
                * share((y - low_y) / across_y, (high_y - y) / across_y);
            if cover <= 0.0 {
                continue;
            }
            let (first, second, toward_second) = taps(x - 0.5, width);
            let (above, below, toward_below) = taps(y - 0.5, height);
            let at = (row * columns + column) * 4;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a share from 0 to 1 is held as a 32-bit float"
            )]
            let cover = cover as f32;
            output[at..at + 4].copy_from_slice(&sample(
                pixels,
                width,
                [(first, second, toward_second), (above, below, toward_below)],
                cover,
            ));
        }
    }
    Raster::made(columns, rows, output)
}

/// A count of pixels as a float: at most 2^27, so exact.
#[expect(clippy::cast_precision_loss, reason = "a count of at most 2^27")]
fn lossless(count: usize) -> f64 {
    count as f64
}

/// How much of a pixel one unit wide, whose centre is `inside` units from an edge (negative if it lies outside), is on the
/// inner side of it: 0 to 1.
fn edge(inside: f64) -> f64 {
    (inside + 0.5).clamp(0.0, 1.0)
}

/// How much of a pixel is between two opposite edges, `low` and `high` units from its centre (each positive inside): the
/// overlap of its extent with the interval, exact for a pixel across axis-aligned edges.
fn share(low: f64, high: f64) -> f64 {
    (edge(low) + edge(high) - 1.0).max(0.0)
}

/// The two raster pixels either side of position `at` (in pixel centres: `0` is the middle of the first pixel) along a line
/// of `count`, clamped to the line, and how far toward the second the position is.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a position clamped into the line, and a weight from 0 to 1 held as a 32-bit float"
)]
fn taps(at: f64, count: usize) -> (usize, usize, f32) {
    let first = at.floor();
    let last = lossless(count - 1);
    let toward_second = (at - first) as f32;
    (
        first.clamp(0.0, last) as usize,
        (first + 1.0).clamp(0.0, last) as usize,
        toward_second,
    )
}

/// The pixel the taps `[(first, second, toward_second) per axis]` and the `cover` make: a pixel at a single tap is copied,
/// the alpha scaled by `cover`; otherwise the four are mixed premultiplied.
fn sample(pixels: &[f32], width: usize, [x, y]: [(usize, usize, f32); 2], cover: f32) -> [f32; 4] {
    let at = |row: usize, column: usize| -> [f32; 4] {
        let mut pixel = [0.0; 4];
        pixel.copy_from_slice(&pixels[(row * width + column) * 4..][..4]);
        pixel
    };
    // a weight of exactly 0 is the exact case: the output pixel's centre is the middle of a raster pixel
    let single = |(first, second, toward_second): (usize, usize, f32)| first == second || toward_second == 0.0;
    if single(x) && single(y) {
        let [red, green, blue, alpha] = at(y.0, x.0);
        return [red, green, blue, alpha * cover];
    }
    // colour and alpha are summed in the same order with a colour never above its alpha, and rounding is monotone, so the
    // mixed colour is never above the mixed alpha and the quotient is at most 1; weights that sum a step over 1 are met
    // by the minimum below
    let mut mixed = [0.0_f32; 4];
    for (row, weight_row) in [(y.0, 1.0 - y.2), (y.1, y.2)] {
        for (column, weight_column) in [(x.0, 1.0 - x.2), (x.1, x.2)] {
            let weight = weight_row * weight_column;
            let [red, green, blue, alpha] = at(row, column);
            mixed[0] += weight * (red * alpha);
            mixed[1] += weight * (green * alpha);
            mixed[2] += weight * (blue * alpha);
            mixed[3] += weight * alpha;
        }
    }
    let alpha = mixed[3];
    let colour = |value: f32| if alpha > 0.0 { value / alpha } else { 0.0 };
    [
        colour(mixed[0]),
        colour(mixed[1]),
        colour(mixed[2]),
        alpha.min(1.0) * cover,
    ]
}

/// The rows and columns of the output that the clip can reach, as `(first row, end row, first column, end column)`: the
/// bounding box of its corners under `map`, a pixel wider, within the output.
fn reach(map: Affine, clip: Rect, columns: usize, rows: usize) -> (usize, usize, usize, usize) {
    let corners = [
        (clip.x, clip.y),
        (clip.x + clip.width, clip.y),
        (clip.x, clip.y + clip.height),
        (clip.x + clip.width, clip.y + clip.height),
    ]
    .map(|(x, y)| map.at(x, y));
    let range = |pick: fn((f64, f64)) -> f64, count: usize| {
        let (low, high) = corners
            .iter()
            .map(|&corner| pick(corner))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), value| {
                (low.min(value), high.max(value))
            });
        // clamped into the output before the cast, so a layer far outside it, or far larger, reaches nothing or all
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped into 0 to the count of pixels first"
        )]
        {
            let count = lossless(count);
            (
                (low - 1.0).floor().clamp(0.0, count) as usize,
                (high + 1.0).ceil().clamp(0.0, count) as usize,
            )
        }
    };
    let (first_column, last_column) = range(|corner| corner.0, columns);
    let (first_row, last_row) = range(|corner| corner.1, rows);
    (first_row, last_row, first_column, last_column)
}

/// `source`, `map` and `clip` with each raster axis the map shrinks (the norm of its column of the map below 1) averaged by
/// area down to the pixels it covers; unchanged where the map does not shrink it.
fn prefiltered(source: &Raster, map: Affine, clip: Rect) -> (Cow<'_, Raster>, Affine, Rect) {
    let (width, height) = (source.width(), source.height());
    let kept = |count: usize, scale: f64| -> usize {
        if scale < 1.0 {
            whole_up(lossless(count) * scale).clamp(1, count)
        } else {
            count
        }
    };
    let (columns, rows) = (kept(width, map.a.hypot(map.d)), kept(height, map.b.hypot(map.e)));
    if (columns, rows) == (width, height) {
        return (Cow::Borrowed(source), map, clip);
    }
    // a pixel of the averaged raster is this many pixels of the source across
    let (step_x, step_y) = (lossless(width) / lossless(columns), lossless(height) / lossless(rows));
    let map = Affine {
        a: map.a * step_x,
        b: map.b * step_y,
        c: map.c,
        d: map.d * step_x,
        e: map.e * step_y,
        f: map.f,
    };
    let clip = Rect {
        x: clip.x / step_x,
        y: clip.y / step_y,
        width: clip.width / step_x,
        height: clip.height / step_y,
    };
    (Cow::Owned(reduce(source, columns, rows)), map, clip)
}

/// The source pixels, and their weights, that make output pixel `j` of `count` on a line of `from` pixels: each source pixel
/// weighs the part of the output pixel's extent it covers.
struct Span {
    first: usize,
    weights: Vec<f32>,
}

fn spans(from: usize, count: usize) -> Vec<Span> {
    let step = lossless(from) / lossless(count);
    (0..count)
        .map(|index| {
            let (start, end) = (lossless(index) * step, lossless(index + 1) * step);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "positions within a line of at most 2^27 pixels"
            )]
            let (first, last) = (start.floor() as usize, (end.ceil() as usize).min(from));
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a weight from 0 to 1 is held as a 32-bit float"
            )]
            let weights = (first..last)
                .map(|pixel| ((end.min(lossless(pixel + 1)) - start.max(lossless(pixel))) / step) as f32)
                .collect();
            Span { first, weights }
        })
        .collect()
}

/// `source` averaged by area to `columns` by `rows` pixels (each at most the source's): the premultiplied mean of the pixels
/// each covers, then straight again.
fn reduce(source: &Raster, columns: usize, rows: usize) -> Raster {
    let width = source.width();
    let (across, down) = (spans(width, columns), spans(source.height(), rows));
    // first along each row: height x columns, premultiplied
    let mut along = vec![0.0_f32; source.height() * columns * 4];
    for (line, from) in along
        .chunks_exact_mut(columns * 4)
        .zip(source.pixels().chunks_exact(width * 4))
    {
        for (span, out) in across.iter().zip(line.as_chunks_mut::<4>().0) {
            for (weight, pixel) in span.weights.iter().zip(from[span.first * 4..].as_chunks::<4>().0) {
                out[0] += weight * (pixel[0] * pixel[3]);
                out[1] += weight * (pixel[1] * pixel[3]);
                out[2] += weight * (pixel[2] * pixel[3]);
                out[3] += weight * pixel[3];
            }
        }
    }
    // then down each column, and straight
    let mut pixels = vec![0.0_f32; columns * rows * 4];
    for (span, line) in down.iter().zip(pixels.chunks_exact_mut(columns * 4)) {
        for (weight, from) in span
            .weights
            .iter()
            .zip(along[span.first * columns * 4..].chunks_exact(columns * 4))
        {
            for (out, value) in line.iter_mut().zip(from) {
                *out += weight * value;
            }
        }
        for pixel in line.as_chunks_mut::<4>().0 {
            let alpha = pixel[3];
            for value in &mut pixel[..3] {
                *value = if alpha > 0.0 { *value / alpha } else { 0.0 };
            }
            pixel[3] = alpha.min(1.0);
        }
    }
    Raster::made(columns, rows, pixels)
}
