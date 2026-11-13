//! Where a layer's box goes on the canvas: `fit` into a box, then `place` (flip, scale, skew, rotate about an anchor, move).
//!
//! Every number is checked once, where it enters (`Layer::fit`, `Layer::place`); the renderer trusts them.

use std::str::FromStr;

use crate::Error;
use crate::geometry::{Axis, MAX_PIXELS, Rect, Scale};
use crate::perspective::Quad;
use crate::ranges::{FRACTION, RATIO, SKEW, TRAVEL};
use crate::source::{Drawn, Request};
use crate::warp::{Affine, Map, Projective};

/// How `fit` puts a layer's box into the box it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Scaled, its aspect kept, to fill the box; what overflows is cut off at the box's edge.
    Cover,
    /// Scaled, its aspect kept, to lie wholly inside the box.
    Contain,
    /// Scaled to the box exactly, its aspect not kept.
    Stretch,
}

impl FromStr for Fit {
    type Err = Error;

    /// `"cover"`, `"contain"` or `"stretch"`.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `how` for any other word.
    fn from_str(word: &str) -> Result<Self, Error> {
        match word {
            "cover" => Ok(Self::Cover),
            "contain" => Ok(Self::Contain),
            "stretch" => Ok(Self::Stretch),
            other => Err(Error::invalid(
                "how",
                format!("\"cover\", \"contain\" or \"stretch\" is wanted, got {other:?}"),
            )),
        }
    }
}

/// An angle in degrees. Rotation is clockwise (the canvas has y down) and any size: it wraps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deg(pub f64);

/// The fields of a layer's placement that a call sets; the others keep their value. Build one with [`Place::new`] and the
/// setters, and give it to [`Layer::place`](crate::Layer::place), which checks each number against its range and says
/// what they are.
///
/// The finished raster is flipped, scaled by `scale` (and by `aspect` along y), skewed (`skew_x`, then `skew_y`) and
/// rotated, all about `anchor` (fractions of the fitted box, the centre by default), then moved by `dx`, `dy` design pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[must_use]
pub struct Place {
    dx: Option<f64>,
    dy: Option<f64>,
    scale: Option<f64>,
    aspect: Option<f64>,
    rotate: Option<f64>,
    skew_x: Option<f64>,
    skew_y: Option<f64>,
    flip_x: Option<bool>,
    flip_y: Option<bool>,
    anchor: Option<(f64, f64)>,
}

impl Place {
    /// A placement that sets nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Move by `value` design pixels along x (default 0).
    pub fn dx(mut self, value: f64) -> Self {
        self.dx = Some(value);
        self
    }

    /// Move by `value` design pixels along y (default 0).
    pub fn dy(mut self, value: f64) -> Self {
        self.dy = Some(value);
        self
    }

    /// Scale by `value` (default 1), along x and y.
    pub fn scale(mut self, value: f64) -> Self {
        self.scale = Some(value);
        self
    }

    /// Scale along y by `value` times what `scale` gives (default 1): 0.12 is a flat shadow.
    pub fn aspect(mut self, value: f64) -> Self {
        self.aspect = Some(value);
        self
    }

    /// Rotate clockwise by `angle`, any size (default 0).
    pub fn rotate(mut self, angle: Deg) -> Self {
        self.rotate = Some(angle.0);
        self
    }

    /// Skew along x by `angle` (default 0): a point moves along x by its y times the tangent.
    pub fn skew_x(mut self, angle: Deg) -> Self {
        self.skew_x = Some(angle.0);
        self
    }

    /// Skew along y by `angle` (default 0), after `skew_x`.
    pub fn skew_y(mut self, angle: Deg) -> Self {
        self.skew_y = Some(angle.0);
        self
    }

    /// Mirror left to right (`true`) or not (default).
    pub fn flip_x(mut self, on: bool) -> Self {
        self.flip_x = Some(on);
        self
    }

    /// Mirror top to bottom (`true`) or not (default).
    pub fn flip_y(mut self, on: bool) -> Self {
        self.flip_y = Some(on);
        self
    }

    /// The point the layer is flipped, scaled, skewed and turned about, as fractions of the fitted box along x and y (default
    /// `(0.5, 0.5)`, the centre).
    pub fn anchor(mut self, x: f64, y: f64) -> Self {
        self.anchor = Some((x, y));
        self
    }
}

/// A layer's `fit`, as given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fitting {
    area: Rect,
    how: Fit,
    align: (f64, f64),
}

impl Fitting {
    /// `fit(area, how, align)`, checked.
    pub(crate) fn new(area: [f64; 4], how: Fit, align: [f64; 2]) -> Result<Self, Error> {
        let [x, y, width, height] = area;
        if !(x.is_finite() && y.is_finite() && width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            return Err(Error::invalid(
                "box",
                format!("(x, y, width, height) with a width and a height above 0 is wanted, got {area:?}"),
            ));
        }
        Ok(Self {
            area: Rect { x, y, width, height },
            how,
            align: FRACTION.check_pair("align", (align[0], align[1]))?,
        })
    }
}

/// Where a layer's fitted box goes: by the fields of `place`, or onto the quadrilateral of `perspective`. One state: the last of
/// the two calls replaces the other, and a `place` after a `perspective` starts from the default fields, not from those of
/// an earlier `place`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Placement {
    /// Flip, scale, skew, turn about an anchor, move.
    Place(Placed),
    /// The fitted box's corners on a convex quadrilateral.
    Perspective(Quad),
}

impl Default for Placement {
    fn default() -> Self {
        Self::Place(Placed::default())
    }
}

impl Placement {
    /// This placement with the fields `place` sets, each checked: those of the placement it was, or the defaults if it was a
    /// perspective.
    pub(crate) fn placed(self, place: Place) -> Result<Self, Error> {
        let before = match self {
            Self::Place(placed) => placed,
            Self::Perspective(_) => Placed::default(),
        };
        Ok(Self::Place(before.with(place)?))
    }
}

/// A layer's placement, every field set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placed {
    dx: f64,
    dy: f64,
    scale: f64,
    aspect: f64,
    rotate: f64,
    skew_x: f64,
    skew_y: f64,
    flip_x: bool,
    flip_y: bool,
    anchor: (f64, f64),
}

impl Default for Placed {
    fn default() -> Self {
        Self {
            dx: 0.0,
            dy: 0.0,
            scale: 1.0,
            aspect: 1.0,
            rotate: 0.0,
            skew_x: 0.0,
            skew_y: 0.0,
            flip_x: false,
            flip_y: false,
            anchor: (0.5, 0.5),
        }
    }
}

impl Placed {
    /// This placement with the fields `place` sets, each checked.
    pub(crate) fn with(self, place: Place) -> Result<Self, Error> {
        let anchor = place
            .anchor
            .map_or(Ok(self.anchor), |pair| FRACTION.check_pair("anchor", pair))?;
        let rotate = match place.rotate {
            Some(angle) if !angle.is_finite() => {
                return Err(Error::invalid(
                    "rotate",
                    format!("a number of degrees is wanted, got {angle}"),
                ));
            }
            Some(angle) => angle,
            None => self.rotate,
        };
        Ok(Self {
            dx: place.dx.map_or(Ok(self.dx), |v| TRAVEL.check("dx", v))?,
            dy: place.dy.map_or(Ok(self.dy), |v| TRAVEL.check("dy", v))?,
            scale: place.scale.map_or(Ok(self.scale), |v| RATIO.check("scale", v))?,
            aspect: place.aspect.map_or(Ok(self.aspect), |v| RATIO.check("aspect", v))?,
            rotate,
            skew_x: place.skew_x.map_or(Ok(self.skew_x), |v| SKEW.check("skew_x", v))?,
            skew_y: place.skew_y.map_or(Ok(self.skew_y), |v| SKEW.check("skew_y", v))?,
            flip_x: place.flip_x.unwrap_or(self.flip_x),
            flip_y: place.flip_y.unwrap_or(self.flip_y),
            anchor,
        })
    }
}

/// The window of the box that is shown, the fitted box on the canvas, and the scale along each axis, for a source whose box
/// is `own` (its top left at the origin) fitted by `fit`. The axis that binds is set to the box's own size, not computed,
/// so that a fit into a box of the same shape is exact.
fn fitted(own: Rect, fit: Option<Fitting>) -> (Rect, Rect, (f64, f64)) {
    let Some(Fitting { area, how, align }) = fit else {
        return (own, own, (1.0, 1.0));
    };
    let (across, down) = (area.width / own.width, area.height / own.height);
    match how {
        Fit::Stretch => (own, area, (across, down)),
        Fit::Contain => {
            let (width, height, factor) = if across <= down {
                (area.width, own.height * across, across)
            } else {
                (own.width * down, area.height, down)
            };
            let placed = Rect {
                x: area.x + (area.width - width) * align.0,
                y: area.y + (area.height - height) * align.1,
                width,
                height,
            };
            (own, placed, (factor, factor))
        }
        Fit::Cover => {
            let (width, height, factor) = if across >= down {
                (own.width, area.height / across, across)
            } else {
                (area.width / down, own.height, down)
            };
            let window = Rect {
                x: own.x + (own.width - width) * align.0,
                y: own.y + (own.height - height) * align.1,
                width,
                height,
            };
            (window, area, (factor, factor))
        }
    }
}

/// The cosine and sine of a turn of `degrees` clockwise (the canvas has y down). A whole quarter turn is exact, so that a
/// layer turned by it is a permutation of its pixels.
#[expect(clippy::float_cmp, reason = "the quarter turns are matched exactly")]
fn turn(degrees: f64) -> (f64, f64) {
    let wrapped = degrees.rem_euclid(360.0);
    if wrapped == 0.0 || wrapped == 360.0 {
        (1.0, 0.0)
    } else if wrapped == 90.0 {
        (0.0, 1.0)
    } else if wrapped == 180.0 {
        (-1.0, 0.0)
    } else if wrapped == 270.0 {
        (0.0, -1.0)
    } else {
        let (sine, cosine) = wrapped.to_radians().sin_cos();
        (cosine, sine)
    }
}

/// The product of two 2 by 2 matrices.
fn multiply(left: [[f64; 2]; 2], right: [[f64; 2]; 2]) -> [[f64; 2]; 2] {
    let entry = |row: usize, column: usize| left[row][0] * right[0][column] + left[row][1] * right[1][column];
    [[entry(0, 0), entry(0, 1)], [entry(1, 0), entry(1, 1)]]
}

impl Placed {
    /// What the layer's points go through about the anchor, without the scale: the flip, the aspect, the skew along x and
    /// then along y, and the turn, as `turn x skew_y x skew_x x aspect x flip` applied to a point.
    fn linear(self) -> [[f64; 2]; 2] {
        let flip = [
            [if self.flip_x { -1.0 } else { 1.0 }, 0.0],
            [0.0, self.aspect * if self.flip_y { -1.0 } else { 1.0 }],
        ];
        let skew_x = [[1.0, self.skew_x.to_radians().tan()], [0.0, 1.0]];
        let skew_y = [[1.0, 0.0], [self.skew_y.to_radians().tan(), 1.0]];
        let (cosine, sine) = turn(self.rotate);
        let turned = [[cosine, -sine], [sine, cosine]];
        multiply(turned, multiply(skew_y, multiply(skew_x, flip)))
    }
}

/// The product of two 3 by 3 matrices.
fn multiply3(left: [[f64; 3]; 3], right: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let entry = |row: usize, column: usize| (0..3).map(|k| left[row][k] * right[k][column]).sum();
    [
        [entry(0, 0), entry(0, 1), entry(0, 2)],
        [entry(1, 0), entry(1, 1), entry(1, 2)],
        [entry(2, 0), entry(2, 1), entry(2, 2)],
    ]
}

/// Where one layer's box lands, for a source of a given size.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Layout {
    scale: Scale,
    /// The part of the source's box that is shown.
    window: Rect,
    /// The fitted box, on the canvas.
    area: Rect,
    /// The scale `fit` gives, along each axis.
    factor: (f64, f64),
    placement: Placement,
}

impl Layout {
    /// The layout of a source whose box is `own`, drawn on an output of scale `scale`.
    pub(crate) fn new(fit: Option<Fitting>, placement: Placement, own: Rect, scale: Scale) -> Self {
        let (window, area, factor) = fitted(own, fit);
        Self {
            scale,
            window,
            area,
            factor,
            placement,
        }
    }

    /// What the source is asked to draw: the window, at the output scale times the fit and the zoom. For a perspective, at
    /// the largest scale the quadrilateral gives any corner of the window along each axis (the near side is sharp), cut down
    /// by one factor if what the source allocates (`whole` says whether that is its whole box or the window) would pass the
    /// 2^27 pixels of a layer: the render stays valid and the near side loses sharpness.
    pub(crate) fn request(&self, own: Rect, whole: bool) -> Request {
        let placed = match self.placement {
            Placement::Place(placed) => placed,
            Placement::Perspective(quad) => {
                let (across, down) = self.largest_scales(&quad);
                let part = if whole { own } else { self.window };
                let fit = capped(part, (across, down));
                return Request {
                    window: self.window,
                    scale: Scale::exactly(across * fit, down * fit),
                    factor: self.factor,
                };
            }
        };
        let zoom = placed.scale;
        Request {
            window: self.window,
            scale: self.scale.times(self.factor.0 * zoom, self.factor.1 * zoom),
            factor: self.factor,
        }
    }

    /// The largest, over the four corners of the fitted box, of the output pixels a design pixel of the box has along its x
    /// axis and along its y axis under the quadrilateral: the scale is largest at a corner, since a homography's local scale
    /// changes by the last coordinate, which is linear (`the_largest_scale_of_a_perspective_is_at_a_corner` walks the box).
    fn largest_scales(&self, quad: &Quad) -> (f64, f64) {
        [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
            .into_iter()
            .map(|corner| self.scales_at(quad, corner))
            .fold((0.0, 0.0), |(across, down), (x, y)| {
                (f64::max(across, x), f64::max(down, y))
            })
    }

    /// The output pixels a design pixel of the box has along x and along y at the point `(u, v)` of the fitted box (each
    /// from 0 to 1): the lengths of the columns of the Jacobian of the output with respect to the box's design pixels.
    fn scales_at(&self, quad: &Quad, (u, v): (f64, f64)) -> (f64, f64) {
        let m = quad.matrix();
        let axes = [self.scale.x.pixels(1.0), self.scale.y.pixels(1.0)];
        let w = m[2][0] * u + m[2][1] * v + m[2][2];
        let (qx, qy) = (
            (m[0][0] * u + m[0][1] * v + m[0][2]) / w,
            (m[1][0] * u + m[1][1] * v + m[1][2]) / w,
        );
        // d(output) / d(box x) and / d(box y): the homography's derivative by u and by v, over the box's design pixels
        let (per_x, per_y) = (self.factor.0 / self.area.width, self.factor.1 / self.area.height);
        let along_x = [
            axes[0] * (m[0][0] - qx * m[2][0]) / w * per_x,
            axes[1] * (m[1][0] - qy * m[2][0]) / w * per_x,
        ];
        let along_y = [
            axes[0] * (m[0][1] - qx * m[2][1]) / w * per_y,
            axes[1] * (m[1][1] - qy * m[2][1]) / w * per_y,
        ];
        (along_x[0].hypot(along_x[1]), along_y[0].hypot(along_y[1]))
    }

    /// The warp of `drawn` onto the output: the map from its pixels to output pixels, and the clip in its pixels. A pixel `p`
    /// of the box is, in the fitted box, `(p - window) x factor`; then, about the anchor `a` and with `linear` the matrix of
    /// `Placed::linear`, `a + zoom x linear x (. - a)`; then moved by `(dx, dy)`; and each output axis scales by its own
    /// output scale. Each entry of the map is formed as one quotient of whole numbers where it can be (the count of pixels
    /// of the raster is the divisor), so that a map that is the identity, a move or a quarter turn has exact entries. A
    /// perspective puts the fitted box on its quadrilateral instead, in the same way when that is a parallelogram.
    pub(crate) fn warp(&self, drawn: &Drawn) -> (Map, Rect) {
        let covers = drawn.covers;
        // the raster's size, as floats
        #[expect(clippy::cast_precision_loss, reason = "a side of at most 2^27 pixels")]
        let counts = [drawn.raster.width() as f64, drawn.raster.height() as f64];
        let clip = if self.window == covers {
            Rect {
                x: 0.0,
                y: 0.0,
                width: counts[0],
                height: counts[1],
            }
        } else {
            let (across, down) = (counts[0] / covers.width, counts[1] / covers.height);
            Rect {
                x: (self.window.x - covers.x) * across,
                y: (self.window.y - covers.y) * down,
                width: self.window.width * across,
                height: self.window.height * down,
            }
        };
        let axes = [self.scale.x, self.scale.y];
        // the length of a pixel of the raster along each of its axes in the fitted box, times the pixels it holds
        let extent = [self.factor.0 * covers.width, self.factor.1 * covers.height];
        // where the raster's top left is in the fitted box
        let corner = [
            (covers.x - self.window.x) * self.factor.0,
            (covers.y - self.window.y) * self.factor.1,
        ];
        let placed = match self.placement {
            Placement::Place(placed) => placed,
            Placement::Perspective(quad) => {
                return (self.perspective(&quad, axes, extent, corner, counts), clip);
            }
        };
        let linear = placed.linear();
        let entry = |row: usize, column: usize| {
            linear[row][column] * axes[row].pixels(placed.scale * extent[column]) / counts[column]
        };
        let anchor = [placed.anchor.0 * self.area.width, placed.anchor.1 * self.area.height];
        let origin = [self.area.x, self.area.y];
        let moved = [placed.dx, placed.dy];
        let translation = |row: usize| {
            let turned: f64 = (0..2)
                .map(|column| linear[row][column] * (corner[column] - anchor[column]))
                .sum();
            axes[row].pixels(origin[row] + anchor[row] + moved[row] + placed.scale * turned)
        };
        let map = Affine {
            a: entry(0, 0),
            b: entry(0, 1),
            c: translation(0),
            d: entry(1, 0),
            e: entry(1, 1),
            f: translation(1),
        };
        (Map::Affine(map), clip)
    }

    /// The map of a perspective: raster pixel to the point of the fitted box (a unit square), the box's homography to the
    /// quadrilateral, and the output scale of each axis. A parallelogram is affine, and its entries are formed as those of
    /// `place` are, so that an axis-aligned one that `place` can make gives the same map bit for bit.
    fn perspective(&self, quad: &Quad, axes: [Axis; 2], extent: [f64; 2], corner: [f64; 2], counts: [f64; 2]) -> Map {
        let m = quad.matrix();
        let (width, height) = (self.area.width, self.area.height);
        if m[2][0] == 0.0 && m[2][1] == 0.0 {
            let ratio = [[m[0][0] / width, m[0][1] / height], [m[1][0] / width, m[1][1] / height]];
            let entry =
                |row: usize, column: usize| axes[row].pixels(ratio[row][column] * extent[column]) / counts[column];
            let translation =
                |row: usize| axes[row].pixels(m[row][2] + ratio[row][0] * corner[0] + ratio[row][1] * corner[1]);
            return Map::Affine(Affine {
                a: entry(0, 0),
                b: entry(0, 1),
                c: translation(0),
                d: entry(1, 0),
                e: entry(1, 1),
                f: translation(1),
            });
        }
        // the raster pixel to the unit square, then the homography, then the output scale
        let to_square = [
            [extent[0] / counts[0] / width, 0.0, corner[0] / width],
            [0.0, extent[1] / counts[1] / height, corner[1] / height],
            [0.0, 0.0, 1.0],
        ];
        let to_output = [
            [axes[0].pixels(1.0), 0.0, 0.0],
            [0.0, axes[1].pixels(1.0), 0.0],
            [0.0, 0.0, 1.0],
        ];
        Map::Projective(Projective(multiply3(to_output, multiply3(m, to_square))))
    }
}

/// The factor, at most 1, by which a part `part` of a box drawn at the scales `(across, down)` must be made smaller to have
/// no more than 2^27 pixels: the ceilings add at most a pixel to each side, so `(w k a + 1)(h k d + 1)` is the bound, and `k`
/// is the root of that quadratic.
fn capped(part: Rect, (across, down): (f64, f64)) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "the limit is a power of two: exact")]
    let limit = MAX_PIXELS as f64;
    let (wide, tall) = (part.width * across, part.height * down);
    let (square, sides) = (wide * tall, wide + tall);
    // wide tall k^2 + (wide + tall) k + 1 - limit = 0
    let root = (-sides + (sides * sides + 4.0 * square * (limit - 1.0)).sqrt()) / (2.0 * square);
    root.min(1.0)
}

// Inline, because no public route reaches these: a render shows the scale a source is asked for only through its pixels.
#[cfg(test)]
mod tests {
    use super::{Fit, Fitting, Layout, Placement, capped};
    use crate::geometry::{MAX_PIXELS, Rect, Scale};
    use crate::perspective::Quad;

    fn rect(width: f64, height: f64) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        }
    }

    /// The cut keeps the ceilinged size of the part within 2^27 pixels, and leaves a part that fits alone.
    #[test]
    fn the_cut_keeps_the_drawn_size_of_a_part_within_the_limit_and_leaves_a_small_one() {
        for (width, height, across, down) in [
            (1000.0, 1000.0, 100.0, 100.0),
            (3.0, 7.0, 1e7, 1e6),
            (1e6, 1e-3, 500.0, 1e4),
            (4000.0, 3000.0, 40.0, 10.0),
            (1.0, 1.0, 11584.0, 11584.0),
        ] {
            let factor = capped(rect(width, height), (across, down));
            assert!(factor > 0.0 && factor <= 1.0, "{factor}");
            let (columns, rows) = ((width * across * factor).ceil(), (height * down * factor).ceil());
            #[expect(clippy::cast_precision_loss, reason = "a power of two")]
            let limit = MAX_PIXELS as f64;
            assert!(
                columns * rows <= limit,
                "{width} x {height} at {across}, {down}: {columns} x {rows}"
            );
        }
        assert!((capped(rect(10.0, 10.0), (2.0, 2.0)) - 1.0).abs() < f64::EPSILON);
    }

    /// The largest scale along each axis of the box is at a corner: a dense walk over the fitted box finds none above it.
    #[test]
    fn the_largest_scale_of_a_perspective_is_at_a_corner() {
        let quads = [
            [[26.0, 6.0], [38.0, 6.0], [58.0, 58.0], [6.0, 58.0]],
            [[11.0, 7.0], [50.0, 15.0], [58.0, 55.0], [5.0, 44.0]],
            [[0.0, 0.0], [90.0, 10.0], [60.0, 40.0], [5.0, 30.0]],
            [[10.0, 50.0], [60.0, 40.0], [55.0, 5.0], [8.0, 20.0]],
        ];
        for corners in quads {
            let quad = Quad::new(corners).unwrap_or_else(|_| unreachable!());
            let layout = Layout::new(
                None,
                Placement::Perspective(quad),
                rect(20.0, 12.0),
                Scale::of_canvas(64, 64, 64, 64),
            );
            let (across, down) = layout.largest_scales(&quad);
            for step_u in 0..=40 {
                for step_v in 0..=40 {
                    let at = (f64::from(step_u) / 40.0, f64::from(step_v) / 40.0);
                    let (a, d) = layout.scales_at(&quad, at);
                    assert!(
                        a <= across * (1.0 + 1e-9) && d <= down * (1.0 + 1e-9),
                        "{at:?}: ({a}, {d}) above ({across}, {down})"
                    );
                }
            }
        }
    }

    /// The cut is the root of the bound, not near it: at the factor it gives, `(w a k + 1)(h d k + 1)` is the limit, to the
    /// rounding of the sum, and a hair more factor passes it. (With the bound off by one pixel the product is the limit plus
    /// one, which a source that allocates a pixel a side more than it asked would pass.)
    #[test]
    fn the_cut_is_tight_to_the_bound_of_the_ceilings() {
        #[expect(clippy::cast_precision_loss, reason = "a power of two")]
        let limit = MAX_PIXELS as f64;
        for (width, height, across, down) in [
            (1000.0, 1000.0, 100.0, 100.0),
            (3.0, 7.0, 1e7, 1e6),
            (4000.0, 3000.0, 40.0, 10.0),
            (20.0, 12.0, 5000.0, 7000.0),
        ] {
            let factor = capped(rect(width, height), (across, down));
            let bound = |k: f64| (width * across * k + 1.0) * (height * down * k + 1.0);
            assert!(
                (bound(factor) - limit).abs() <= limit * 1e-12,
                "{width} x {height}: the bound is {}",
                bound(factor)
            );
            assert!(bound(factor * 1.000_001) > limit);
        }
    }

    /// The scale along each axis of the box is the length of the column of the output's derivative by that axis: here by a
    /// central difference of the position of a point of the box on the output, with a fit that stretches the two axes by
    /// different factors (2 along x, 1 along y), so that an axis taking the other's factor is seen.
    #[test]
    fn the_scales_of_a_perspective_are_the_derivatives_of_the_position_of_a_point_of_the_box() {
        let quad = Quad::new([[11.0, 7.0], [50.0, 15.0], [58.0, 55.0], [5.0, 44.0]]).unwrap_or_else(|_| unreachable!());
        let fit = Fitting::new([0.0, 0.0, 40.0, 10.0], Fit::Stretch, [0.5, 0.5]).ok();
        let layout = Layout::new(
            fit,
            Placement::Perspective(quad),
            rect(20.0, 10.0),
            Scale::of_canvas(64, 64, 64, 64),
        );
        let m = quad.matrix();
        // the output position of the point (x, y) of the box, whose own size is 20 by 10 and fitted size 40 by 10
        let position = |x: f64, y: f64| {
            let (u, v) = (x * 2.0 / 40.0, y / 10.0);
            let w = m[2][0] * u + m[2][1] * v + m[2][2];
            (
                (m[0][0] * u + m[0][1] * v + m[0][2]) / w,
                (m[1][0] * u + m[1][1] * v + m[1][2]) / w,
            )
        };
        let step = 1e-5;
        for (x, y) in [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (0.0, 10.0), (7.0, 4.0)] {
            let (right, left) = (position(x + step, y), position(x - step, y));
            let (below, above) = (position(x, y + step), position(x, y - step));
            let along_x = ((right.0 - left.0) / (2.0 * step)).hypot((right.1 - left.1) / (2.0 * step));
            let along_y = ((below.0 - above.0) / (2.0 * step)).hypot((below.1 - above.1) / (2.0 * step));
            let (u, v) = (x * 2.0 / 40.0, y / 10.0);
            let (got_x, got_y) = layout.scales_at(&quad, (u, v));
            assert!(
                (got_x - along_x).abs() <= 1e-4 * along_x && (got_y - along_y).abs() <= 1e-4 * along_y,
                "({x}, {y}): ({got_x}, {got_y}) is not ({along_x}, {along_y})"
            );
        }
    }
}
