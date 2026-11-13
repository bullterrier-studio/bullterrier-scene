//! The footprint rule: how a warp filters where its map is not a move, a scale, a flip or a quarter turn.
//!
//! An output pixel is a unit square; its **footprint** is the parallelogram of the raster that the inverse map makes of it.
//! The Jacobian of the inverse map (constant for an affine map) gives it two singular values `s1 >= s2`, the lengths in raster
//! pixels of its major and minor axes, and the direction of the major one.
//!
//! * `s1` at most 1 (up to [`ROUNDING`](crate::geometry::ROUNDING)): the footprint is no larger than a raster pixel. The
//!   sample is plain bilinear, as for the maps that keep the axes: a turn by a small angle at scale 1 does not blur.
//! * Otherwise the sample is the mean of `n = ceil(s1 / max(s2, 1))` taps spread evenly along the major axis, each trilinear
//!   at level `log2(max(s2, 1))` of a pyramid of the raster (each level a 2 by 2 area average of the one below). So the
//!   minor axis is filtered to its length, and the major one is covered by taps no more than a level pixel apart: an average
//!   over the footprint, anisotropic where the footprint is long and thin (a strong skew, the far side of a perspective).
//!
//! Every weight is at least 0, so the filter is convex like the rest of the warp. Every number comes from the singular
//! values; there is no threshold and no cap on `n` but the size of the raster (a footprint longer than its diagonal reads
//! the edge past it). The taps of a layer total about its pixels over `s2`, and the worst cases are measured in the
//! decision entry.

use std::cell::OnceCell;

use super::{Affine, Rect, accumulate, finish, is_drawn, lossless, reach, reduce, sample, share, taps};
use crate::Raster;
use crate::geometry::{ROUNDING, whole_up};

/// [`warp_through`](super::warp_through) for a map that is not of the exact class: each output pixel by its footprint.
pub(super) fn through(
    source: &Raster,
    map: Affine,
    clip: Rect,
    (columns, rows): (usize, usize),
    put: &mut impl FnMut(usize, [f32; 4]),
) {
    let (width, height) = (source.width(), source.height());
    let inverse = map.inverse();
    let diagonal = lossless(width).hypot(lossless(height));
    let footprint = Footprint::of([[inverse.a, inverse.b], [inverse.d, inverse.e]], diagonal);
    let pyramid = Pyramid::of(source);
    // raster pixels per output pixel along the gradient of each raster axis: the extent of an output pixel across an edge
    let (across_x, across_y) = (inverse.a.hypot(inverse.b), inverse.d.hypot(inverse.e));
    let (low_x, high_x) = (clip.x, clip.x + clip.width);
    let (low_y, high_y) = (clip.y, clip.y + clip.height);
    let (first_row, last_row, first_column, last_column) = reach(map, clip, columns, rows);
    for row in first_row..last_row {
        let centre_y = lossless(row) + 0.5;
        for column in first_column..last_column {
            let (x, y) = inverse.at(lossless(column) + 0.5, centre_y);
            let cover = share((x - low_x) / across_x, (high_x - x) / across_x)
                * share((y - low_y) / across_y, (high_y - y) / across_y);
            if !is_drawn(cover) {
                continue;
            }
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a share from 0 to 1 is held as a 32-bit float"
            )]
            let cover = cover as f32;
            let pixel = if footprint.plain {
                sample(
                    source.pixels(),
                    width,
                    [taps(x - 0.5, width), taps(y - 0.5, height)],
                    cover,
                )
            } else {
                footprint.sample(&pyramid, (x, y), cover)
            };
            put(row * columns + column, pixel);
        }
    }
}

/// What the footprint of an output pixel asks of the sample.
struct Footprint {
    /// No larger than a raster pixel: plain bilinear.
    plain: bool,
    /// The taps along the major axis.
    taps: usize,
    /// The length of the major axis in raster pixels, over which the taps are spread.
    length: f64,
    /// The direction of the major axis in the raster.
    along: (f64, f64),
    /// The pyramid level of each tap, between two whole levels.
    level: f64,
}

impl Footprint {
    /// The footprint of a map whose inverse has the Jacobian `jacobian` (raster pixels per output pixel). A footprint longer
    /// than `reach`, the diagonal of the raster, takes no more taps than that covers: past it every tap would read the edge.
    fn of(jacobian: [[f64; 2]; 2], reach: f64) -> Self {
        let [[a, b], [c, d]] = jacobian;
        let determinant = (a * d - b * c).abs();
        let squares = a * a + b * b + c * c + d * d;
        // the singular values are the roots of s^2 - sqrt(squares + 2 det) ... : s1 + s2 and s1 - s2 are two square roots
        let sum = (squares + 2.0 * determinant).sqrt();
        let difference = (squares - 2.0 * determinant).max(0.0).sqrt();
        let major = f64::midpoint(sum, difference);
        let minor = if major > 0.0 { determinant / major } else { 0.0 };
        // the major axis of the footprint in the raster is the eigenvector of J J^T of the larger eigenvalue
        let (first, mixed, second) = (a * a + b * b, a * c + b * d, c * c + d * d);
        let angle = 0.5 * (2.0 * mixed).atan2(first - second);
        let kept = minor.max(1.0);
        let length = major.min(reach);
        Self {
            plain: major <= 1.0 + ROUNDING,
            taps: whole_up(length / kept).max(1),
            length,
            along: (angle.cos(), angle.sin()),
            level: if minor <= 1.0 + ROUNDING { 0.0 } else { minor.log2() },
        }
    }

    /// The pixel at `position` (in raster pixels) filtered over the footprint, its alpha scaled by `cover`.
    fn sample(&self, pyramid: &Pyramid<'_>, (x, y): (f64, f64), cover: f32) -> [f32; 4] {
        let mut mixed = [0.0_f32; 4];
        let mut total = 0.0_f32;
        let low = self.level.floor();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a level from 0 to the height of the pyramid, and a weight from 0 to 1 held as a 32-bit float"
        )]
        let (low_level, toward_high) = (low as usize, (self.level - low) as f32);
        #[expect(clippy::cast_precision_loss, reason = "a count of taps below 2^53")]
        let share = 1.0 / self.taps as f32;
        for tap in 0..self.taps {
            #[expect(clippy::cast_precision_loss, reason = "a count of taps below 2^53")]
            let along = ((tap as f64 + 0.5) / self.taps as f64 - 0.5) * self.length;
            let (at_x, at_y) = (x + along * self.along.0, y + along * self.along.1);
            for (level, weight) in [(low_level, 1.0 - toward_high), (low_level + 1, toward_high)] {
                if weight > 0.0 {
                    let raster = pyramid.level(level);
                    // raster pixels of the base to pixels of the level
                    let (to_x, to_y) = (
                        lossless(raster.width()) / lossless(pyramid.base.width()),
                        lossless(raster.height()) / lossless(pyramid.base.height()),
                    );
                    accumulate(
                        raster.pixels(),
                        raster.width(),
                        [
                            taps(at_x * to_x - 0.5, raster.width()),
                            taps(at_y * to_y - 0.5, raster.height()),
                        ],
                        share * weight,
                        &mut mixed,
                        &mut total,
                    );
                }
            }
        }
        finish(mixed, total, cover)
    }
}

/// A raster and the levels above it, each the 2 by 2 area average of the one below (a side of an odd number of pixels is
/// averaged into one of half of it, rounded up), made when first asked for.
struct Pyramid<'a> {
    base: &'a Raster,
    above: Vec<OnceCell<Raster>>,
}

impl<'a> Pyramid<'a> {
    /// The pyramid of `base`, to the level of one pixel.
    fn of(base: &'a Raster) -> Self {
        let (mut width, mut height) = (base.width(), base.height());
        let mut above = Vec::new();
        while width > 1 || height > 1 {
            width = width.div_ceil(2);
            height = height.div_ceil(2);
            above.push(OnceCell::new());
        }
        Self { base, above }
    }

    /// Level `number` (0 is the raster itself), or the top level where it is past it.
    fn level(&self, number: usize) -> &Raster {
        let number = number.min(self.above.len());
        if number == 0 {
            return self.base;
        }
        self.above[number - 1].get_or_init(|| {
            let below = self.level(number - 1);
            reduce(below, below.width().div_ceil(2), below.height().div_ceil(2))
        })
    }
}
