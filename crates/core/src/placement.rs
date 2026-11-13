//! Where a layer's box goes on the canvas: `fit` into a box, then `place` (flip, scale, skew, rotate about an anchor, move).
//!
//! Every number is checked once, where it enters (`Layer::fit`, `Layer::place`); the renderer trusts them.

use std::str::FromStr;

use crate::Error;
use crate::geometry::{Rect, Scale};
use crate::source::{Drawn, Request};
use crate::warp::Affine;

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
                format!("how is \"cover\", \"contain\" or \"stretch\", got {other:?}"),
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

/// The values a number of a placement may take: the one place a range is written. `Layer::fit` and `Layer::place` check
/// against these and their errors name them; C6 serves `parameters()` from them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Range {
    low: f64,
    high: f64,
    unit: &'static str,
}

/// `scale` and `aspect`: a ratio.
const RATIO: Range = Range {
    low: 0.001,
    high: 1000.0,
    unit: "",
};
/// `skew_x` and `skew_y`: degrees, short of the 90 where a skew has no end.
const SKEW: Range = Range {
    low: -89.0,
    high: 89.0,
    unit: " degrees",
};
/// `dx` and `dy`: design pixels.
const TRAVEL: Range = Range {
    low: -1e6,
    high: 1e6,
    unit: " design pixels",
};
/// `anchor` and `align`: a fraction of a box.
const FRACTION: Range = Range {
    low: 0.0,
    high: 1.0,
    unit: "",
};

impl Range {
    /// `value` if it is a number within the range.
    fn check(self, field: &str, value: f64) -> Result<f64, Error> {
        if (self.low..=self.high).contains(&value) {
            Ok(value)
        } else {
            Err(Error::invalid(
                field,
                format!(
                    "{field} is a number from {} to {}{}, got {value}",
                    self.low, self.high, self.unit
                ),
            ))
        }
    }

    /// The two numbers `(x, y)` if each is within the range.
    fn check_pair(self, field: &str, (x, y): (f64, f64)) -> Result<(f64, f64), Error> {
        if (self.low..=self.high).contains(&x) && (self.low..=self.high).contains(&y) {
            Ok((x, y))
        } else {
            Err(Error::invalid(
                field,
                format!(
                    "{field} is two numbers from {} to {}{}, got ({x}, {y})",
                    self.low, self.high, self.unit
                ),
            ))
        }
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
                format!("box is (x, y, width, height) with a width and a height above 0, got {area:?}"),
            ));
        }
        Ok(Self {
            area: Rect { x, y, width, height },
            how,
            align: FRACTION.check_pair("align", (align[0], align[1]))?,
        })
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
                    format!("rotate is a number of degrees, got {angle}"),
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
                x: (own.width - width) * align.0,
                y: (own.height - height) * align.1,
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
    placed: Placed,
}

impl Layout {
    /// The layout of a source whose box is `own`, drawn on an output of scale `scale`.
    pub(crate) fn new(fit: Option<Fitting>, placed: Placed, own: Rect, scale: Scale) -> Self {
        let (window, area, factor) = fitted(own, fit);
        Self {
            scale,
            window,
            area,
            factor,
            placed,
        }
    }

    /// What the source is asked to draw: the window, at the output scale times the fit and the zoom.
    pub(crate) fn request(&self) -> Request {
        let zoom = self.placed.scale;
        Request {
            window: self.window,
            scale: self.scale.times(self.factor.0 * zoom, self.factor.1 * zoom),
        }
    }

    /// The warp of `drawn` onto the output: the map from its pixels to output pixels, and the clip in its pixels. A pixel `p`
    /// of the box is, in the fitted box, `(p - window) x factor`; then, about the anchor `a` and with `linear` the matrix of
    /// `Placed::linear`, `a + zoom x linear x (. - a)`; then moved by `(dx, dy)`; and each output axis scales by its own
    /// output scale. Each entry of the map is formed as one quotient of whole numbers where it can be (the count of pixels
    /// of the raster is the divisor), so that a map that is the identity, a move or a quarter turn has exact entries.
    pub(crate) fn warp(&self, drawn: &Drawn) -> (Affine, Rect) {
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
        let placed = self.placed;
        let linear = placed.linear();
        let axes = [self.scale.x, self.scale.y];
        // the length of a pixel of the raster along each of its axes in the fitted box, times the pixels it holds
        let extent = [self.factor.0 * covers.width, self.factor.1 * covers.height];
        let entry = |row: usize, column: usize| {
            linear[row][column] * axes[row].pixels(placed.scale * extent[column]) / counts[column]
        };
        // where the raster's top left is in the fitted box, and the anchor
        let corner = [
            (covers.x - self.window.x) * self.factor.0,
            (covers.y - self.window.y) * self.factor.1,
        ];
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
        (map, clip)
    }
}
