//! Where a layer's box goes on the canvas: `fit` into a box, then `place` (flip, scale, skew, rotate about an anchor, move).
//!
//! Every number is checked once, where it enters (`Layer::fit`, `Layer::place`); the renderer trusts them.

use std::str::FromStr;

use crate::Error;
use crate::geometry::{Axis, Rect, Scale};
use crate::parameters::{Parameter, Value};
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

impl Fit {
    /// The word `from_str` reads.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Contain => "contain",
            Self::Stretch => "stretch",
        }
    }
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
    pub(crate) area: Rect,
    pub(crate) how: Fit,
    pub(crate) align: (f64, f64),
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

/// `degrees` brought into one turn from 0 (up to, not including, 360): a direction is the same a turn further.
fn wrapped(degrees: f64) -> f64 {
    let turned = degrees.rem_euclid(360.0);
    // a value a hair below 0 comes out as 360, which is 0 of the next turn
    if turned < 360.0 { turned } else { 0.0 }
}

/// A layer's placement, every field set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placed {
    pub(crate) dx: f64,
    pub(crate) dy: f64,
    pub(crate) scale: f64,
    pub(crate) aspect: f64,
    pub(crate) rotate: f64,
    pub(crate) skew_x: f64,
    pub(crate) skew_y: f64,
    pub(crate) flip_x: bool,
    pub(crate) flip_y: bool,
    pub(crate) anchor: (f64, f64),
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
            Some(angle) => wrapped(angle),
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

impl Placed {
    /// The fields of this placement that are parameters, in order, each at its field's name. The anchor is not one.
    pub(crate) fn parameters(self) -> Vec<Parameter> {
        let default = Self::default();
        let number =
            |name: &str, meaning: &str, unit: &str, range: (f64, f64), cyclic: bool, default: f64, value: f64| {
                Parameter::number(name, meaning, unit, range, (false, cyclic), default, value)
            };
        let flag = |name: &str, meaning: &str, value: bool| Parameter::flag(name, meaning, false, value);
        vec![
            number(
                "dx",
                "how far the layer moves right",
                "px",
                TRAVEL.bounds(),
                false,
                default.dx,
                self.dx,
            ),
            number(
                "dy",
                "how far the layer moves down",
                "px",
                TRAVEL.bounds(),
                false,
                default.dy,
                self.dy,
            ),
            number(
                "scale",
                "the zoom, about the anchor",
                "ratio",
                RATIO.bounds(),
                false,
                default.scale,
                self.scale,
            ),
            number(
                "aspect",
                "the height ratio: the zoom along y is `scale` times this",
                "ratio",
                RATIO.bounds(),
                false,
                default.aspect,
                self.aspect,
            ),
            number(
                "rotate",
                "the turn, clockwise",
                "degrees",
                (0.0, 360.0),
                true,
                default.rotate,
                self.rotate,
            ),
            number(
                "skew_x",
                "the skew along x",
                "degrees",
                SKEW.bounds(),
                false,
                default.skew_x,
                self.skew_x,
            ),
            number(
                "skew_y",
                "the skew along y, after `skew_x`",
                "degrees",
                SKEW.bounds(),
                false,
                default.skew_y,
                self.skew_y,
            ),
            flag("flip_x", "mirrored left to right", self.flip_x),
            flag("flip_y", "mirrored top to bottom", self.flip_y),
        ]
    }

    /// This placement with the parameter `field` (`dx`, `flip_x`...) set to `value`, checked as `Layer::place` checks it.
    pub(crate) fn with_field(self, field: &str, value: &Value) -> Result<Self, Error> {
        let place = match field {
            "dx" => Place::new().dx(value.number(field)?),
            "dy" => Place::new().dy(value.number(field)?),
            "scale" => Place::new().scale(value.number(field)?),
            "aspect" => Place::new().aspect(value.number(field)?),
            "rotate" => Place::new().rotate(Deg(value.number(field)?)),
            "skew_x" => Place::new().skew_x(Deg(value.number(field)?)),
            "skew_y" => Place::new().skew_y(Deg(value.number(field)?)),
            "flip_x" => Place::new().flip_x(value.flag(field)?),
            "flip_y" => Place::new().flip_y(value.flag(field)?),
            other => return Err(Error::invalid(other, "this is no field of a placement")),
        };
        self.with(place)
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
    /// Where the corners of the fitted box `area` (top left, top right, bottom right, bottom left) land on the canvas, in
    /// design pixels: the point of the box `(x, y)` goes to `origin + anchor + moved + scale x linear x ((x, y) - anchor)`.
    fn corners(self, area: Rect) -> [[f64; 2]; 4] {
        let linear = self.linear();
        let anchor = [self.anchor.0 * area.width, self.anchor.1 * area.height];
        [
            [0.0, 0.0],
            [area.width, 0.0],
            [area.width, area.height],
            [0.0, area.height],
        ]
        .map(|point| {
            let turned: [f64; 2] = std::array::from_fn(|row| {
                (0..2)
                    .map(|column| linear[row][column] * (point[column] - anchor[column]))
                    .sum()
            });
            [
                area.x + anchor[0] + self.dx + self.scale * turned[0],
                area.y + anchor[1] + self.dy + self.scale * turned[1],
            ]
        })
    }

    /// Whether the placement only moves and zooms: no flip, no `aspect`, no skew, no turn. Its map then takes a pixel of
    /// the raster to a pixel of the output (at the zoom the source was asked to draw at), so that a raster put on whole pixels
    /// is copied, not resampled.
    #[expect(
        clippy::float_cmp,
        reason = "a placement that is exactly the default for these fields"
    )]
    fn keeps_pixels(self) -> bool {
        !self.flip_x
            && !self.flip_y
            && self.aspect == 1.0
            && self.skew_x == 0.0
            && self.skew_y == 0.0
            && self.rotate.rem_euclid(360.0) == 0.0
    }

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

/// An extra `place` (a reference's, applied after the placement of the layer it names) made concrete for one layer: the map
/// of design pixels of the canvas `p -> matrix x p + at`, about the pivot the bounding rectangle of what came before gave.
#[derive(Debug, Clone, Copy)]
struct Step {
    /// The zoom times the flip, aspect, skew and turn of the extra place.
    matrix: [[f64; 2]; 2],
    /// The zoom alone: the factor the drawn density follows.
    zoom: f64,
    at: [f64; 2],
    /// Whether it only moves and zooms.
    keeps_pixels: bool,
}

impl Step {
    /// The extra place `extra` about the point of `bounds` its anchor names. The translation is `moved + (pivot - matrix x
    /// pivot)`, so that a move alone is its `dx`, `dy` to the bit, not the difference of two sums that hold the pivot.
    fn new(extra: Placed, bounds: Rect) -> Self {
        let pivot = [
            bounds.x + extra.anchor.0 * bounds.width,
            bounds.y + extra.anchor.1 * bounds.height,
        ];
        let linear = extra.linear();
        let matrix = linear.map(|row| row.map(|entry| extra.scale * entry));
        let at = std::array::from_fn(|row| {
            [extra.dx, extra.dy][row] + (pivot[row] - (matrix[row][0] * pivot[0] + matrix[row][1] * pivot[1]))
        });
        Self {
            matrix,
            zoom: extra.scale,
            at,
            keeps_pixels: extra.keeps_pixels(),
        }
    }

    /// The point `(x, y)` of the canvas, moved.
    fn at(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        [
            self.matrix[0][0] * x + self.matrix[0][1] * y + self.at[0],
            self.matrix[1][0] * x + self.matrix[1][1] * y + self.at[1],
        ]
    }

    /// This step as a 3 by 3 matrix of design pixels.
    fn homography(&self) -> [[f64; 3]; 3] {
        [
            [self.matrix[0][0], self.matrix[0][1], self.at[0]],
            [self.matrix[1][0], self.matrix[1][1], self.at[1]],
            [0.0, 0.0, 1.0],
        ]
    }

    /// This step as a map of output pixels, the axes `axes` scaling each design length: entry `(row, column)` is the design
    /// entry times the pixels of one design pixel along `row`, over those along `column`.
    fn in_pixels(&self, axes: [Axis; 2]) -> Affine {
        let one = [axes[0].pixels(1.0), axes[1].pixels(1.0)];
        let entry = |row: usize, column: usize| axes[row].pixels(self.matrix[row][column]) / one[column];
        Affine {
            a: entry(0, 0),
            b: entry(0, 1),
            c: axes[0].pixels(self.at[0]),
            d: entry(1, 0),
            e: entry(1, 1),
            f: axes[1].pixels(self.at[1]),
        }
    }
}

/// How the fitted box is put on the canvas, with every extra place folded in where that can be: the fields of a placement
/// (the extras follow, as steps), or the one homography of the unit square to the canvas that a perspective and its extras
/// make.
#[derive(Debug, Clone, Copy)]
enum Basis {
    Place(Placed),
    Homography([[f64; 3]; 3]),
}

/// Where one layer's box lands, for a source of a given size.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    scale: Scale,
    /// The part of the source's box that is shown.
    window: Rect,
    /// The fitted box, on the canvas.
    area: Rect,
    /// The scale `fit` gives, along each axis.
    factor: (f64, f64),
    basis: Basis,
    /// The extra places, in order; empty for a layer that is not a reference (and for a perspective, whose steps are in
    /// its homography).
    steps: Vec<Step>,
    /// How far, in output pixels, the canvas's top left lies from the output's: a part of a pixel when a scene used as a layer
    /// is drawn from a fractional place (`Target::offset`); `(0, 0)` otherwise.
    offset: (f64, f64),
}

impl Layout {
    /// The layout of a source whose box is `own`, drawn on an output of scale `scale`, with the extra places `extras` (a
    /// reference's own, in the order of its chain) applied after `placement`, each about the bounding rectangle of the
    /// fitted box as it lands so far.
    pub(crate) fn new(fit: Option<Fitting>, placement: Placement, extras: &[Placed], own: Rect, scale: Scale) -> Self {
        let (window, area, factor) = fitted(own, fit);
        let mut corners = match placement {
            Placement::Place(placed) => placed.corners(area),
            Placement::Perspective(quad) => quad.corners(),
        };
        let steps: Vec<Step> = extras
            .iter()
            .map(|&extra| {
                let (low, high) =
                    corners
                        .iter()
                        .fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(low, high), corner| {
                            (
                                std::array::from_fn(|axis| low[axis].min(corner[axis])),
                                std::array::from_fn(|axis| high[axis].max(corner[axis])),
                            )
                        });
                let step = Step::new(
                    extra,
                    Rect {
                        x: low[0],
                        y: low[1],
                        width: high[0] - low[0],
                        height: high[1] - low[1],
                    },
                );
                corners = corners.map(|corner| step.at(corner));
                step
            })
            .collect();
        match placement {
            Placement::Place(placed) => Self {
                scale,
                window,
                area,
                factor,
                basis: Basis::Place(placed),
                steps,
                offset: (0.0, 0.0),
            },
            // an affine map after a homography is one homography: its last row stays what it was, so a parallelogram stays
            // one (the exact path) and a perspective stays a perspective
            Placement::Perspective(quad) => Self {
                scale,
                window,
                area,
                factor,
                basis: Basis::Homography(
                    steps
                        .iter()
                        .fold(quad.matrix(), |matrix, step| multiply3(step.homography(), matrix)),
                ),
                steps: Vec::new(),
                offset: (0.0, 0.0),
            },
        }
    }

    /// This layout on an output whose grid starts `offset` output pixels before the canvas, which moves the pixel phase.
    pub(crate) fn offset(mut self, offset: (f64, f64)) -> Self {
        self.offset = offset;
        self
    }

    /// What the source is asked to draw: the window, at the output scale times the fit and the zoom. For a perspective, at
    /// the largest scale the quadrilateral gives any corner of the window along each axis (the near side is sharp), cut down
    /// by one factor if what the source allocates (`whole` says whether that is its whole box or the window) would pass the
    /// `limit` pixels of a layer (2^27, or the 2^26 image allows a look): the render stays valid and the near side loses sharpness.
    pub(crate) fn request(&self, own: Rect, whole: bool, limit: usize) -> Request {
        let placed = match self.basis {
            Basis::Place(placed) => placed,
            Basis::Homography(matrix) => {
                let (across, down) = self.largest_scales(&matrix);
                let part = if whole { own } else { self.window };
                let fit = capped(part, (across, down), limit);
                return Request {
                    window: self.window,
                    scale: Scale::exactly(across * fit, down * fit),
                    factor: self.factor,
                    phase: (0.0, 0.0),
                    canvas: (0.0, 0.0),
                    held: 0,
                };
            }
        };
        // the drawn density follows the composite: a halo scales with the extra places as with the layer's own
        let zoom = self.steps.iter().fold(placed.scale, |zoom, step| zoom * step.zoom);
        Request {
            window: self.window,
            scale: self.scale.times(self.factor.0 * zoom, self.factor.1 * zoom),
            factor: self.factor,
            phase: self.phase(own, placed),
            canvas: (0.0, 0.0),
            held: 0,
        }
    }

    /// Where the top left of the box `own` falls between output pixels (`Request::phase`), worked out as `warp` works out
    /// the translation of a raster whose top left is the box's: `(0, 0)` unless the placement only moves and zooms.
    fn phase(&self, own: Rect, placed: Placed) -> (f64, f64) {
        if !(placed.keeps_pixels() && self.steps.iter().all(|step| step.keeps_pixels)) {
            return (0.0, 0.0);
        }
        let corner = [
            (own.x - self.window.x) * self.factor.0,
            (own.y - self.window.y) * self.factor.1,
        ];
        let anchor = [placed.anchor.0 * self.area.width, placed.anchor.1 * self.area.height];
        let design: [f64; 2] = std::array::from_fn(|row| {
            let origin = [self.area.x, self.area.y][row];
            let moved = [placed.dx, placed.dy][row];
            origin + anchor[row] + moved + placed.scale * (corner[row] - anchor[row])
        });
        let design = self.steps.iter().fold(design, |point, step| step.at(point));
        let at = |row: usize, axis: Axis| {
            let position = axis.pixels(design[row]) + [self.offset.0, self.offset.1][row];
            position - position.floor()
        };
        (at(0, self.scale.x), at(1, self.scale.y))
    }

    /// The largest, over the four corners of the fitted box, of the output pixels a design pixel of the box has along its x
    /// axis and along its y axis under the quadrilateral: the scale is largest at a corner, since a homography's local scale
    /// changes by the last coordinate, which is linear (`the_largest_scale_of_a_perspective_is_at_a_corner` walks the box).
    fn largest_scales(&self, m: &[[f64; 3]; 3]) -> (f64, f64) {
        [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
            .into_iter()
            .map(|corner| self.scales_at(m, corner))
            .fold((0.0, 0.0), |(across, down), (x, y)| {
                (f64::max(across, x), f64::max(down, y))
            })
    }

    /// The output pixels a design pixel of the box has along x and along y at the point `(u, v)` of the fitted box (each
    /// from 0 to 1): the lengths of the columns of the Jacobian of the output with respect to the box's design pixels.
    fn scales_at(&self, m: &[[f64; 3]; 3], (u, v): (f64, f64)) -> (f64, f64) {
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
        let clip = if self.window == covers || drawn.ends_at_window {
            Rect {
                x: 0.0,
                y: 0.0,
                width: counts[0],
                height: counts[1],
            }
        } else {
            let (across, down) = (counts[0] / covers.width, counts[1] / covers.height);
            // a light layer's room is shown with the window; its pixels are the raster's own
            #[expect(clippy::cast_precision_loss, reason = "a room of at most 2^26 pixels")]
            let [top, right, bottom, left] = drawn.room.map(|side| side as f64);
            Rect {
                x: (self.window.x - covers.x) * across - left,
                y: (self.window.y - covers.y) * down - top,
                width: self.window.width * across + left + right,
                height: self.window.height * down + top + bottom,
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
        let placed = match self.basis {
            Basis::Place(placed) => placed,
            Basis::Homography(matrix) => {
                return (self.perspective(&matrix, axes, extent, corner, counts), clip);
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
        // each extra place after the layer's own, in output pixels: no steps leaves the map as it was
        let map = self.steps.iter().fold(map, |map, step| map.then(step.in_pixels(axes)));
        (Map::Affine(map), clip)
    }

    /// The map of a perspective: raster pixel to the point of the fitted box (a unit square), the box's homography to the
    /// quadrilateral, and the output scale of each axis. A parallelogram is affine, and its entries are formed as those of
    /// `place` are, so that an axis-aligned one that `place` can make gives the same map bit for bit.
    fn perspective(
        &self,
        m: &[[f64; 3]; 3],
        axes: [Axis; 2],
        extent: [f64; 2],
        corner: [f64; 2],
        counts: [f64; 2],
    ) -> Map {
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
        Map::Projective(Projective(multiply3(to_output, multiply3(*m, to_square))))
    }
}

/// The factor, at most 1, by which a part `part` of a box drawn at the scales `(across, down)` must be made smaller to have
/// no more than `limit` pixels: the ceilings add at most a pixel to each side, so `(w k a + 1)(h k d + 1)` is the bound, and `k`
/// is the root of that quadratic.
fn capped(part: Rect, (across, down): (f64, f64), limit: usize) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "the limit is a power of two: exact")]
    let limit = limit as f64;
    let (wide, tall) = (part.width * across, part.height * down);
    let (square, sides) = (wide * tall, wide + tall);
    // wide tall k^2 + (wide + tall) k + 1 - limit = 0
    let root = (-sides + (sides * sides + 4.0 * square * (limit - 1.0)).sqrt()) / (2.0 * square);
    root.min(1.0)
}

// Inline, because no public route reaches these: a render shows the scale a source is asked for only through its pixels.
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{Fit, Fitting, Layout, Place, Placed, Placement, capped};
    use crate::Raster;
    use crate::geometry::{MAX_PIXELS, Rect, Scale};
    use crate::perspective::Quad;
    use crate::placement::Deg;
    use crate::source::Drawn;
    use crate::warp::{Affine, Map};

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
            let factor = capped(rect(width, height), (across, down), MAX_PIXELS);
            assert!(factor > 0.0 && factor <= 1.0, "{factor}");
            let (columns, rows) = ((width * across * factor).ceil(), (height * down * factor).ceil());
            #[expect(clippy::cast_precision_loss, reason = "a power of two")]
            let limit = MAX_PIXELS as f64;
            assert!(
                columns * rows <= limit,
                "{width} x {height} at {across}, {down}: {columns} x {rows}"
            );
        }
        assert!((capped(rect(10.0, 10.0), (2.0, 2.0), MAX_PIXELS) - 1.0).abs() < f64::EPSILON);
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
                &[],
                rect(20.0, 12.0),
                Scale::of_canvas(64, 64, 64, 64),
            );
            let (across, down) = layout.largest_scales(&quad.matrix());
            for step_u in 0..=40 {
                for step_v in 0..=40 {
                    let at = (f64::from(step_u) / 40.0, f64::from(step_v) / 40.0);
                    let (a, d) = layout.scales_at(&quad.matrix(), at);
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
            let factor = capped(rect(width, height), (across, down), MAX_PIXELS);
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
            &[],
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
            let (got_x, got_y) = layout.scales_at(&quad.matrix(), (u, v));
            assert!(
                (got_x - along_x).abs() <= 1e-4 * along_x && (got_y - along_y).abs() <= 1e-4 * along_y,
                "({x}, {y}): ({got_x}, {got_y}) is not ({along_x}, {along_y})"
            );
        }
    }

    // --- extra places: a reference's, applied after the placement of the layer it names -----------------------------

    /// A placement with the fields `fields` sets.
    fn placed(fields: impl FnOnce(Place) -> Place) -> Placed {
        Placed::default()
            .with(fields(Place::new()))
            .unwrap_or_else(|_| unreachable!())
    }

    /// A layout of a 20 by 10 box on a canvas of the same size drawn at one pixel to the design pixel.
    fn layout(base: Placement, extras: &[Placed]) -> Layout {
        Layout::new(None, base, extras, rect(20.0, 10.0), Scale::of_canvas(20, 10, 20, 10))
    }

    /// The warp of the whole 20 by 10 raster: its map.
    fn map_of(layout: &Layout) -> Map {
        let drawn = Drawn {
            raster: Arc::new(Raster::made(20, 10, vec![0.0; 20 * 10 * 4])),
            covers: rect(20.0, 10.0),
            ends_at_window: false,
            room: [0; 4],
            maps: std::collections::BTreeMap::new(),
        };
        layout.warp(&drawn).0
    }

    fn affine(layout: &Layout) -> Affine {
        match map_of(layout) {
            Map::Affine(map) => map,
            Map::Projective(_) => unreachable!("an affine placement made a homography"),
        }
    }

    /// The point `(x, y)` turned clockwise by `degrees` about `about`, worked out here with the sine and cosine of the angle.
    fn turned((x, y): (f64, f64), degrees: f64, about: (f64, f64)) -> (f64, f64) {
        let (sine, cosine) = degrees.to_radians().sin_cos();
        let (dx, dy) = (x - about.0, y - about.1);
        (about.0 + cosine * dx - sine * dy, about.1 + sine * dx + cosine * dy)
    }

    const CORNERS: [(f64, f64); 4] = [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (0.0, 10.0)];

    /// An extra turn is about the bounding rectangle of the layer's box as the named layer's placement leaves it (a tilted
    /// quadrilateral), not about the box: here the extra's anchor is that rectangle's top left corner, and each corner of the
    /// box lands where turning, by hand, the first placement's corners about that point puts it.
    #[test]
    fn an_extra_turn_is_about_the_bounding_rectangle_of_the_placed_box() {
        let base = placed(|place| place.rotate(Deg(30.0)).dx(3.0));
        let extra = placed(|place| place.rotate(Deg(40.0)).anchor(0.0, 0.0).dy(2.5));
        let map = affine(&layout(Placement::Place(base), &[extra]));
        let tilted = CORNERS.map(|corner| {
            let (x, y) = turned(corner, 30.0, (10.0, 5.0));
            (x + 3.0, y)
        });
        let left = tilted.iter().map(|corner| corner.0).fold(f64::INFINITY, f64::min);
        let top = tilted.iter().map(|corner| corner.1).fold(f64::INFINITY, f64::min);
        for (corner, tilt) in CORNERS.into_iter().zip(tilted) {
            let (x, y) = turned(tilt, 40.0, (left, top));
            let (got_x, got_y) = map.at(corner.0, corner.1);
            assert!(
                (got_x - x).abs() < 1e-9 && (got_y - (y + 2.5)).abs() < 1e-9,
                "{corner:?}: ({got_x}, {got_y}) is not ({x}, {})",
                y + 2.5
            );
        }
    }

    /// A quarter turn after a quarter turn is a half turn, bit for bit, and a move by whole pixels after one is the move of
    /// their sum: the entries of the map are those of the single placement.
    #[test]
    fn exact_places_compose_to_the_exact_place_of_their_sum() {
        let half = affine(&layout(Placement::Place(placed(|place| place.rotate(Deg(180.0)))), &[]));
        let quarters = affine(&layout(
            Placement::Place(placed(|place| place.rotate(Deg(90.0)))),
            &[placed(|place| place.rotate(Deg(90.0)))],
        ));
        assert_eq!(quarters, half);
        let one_move = affine(&layout(Placement::Place(placed(|place| place.dx(7.0).dy(1.0))), &[]));
        let two_moves = affine(&layout(
            Placement::Place(placed(|place| place.dx(3.0).dy(2.0))),
            &[placed(|place| place.dx(4.0).dy(-1.0))],
        ));
        assert_eq!(two_moves, one_move);
        // and a flip after a flip is no flip at all
        let none = affine(&layout(Placement::Place(placed(|place| place)), &[]));
        let flips = affine(&layout(
            Placement::Place(placed(|place| place.flip_x(true))),
            &[placed(|place| place.flip_x(true))],
        ));
        assert_eq!(flips, none);
    }

    /// A move after any place is that place and the move, to the bit, whatever the place does to the box: the pivot of the
    /// extra place (the bounding rectangle of a box scaled by 0.3, turned 17 degrees: inexact numbers) adds no rounding to a
    /// translation that is whole pixels.
    #[test]
    fn a_move_after_any_place_is_the_place_and_the_move_to_the_bit() {
        // a spread of inexact pivots: the sum of a whole move and a pivot, less the pivot, is the move in most of them and a
        // unit in the last place off in some
        let spread = (0..200).map(|step| {
            let step = f64::from(step);
            placed(|place| {
                place
                    .scale(0.3 + 0.0137 * step)
                    .rotate(Deg(step * 1.7))
                    .dx(0.1 * step)
                    .dy(0.37 * step)
            })
        });
        for base in spread.chain([
            placed(|place| place.scale(0.3).dx(1.0)),
            placed(|place| place.skew_x(Deg(33.0)).aspect(0.7)),
        ]) {
            let alone = affine(&layout(Placement::Place(base), &[]));
            let moved = affine(&layout(
                Placement::Place(base),
                &[placed(|place| place.dx(4.0).dy(-2.0))],
            ));
            assert_eq!(
                (moved.a, moved.b, moved.d, moved.e),
                (alone.a, alone.b, alone.d, alone.e)
            );
            assert_eq!((moved.c, moved.f), (alone.c + 4.0, alone.f - 2.0));
        }
    }

    /// An affine map after a perspective is one homography: the box's corners land on the quadrilateral's corners moved by
    /// the extra place, which is worked out by hand here; and after a parallelogram the map stays affine.
    #[test]
    fn an_extra_place_after_a_perspective_is_one_homography() {
        let corners = [[5.0, 2.0], [18.0, 4.0], [16.0, 9.0], [3.0, 8.0]];
        let quad = Quad::new(corners).unwrap_or_else(|_| unreachable!());
        let extra = placed(|place| place.scale(0.5).rotate(Deg(20.0)).dx(4.0).anchor(1.0, 1.0));
        let Map::Projective(homography) = map_of(&layout(Placement::Perspective(quad), &[extra])) else {
            unreachable!("a perspective with an extra place is no longer a perspective")
        };
        let (high_x, high_y) = (
            corners.iter().map(|c| c[0]).fold(f64::NEG_INFINITY, f64::max),
            corners.iter().map(|c| c[1]).fold(f64::NEG_INFINITY, f64::max),
        );
        let pivot = (high_x, high_y);
        for (corner, at) in CORNERS.into_iter().zip(corners) {
            let (x, y) = turned((at[0], at[1]), 20.0, pivot);
            // the zoom is about the pivot too: p -> pivot + 0.5 (turned p - pivot), then moved by dx
            let want = (pivot.0 + 0.5 * (x - pivot.0) + 4.0, pivot.1 + 0.5 * (y - pivot.1));
            let m = homography.0;
            let w = m[2][0] * corner.0 + m[2][1] * corner.1 + m[2][2];
            let got = (
                (m[0][0] * corner.0 + m[0][1] * corner.1 + m[0][2]) / w,
                (m[1][0] * corner.0 + m[1][1] * corner.1 + m[1][2]) / w,
            );
            assert!(
                (got.0 - want.0).abs() < 1e-9 && (got.1 - want.1).abs() < 1e-9,
                "{corner:?}: {got:?} is not {want:?}"
            );
        }
        let parallelogram =
            Quad::new([[2.0, 1.0], [14.0, 3.0], [16.0, 9.0], [4.0, 7.0]]).unwrap_or_else(|_| unreachable!());
        assert!(matches!(
            map_of(&layout(Placement::Perspective(parallelogram), &[extra])),
            Map::Affine(_)
        ));
    }

    /// What the source is asked to draw follows the composite: the zoom of the extra place multiplies the layer's own, for an
    /// affine placement and for a perspective.
    #[test]
    fn the_drawn_density_follows_the_composite_zoom() {
        let density = |layout: &Layout| layout.request(rect(20.0, 10.0), false, MAX_PIXELS).scale.x.pixels(1.0);
        let base = layout(Placement::Place(placed(|place| place.scale(0.5))), &[]);
        let both = layout(
            Placement::Place(placed(|place| place.scale(0.5))),
            &[
                placed(|place| place.scale(3.0)),
                placed(|place| place.scale(2.0).rotate(Deg(10.0))),
            ],
        );
        assert!((density(&both) / density(&base) - 6.0).abs() < 1e-12);
        let quad = Quad::new([[5.0, 2.0], [18.0, 4.0], [16.0, 9.0], [3.0, 8.0]]).unwrap_or_else(|_| unreachable!());
        let flat = layout(Placement::Perspective(quad), &[]);
        let doubled = layout(
            Placement::Perspective(quad),
            &[placed(|place| place.scale(2.0).rotate(Deg(35.0)))],
        );
        assert!((density(&doubled) / density(&flat) - 2.0).abs() < 1e-9);
    }

    /// Where the box's top left falls between pixels includes the extra places: 0.25 and 0.5 of a pixel add to 0.75, and a
    /// whole pixel after them takes none away.
    #[test]
    fn the_phase_of_a_move_includes_the_extra_moves() {
        let phase = |extras: &[Placed]| {
            layout(Placement::Place(placed(|place| place.dx(0.25))), extras)
                .request(rect(20.0, 10.0), false, MAX_PIXELS)
                .phase
        };
        assert_eq!(phase(&[]), (0.25, 0.0));
        assert_eq!(phase(&[placed(|place| place.dx(0.5))]), (0.75, 0.0));
        assert_eq!(
            phase(&[placed(|place| place.dx(0.5)), placed(|place| place.dx(3.0).dy(-1.0))]),
            (0.75, 0.0)
        );
        // a turned extra place gives the warp the pixels: no phase
        assert_eq!(phase(&[placed(|place| place.rotate(Deg(10.0)))]), (0.0, 0.0));
    }
}
