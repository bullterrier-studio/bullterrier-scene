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

use super::{Affine, Projective, Rect, accumulate, finish, is_positive, lossless, reach, reduce, sample, share, taps};
use crate::Raster;
use crate::geometry::{ROUNDING, whole_up};

/// Where an output pixel falls in the raster, and how the raster changes around it.
struct Local {
    /// The raster point the centre of the pixel falls on.
    x: f64,
    y: f64,
    /// Raster pixels per output pixel: `[[dx/dX, dx/dY], [dy/dX, dy/dY]]`.
    jacobian: [[f64; 2]; 2],
}

/// A map from the output back to the raster.
trait Back {
    /// Where the centre `(x, y)` of an output pixel falls, or `None` where the map does not place it (behind the horizon of
    /// a perspective).
    fn at(&self, x: f64, y: f64) -> Option<Local>;
    /// Where the raster point `(x, y)` lands on the output.
    fn forward(&self, x: f64, y: f64) -> (f64, f64);
    /// The Jacobian where it is the same everywhere (an affine map), else `None`.
    fn constant(&self) -> Option<[[f64; 2]; 2]>;
}

/// An affine map and its inverse.
struct AffineBack {
    map: Affine,
    inverse: Affine,
}

impl Back for AffineBack {
    fn at(&self, x: f64, y: f64) -> Option<Local> {
        let (x, y) = self.inverse.at(x, y);
        Some(Local {
            x,
            y,
            jacobian: self.constant()?,
        })
    }

    fn forward(&self, x: f64, y: f64) -> (f64, f64) {
        self.map.at(x, y)
    }

    fn constant(&self) -> Option<[[f64; 2]; 2]> {
        Some([[self.inverse.a, self.inverse.b], [self.inverse.d, self.inverse.e]])
    }
}

/// A homography and its inverse, signed so that the points the map places in front of the horizon have a positive last
/// coordinate in both.
struct ProjectiveBack {
    map: [[f64; 3]; 3],
    inverse: [[f64; 3]; 3],
}

impl ProjectiveBack {
    /// The two directions of `map`, which is given the sign that makes the middle of `clip` land in front.
    fn new(map: Projective, clip: Rect) -> Self {
        let mut map = map.0;
        let (middle_x, middle_y) = (clip.x + clip.width / 2.0, clip.y + clip.height / 2.0);
        if along(map[2], middle_x, middle_y) < 0.0 {
            map = map.map(|row| row.map(|entry| -entry));
        }
        // the adjugate is the inverse times the determinant, whose sign the last coordinate takes: undo it
        let cofactor = |row: usize, column: usize| {
            let (r1, r2) = ((row + 1) % 3, (row + 2) % 3);
            let (c1, c2) = ((column + 1) % 3, (column + 2) % 3);
            map[r1][c1] * map[r2][c2] - map[r1][c2] * map[r2][c1]
        };
        let determinant: f64 = (0..3).map(|column| map[0][column] * cofactor(0, column)).sum();
        let sign = if determinant < 0.0 { -1.0 } else { 1.0 };
        // the inverse's entry (row, column) is the cofactor of (column, row)
        Self {
            map,
            inverse: std::array::from_fn(|row| std::array::from_fn(|column| cofactor(column, row) * sign)),
        }
    }
}

/// `row` applied to the point `(x, y)`: `row[0] x + row[1] y + row[2]`.
fn along(row: [f64; 3], x: f64, y: f64) -> f64 {
    row[0] * x + row[1] * y + row[2]
}

impl Back for ProjectiveBack {
    fn at(&self, x: f64, y: f64) -> Option<Local> {
        let [first, second, last] = self.inverse;
        let depth = along(last, x, y);
        if !is_positive(depth) {
            return None;
        }
        let (at_x, at_y) = (along(first, x, y) / depth, along(second, x, y) / depth);
        // d(raster) / d(output): each row's derivative, less the point times the last row's, over w
        let jacobian = [
            [(first[0] - at_x * last[0]) / depth, (first[1] - at_x * last[1]) / depth],
            [
                (second[0] - at_y * last[0]) / depth,
                (second[1] - at_y * last[1]) / depth,
            ],
        ];
        let finite = [
            at_x,
            at_y,
            jacobian[0][0],
            jacobian[0][1],
            jacobian[1][0],
            jacobian[1][1],
        ]
        .iter()
        .all(|value| value.is_finite());
        finite.then_some(Local {
            x: at_x,
            y: at_y,
            jacobian,
        })
    }

    fn forward(&self, x: f64, y: f64) -> (f64, f64) {
        let [first, second, last] = self.map;
        let depth = along(last, x, y);
        (along(first, x, y) / depth, along(second, x, y) / depth)
    }

    fn constant(&self) -> Option<[[f64; 2]; 2]> {
        None
    }
}

/// [`warp_through`](super::warp_through) for an affine map that is not of the exact class: each output pixel by its
/// footprint.
pub(super) fn through(
    source: &Raster,
    map: Affine,
    clip: Rect,
    (columns, rows): (usize, usize),
    put: &mut impl FnMut(usize, [f32; 4]),
) {
    let back = AffineBack {
        map,
        inverse: map.inverse(),
    };
    through_back(source, &back, clip, (columns, rows), put);
}

/// The warp of a perspective: each output pixel by its footprint, which differs from pixel to pixel.
pub(super) fn through_projective(
    source: &Raster,
    map: Projective,
    clip: Rect,
    (columns, rows): (usize, usize),
    put: &mut impl FnMut(usize, [f32; 4]),
) {
    through_back(source, &ProjectiveBack::new(map, clip), clip, (columns, rows), put);
}

fn through_back(
    source: &Raster,
    back: &impl Back,
    clip: Rect,
    (columns, rows): (usize, usize),
    put: &mut impl FnMut(usize, [f32; 4]),
) {
    let (width, height) = (source.width(), source.height());
    let diagonal = lossless(width).hypot(lossless(height));
    let fixed = back.constant().map(|jacobian| Footprint::of(jacobian, diagonal));
    let pyramid = Pyramid::of(source);
    let (low_x, high_x) = (clip.x, clip.x + clip.width);
    let (low_y, high_y) = (clip.y, clip.y + clip.height);
    let (first_row, last_row, first_column, last_column) = reach(|x, y| back.forward(x, y), clip, columns, rows);
    for row in first_row..last_row {
        let centre_y = lossless(row) + 0.5;
        for column in first_column..last_column {
            let Some(Local { x, y, jacobian }) = back.at(lossless(column) + 0.5, centre_y) else {
                continue;
            };
            // raster pixels per output pixel along the gradient of each raster axis: the extent of an output pixel across
            // an edge
            let (across_x, across_y) = (
                jacobian[0][0].hypot(jacobian[0][1]),
                jacobian[1][0].hypot(jacobian[1][1]),
            );
            let cover = share((x - low_x) / across_x, (high_x - x) / across_x)
                * share((y - low_y) / across_y, (high_y - y) / across_y);
            if !is_positive(cover) {
                continue;
            }
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a share from 0 to 1 is held as a 32-bit float"
            )]
            let cover = cover as f32;
            let footprint = fixed.unwrap_or_else(|| Footprint::of(jacobian, diagonal));
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
#[derive(Clone, Copy)]
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
        if major <= 1.0 + ROUNDING {
            return Self {
                plain: true,
                taps: 1,
                length: major,
                along: (1.0, 0.0),
                level: 0.0,
            };
        }
        // the major axis of the footprint in the raster is the eigenvector of J J^T of the larger eigenvalue
        let (first, mixed, second) = (a * a + b * b, a * c + b * d, c * c + d * d);
        let angle = 0.5 * (2.0 * mixed).atan2(first - second);
        let kept = minor.max(1.0);
        let length = major.min(reach);
        Self {
            plain: false,
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

// Inline, because no public route reaches the inverse of a homography on its own: a render shows it only through the pixels.
#[cfg(test)]
mod tests {
    use super::{Back, ProjectiveBack};
    use crate::geometry::Rect;
    use crate::perspective::Quad;
    use crate::warp::Projective;

    /// A point sent forward and back by the two directions of a homography returns, whatever the sign the matrix was given.
    #[test]
    fn a_homography_and_its_inverse_return_the_point_they_were_given() {
        let quad = Quad::new([[11.0, 7.0], [50.0, 15.0], [58.0, 55.0], [5.0, 44.0]]).unwrap_or_else(|_| unreachable!());
        let clip = Rect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        };
        for sign in [1.0, -1.0] {
            let matrix = quad.matrix().map(|row| row.map(|entry| entry * sign));
            let back = ProjectiveBack::new(Projective(matrix), clip);
            for (x, y) in [(0.0, 0.0), (1.0, 0.0), (0.3, 0.9), (0.5, 0.5), (1.0, 1.0)] {
                let (out_x, out_y) = back.forward(x, y);
                let local = back.at(out_x, out_y);
                assert!(local.is_some(), "({x}, {y}) is in front of the horizon");
                let local = local.unwrap_or_else(|| unreachable!());
                assert!(
                    (local.x - x).abs() < 1e-9 && (local.y - y).abs() < 1e-9,
                    "({x}, {y}) came back as ({}, {})",
                    local.x,
                    local.y
                );
            }
        }
    }

    /// A point beyond the horizon of the homography (the line where its last coordinate is 0) is not placed.
    #[test]
    fn a_point_behind_the_horizon_is_not_placed() {
        let quad = Quad::new([[20.0, 0.0], [30.0, 0.0], [60.0, 50.0], [0.0, 50.0]]).unwrap_or_else(|_| unreachable!());
        let clip = Rect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        };
        let back = ProjectiveBack::new(Projective(quad.matrix()), clip);
        // the far edge is the top (y = 0): above it the plane goes to the horizon and past it
        let placed = (0..200).filter(|&row| back.at(25.0, -f64::from(row)).is_some()).count();
        assert!(
            placed > 0 && placed < 200,
            "{placed} of 200 points above the quadrilateral are placed"
        );
    }
}
