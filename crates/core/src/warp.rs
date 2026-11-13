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
//! * **Reduction, two classes of map.** Where the map keeps the axes of the raster (a move, a scale, a flip, `aspect`, a
//!   quarter turn, each column of the map having one entry that is not 0) the footprint of an output pixel is a rectangle
//!   along the raster's axes, and each axis the map shrinks is first averaged by area to the pixels it will cover: the exact
//!   area average, with no threshold. Every other map (a turn by another angle, a skew, a perspective) has a footprint that
//!   is a parallelogram of any direction: see `footprint`, whose rule replaces the area average there.

mod footprint;

use std::borrow::Cow;
use std::sync::Arc;

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

/// Whether `map` and `clip` leave `source` exactly as it is on an output of `columns` by `rows` pixels.
pub(crate) fn is_identity(source: &Raster, map: Affine, clip: Rect, columns: usize, rows: usize) -> bool {
    let whole = Rect {
        x: 0.0,
        y: 0.0,
        width: lossless(source.width()),
        height: lossless(source.height()),
    };
    map == Affine::IDENTITY && clip == whole && (source.width(), source.height()) == (columns, rows)
}

/// The part of `source` inside `clip` (a rectangle in raster pixels) put on an output of `columns` by `rows` pixels by
/// `map`; transparent where it does not reach. The map must be invertible and its entries finite. A map that changes
/// nothing hands the source back, not a copy of it.
pub(crate) fn warp(source: &Arc<Raster>, map: Affine, clip: Rect, columns: usize, rows: usize) -> Arc<Raster> {
    if is_identity(source, map, clip, columns, rows) {
        return Arc::clone(source);
    }
    Arc::new(warp_by(source, map, clip, columns, rows, keeps_axes(map)))
}

/// The part `window` (design pixels of the picture, one to a pixel) of `source` as a raster of `columns` by `rows` pixels that
/// divide it evenly: the rule of `geometry` for a rectangle drawn at a scale, made by the warp.
pub(crate) fn resize(source: &Arc<Raster>, window: Rect, (columns, rows): (usize, usize)) -> Arc<Raster> {
    // a side of at most 2^27 pixels
    #[expect(clippy::cast_precision_loss, reason = "a side of at most 2^27 pixels")]
    let (to_columns, to_rows) = (columns as f64 / window.width, rows as f64 / window.height);
    let map = Affine {
        a: to_columns,
        b: 0.0,
        c: -window.x * to_columns,
        d: 0.0,
        e: to_rows,
        f: -window.y * to_rows,
    };
    warp(source, map, window, columns, rows)
}

/// Whether `map` keeps the axes of the raster: a move, a scale, a flip or a quarter turn about x and y alone.
fn keeps_axes(map: Affine) -> bool {
    map.b == 0.0 && map.d == 0.0
}

/// [`warp`] as a new raster, by the general kernel (`aligned` false) or by the one for a map that keeps the axes (`aligned`
/// true).
fn warp_by(source: &Raster, map: Affine, clip: Rect, columns: usize, rows: usize, aligned: bool) -> Raster {
    let mut output = vec![0.0_f32; columns * rows * 4];
    warp_through(source, map, clip, (columns, rows), aligned, &mut |at, pixel| {
        output[at * 4..at * 4 + 4].copy_from_slice(&pixel);
    });
    Raster::made(columns, rows, output)
}

/// [`warp`] giving each output pixel the picture reaches to `put(index, pixel)` (`index` is `row * columns + column`; a
/// pixel it does not reach is not given: it is transparent). Both kernels give the same pixels, bit for bit; the one for a
/// map that keeps the axes finds each column's and row's taps and coverage once instead of for every pixel, and handing
/// the pixels over lets a caller blend them where they land instead of making a raster of the output first.
pub(crate) fn warp_into(
    source: &Raster,
    map: Affine,
    clip: Rect,
    (columns, rows): (usize, usize),
    put: &mut impl FnMut(usize, [f32; 4]),
) {
    warp_through(source, map, clip, (columns, rows), keeps_axes(map), put);
}

/// Whether the footprint of an output pixel under `map` is a rectangle along the raster's axes: each column of the map has
/// one entry that is not 0.
fn is_exact_class(map: Affine) -> bool {
    (map.a != 0.0) != (map.d != 0.0) && (map.b != 0.0) != (map.e != 0.0)
}

fn warp_through(
    source: &Raster,
    map: Affine,
    clip: Rect,
    (columns, rows): (usize, usize),
    aligned: bool,
    put: &mut impl FnMut(usize, [f32; 4]),
) {
    if is_exact_class(map) {
        exact_through(source, map, clip, (columns, rows), aligned, put);
    } else {
        footprint::through(source, map, clip, (columns, rows), put);
    }
}

/// [`warp_through`] for a map whose footprint is a rectangle along the raster's axes: area average by the column norms, then
/// bilinear.
fn exact_through(
    source: &Raster,
    map: Affine,
    clip: Rect,
    (columns, rows): (usize, usize),
    aligned: bool,
    put: &mut impl FnMut(usize, [f32; 4]),
) {
    let (source, map, clip) = prefiltered(source, map, clip);
    let (width, height) = (source.width(), source.height());
    let pixels = source.pixels();
    let inverse = map.inverse();
    // raster pixels per output pixel along the gradient of each raster axis: the extent of an output pixel across an edge
    let (across_x, across_y) = (inverse.a.hypot(inverse.b), inverse.d.hypot(inverse.e));
    let (low_x, high_x) = (clip.x, clip.x + clip.width);
    let (low_y, high_y) = (clip.y, clip.y + clip.height);
    let (first_row, last_row, first_column, last_column) = reach(map, clip, columns, rows);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a share from 0 to 1 is held as a 32-bit float"
    )]
    let single = |cover: f64| cover as f32;

    if aligned {
        // each column's position and each row's depend on that column or row alone
        let lines =
            |range: std::ops::Range<usize>, scale: f64, shift: f64, across: f64, low: f64, high: f64, count: usize| {
                range
                    .map(|index| {
                        let at = scale * (lossless(index) + 0.5) + shift;
                        (taps(at - 0.5, count), share((at - low) / across, (high - at) / across))
                    })
                    .collect::<Vec<_>>()
            };
        let across = lines(
            first_column..last_column,
            inverse.a,
            inverse.c,
            across_x,
            low_x,
            high_x,
            width,
        );
        let down = lines(
            first_row..last_row,
            inverse.e,
            inverse.f,
            across_y,
            low_y,
            high_y,
            height,
        );
        // a one-pixel raster (a flat colour) needs no taps: every pixel is that one with its alpha scaled by the cover
        let flat = (width == 1 && height == 1).then(|| [pixels[0], pixels[1], pixels[2], pixels[3]]);
        for (row, (y_taps, share_y)) in (first_row..last_row).zip(down) {
            if !is_drawn(share_y) {
                continue;
            }
            for (column, (x_taps, share_x)) in (first_column..last_column).zip(&across) {
                let cover = share_x * share_y;
                if !is_drawn(cover) {
                    continue;
                }
                let pixel = match flat {
                    Some([red, green, blue, alpha]) => [red, green, blue, alpha * single(cover)],
                    None => sample(pixels, width, [*x_taps, y_taps], single(cover)),
                };
                put(row * columns + column, pixel);
            }
        }
    } else {
        for row in first_row..last_row {
            let centre_y = lossless(row) + 0.5;
            for column in first_column..last_column {
                let centre_x = lossless(column) + 0.5;
                let (x, y) = inverse.at(centre_x, centre_y);
                let cover = share((x - low_x) / across_x, (high_x - x) / across_x)
                    * share((y - low_y) / across_y, (high_y - y) / across_y);
                if !is_drawn(cover) {
                    continue;
                }
                let pixel = sample(
                    pixels,
                    width,
                    [taps(x - 0.5, width), taps(y - 0.5, height)],
                    single(cover),
                );
                put(row * columns + column, pixel);
            }
        }
    }
}

/// Whether a coverage puts anything on the output: a number above 0. A coverage that is NaN (a pixel the map cannot place)
/// draws nothing, and is not mistaken for one that does.
fn is_drawn(cover: f64) -> bool {
    matches!(cover.partial_cmp(&0.0), Some(std::cmp::Ordering::Greater))
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
    let mut mixed = [0.0_f32; 4];
    let mut total = 0.0_f32;
    accumulate(pixels, width, [x, y], 1.0, &mut mixed, &mut total);
    finish(mixed, total, cover)
}

/// Adds to `mixed` (premultiplied red, green, blue and alpha) and `total` (the weights) the four taps `[x, y]` of the raster
/// `pixels`, bilinear, each at `tap_weight` times its bilinear weight. Colour and alpha are summed in the same order with a
/// colour never above its alpha, and rounding is monotone, so the mixed colour is never above the mixed alpha and the
/// quotient is at most 1.
fn accumulate(
    pixels: &[f32],
    width: usize,
    [x, y]: [(usize, usize, f32); 2],
    tap_weight: f32,
    mixed: &mut [f32; 4],
    total: &mut f32,
) {
    for (row, weight_row) in [(y.0, 1.0 - y.2), (y.1, y.2)] {
        for (column, weight_column) in [(x.0, 1.0 - x.2), (x.1, x.2)] {
            let weight = tap_weight * (weight_row * weight_column);
            let at = (row * width + column) * 4;
            let [red, green, blue, alpha] = [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]];
            mixed[0] += weight * (red * alpha);
            mixed[1] += weight * (green * alpha);
            mixed[2] += weight * (blue * alpha);
            mixed[3] += weight * alpha;
            *total += weight;
        }
    }
}

/// The straight pixel that the premultiplied sums `mixed` and the sum of weights `total` make, its alpha scaled by `cover`.
/// The alpha is divided by the sum of the weights, added in the same order: an opaque picture is exactly opaque, though
/// weights of 32-bit floats may sum to a step under 1.
fn finish(mixed: [f32; 4], total: f32, cover: f32) -> [f32; 4] {
    let alpha = mixed[3];
    let colour = |value: f32| if alpha > 0.0 { value / alpha } else { 0.0 };
    [
        colour(mixed[0]),
        colour(mixed[1]),
        colour(mixed[2]),
        (alpha / total).min(1.0) * cover,
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

// Inline, because no public route reaches the general kernel for a map that keeps the axes: `warp` takes the other one there,
// and the two must give the same pixels.
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{Affine, Rect, warp, warp_by};
    use crate::Raster;

    /// A `width` by `height` raster whose pixel `(x, y)` is `pixel_of(x, y)`.
    fn made(width: usize, height: usize, pixel_of: impl Fn(usize, usize) -> [f32; 4]) -> Raster {
        let pixels = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .flat_map(|(x, y)| pixel_of(x, y));
        Raster::made(width, height, pixels.collect())
    }

    fn whole(raster: &Raster) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: f64::from(u32::try_from(raster.width()).unwrap_or(0)),
            height: f64::from(u32::try_from(raster.height()).unwrap_or(0)),
        }
    }

    fn scaled(x: f64, y: f64, move_x: f64, move_y: f64) -> Affine {
        Affine {
            a: x,
            b: 0.0,
            c: move_x,
            d: 0.0,
            e: y,
            f: move_y,
        }
    }

    /// The kernel for a map that keeps the axes and the general one give the same pixels, bit for bit: a one-pixel fill and a
    /// constant picture (the cases it was made for), and a picture of different pixels under a move, an enlargement, a flip
    /// and a reduction (which averages first).
    #[test]
    fn the_kernel_for_axis_aligned_maps_gives_what_the_general_kernel_gives() {
        let fill = made(1, 1, |_, _| [0.2, 0.4, 0.6, 0.5]);
        let constant = made(2, 2, |_, _| [0.7, 0.1, 0.3, 0.8]);
        let varied = made(5, 4, |x, y| {
            let level = |number: usize| f32::from(u8::try_from(number).unwrap_or(0)) / 32.0;
            [level(x + 1), level(y + 3), level(x * y + 2), level(8 + (x + y) % 4 * 6)]
        });
        let maps = [
            ("a move by a fraction", scaled(1.0, 1.0, 0.3, -0.45)),
            ("an enlargement", scaled(5.0 / 3.0, 2.5, 1.2, 0.25)),
            ("a flip", scaled(-1.0, 1.0, 5.0, 0.0)),
            ("a flip of y with an enlargement", scaled(2.0, -2.0, 0.5, 9.0)),
            ("a reduction", scaled(0.4, 0.7, 0.3, 0.2)),
            ("a reduction of one axis", scaled(1.0, 0.25, 1.5, 0.5)),
        ];
        for (what, raster) in [
            ("a one-pixel fill", &fill),
            ("a constant picture", &constant),
            ("a picture", &varied),
        ] {
            for (how, map) in maps {
                let clip = whole(raster);
                let general = warp_by(raster, map, clip, 12, 11, false);
                let aligned = warp_by(raster, map, clip, 12, 11, true);
                let bits = |raster: &Raster| raster.pixels().iter().map(|value| value.to_bits()).collect::<Vec<_>>();
                assert_eq!(bits(&aligned), bits(&general), "{what} under {how}");
                // and `warp` itself takes one of them
                let given = warp(&Arc::new(raster.clone()), map, clip, 12, 11);
                assert_eq!(bits(&given), bits(&general), "{what} under {how}, through warp");
            }
        }
    }

    /// A map that changes nothing hands the source back: the same allocation, not a copy.
    #[test]
    fn an_identity_map_hands_back_the_source_itself() {
        let source = Arc::new(made(3, 2, |x, y| {
            [0.1 * f32::from(u8::try_from(x + y).unwrap_or(0)), 0.0, 0.0, 1.0]
        }));
        let clip = whole(&source);
        let same = warp(&source, scaled(1.0, 1.0, 0.0, 0.0), clip, 3, 2);
        assert!(Arc::ptr_eq(&same, &source));
        let moved = warp(&source, scaled(1.0, 1.0, 1.0, 0.0), clip, 3, 2);
        assert!(!Arc::ptr_eq(&moved, &source));
    }
}
