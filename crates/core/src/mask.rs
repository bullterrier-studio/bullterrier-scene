//! Masks: what multiplies a layer's alpha. A mask is made by a layer drawn on the canvas (stage 6 of a layer) or by one of
//! the layer's own maps (stage 4, in `source/picture.rs`); each can be blurred (`feather`) and inverted.

use std::fmt;
use std::str::FromStr;

use crate::feather;
use crate::ranges::FEATHER;
use crate::render::{Target, land};
use crate::{Error, Layer};

/// What a mask reads of the layer that makes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Use {
    /// The layer's alpha: where it is drawn, the mask is 1.
    #[default]
    Alpha,
    /// The brightness of the layer's colour times its alpha: white keeps, black hides. The brightness is
    /// `0.2126 R + 0.7152 G + 0.0722 B` of the sRGB values.
    Luminance,
}

impl FromStr for Use {
    type Err = Error;

    /// `"alpha"` or `"luminance"`.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `use` for any other word.
    fn from_str(word: &str) -> Result<Self, Error> {
        match word {
            "alpha" => Ok(Self::Alpha),
            "luminance" => Ok(Self::Luminance),
            other => Err(Error::invalid(
                "use",
                format!("\"alpha\" or \"luminance\" is wanted, got {other:?}"),
            )),
        }
    }
}

/// The weights of red, green and blue in the brightness of a colour (Rec. 709, of sRGB values). In 32-bit floats they sum to
/// exactly 1 (`luminance_is_the_weighted_sum_of_the_colour_times_the_alpha` pins it), so white is 1 to the bit and, rounding
/// being monotone, no colour is brighter than 1.
const RED: f32 = 0.2126;
const GREEN: f32 = 0.7152;
const BLUE: f32 = 0.0722;

impl Use {
    /// The mask's value at a pixel of the layer: straight `[red, green, blue, alpha]`.
    fn of(self, [red, green, blue, alpha]: [f32; 4]) -> f32 {
        match self {
            Self::Alpha => alpha,
            Self::Luminance => (red * RED + green * GREEN + blue * BLUE) * alpha,
        }
    }
}

/// What a mask is made by: a layer drawn on the canvas, or the name of one of the layer's own maps. Made from either by
/// `into()`.
#[derive(Debug, Clone)]
pub struct MaskBy(pub(crate) By);

/// What a mask is made by.
#[derive(Debug, Clone)]
pub(crate) enum By {
    /// A layer drawn on the canvas as its own `fit` and `place` say (stage 6), read as [`Use`] says. It may have masks of its
    /// own and an opacity (which multiplies the mask); it has no name.
    Layer(Box<Layer>),
    /// The name of one of the layer's own maps (`Layer::map`): cut and fitted with the picture (stage 4).
    Map(String),
}

impl From<Layer> for MaskBy {
    fn from(layer: Layer) -> Self {
        Self(By::Layer(Box::new(layer)))
    }
}

impl From<&str> for MaskBy {
    fn from(name: &str) -> Self {
        Self(By::Map(name.to_owned()))
    }
}

/// A mask for [`Layer::mask`]: what it is made by, what it reads of it, how soft its edge is, and whether it is inverted.
/// Build one with [`Mask::new`] and the setters; the required argument alone converts into it (`layer.mask(by)`).
///
/// A layer's masks multiply. The mask's value is what it reads, blurred by `feather`, then inverted.
#[derive(Debug, Clone)]
#[must_use]
pub struct Mask {
    pub(crate) by: MaskBy,
    pub(crate) uses: Use,
    pub(crate) feather: f64,
    pub(crate) invert: bool,
}

impl Mask {
    /// A mask by `by`: a layer (a mask in canvas space) or the name of one of the layer's maps. It reads the alpha, is not
    /// blurred and is not inverted.
    pub fn new(by: impl Into<MaskBy>) -> Self {
        Self {
            by: by.into(),
            uses: Use::Alpha,
            feather: 0.0,
            invert: false,
        }
    }

    /// Read the mask layer as `uses` says (default [`Use::Alpha`]). A map is its own values: only `Use::Alpha`.
    pub fn using(mut self, uses: Use) -> Self {
        self.uses = uses;
        self
    }

    /// Blur the mask's edge by a Gaussian of standard deviation `value` design pixels (default 0: a hard edge), 0 to
    /// 1,000,000. The blur is of the values the mask reads, before it is inverted.
    pub fn feather(mut self, value: f64) -> Self {
        self.feather = value;
        self
    }

    /// Make the mask 1 minus what it would be, after the feather: type that passes behind a subject is masked by the inverted
    /// subject.
    pub fn invert(mut self) -> Self {
        self.invert = true;
        self
    }

    /// This mask's feather, checked against its range.
    pub(crate) fn checked(&self) -> Result<(), Error> {
        FEATHER.check("feather", self.feather).map(|_| ())
    }
}

impl From<Layer> for Mask {
    fn from(layer: Layer) -> Self {
        Self::new(layer)
    }
}

impl From<&str> for Mask {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

/// The name of the `number`th mask call of a layer, from 1: `mask`, `mask#2`...
pub(crate) fn label(number: usize) -> String {
    if number == 1 {
        "mask".to_owned()
    } else {
        format!("mask#{number}")
    }
}

/// A mask by a layer drawn on the canvas, as a layer holds it.
#[derive(Clone)]
pub(crate) struct CanvasMask {
    layer: Layer,
    uses: Use,
    feather: f64,
    invert: bool,
    /// The place among the layer's `mask` calls, from 1.
    number: usize,
}

impl fmt::Debug for CanvasMask {
    /// Not the mask layer: masks nest, and a layer used twice would print twice at every level.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "CanvasMask {{ number: {}, uses: {:?}, feather: {}, invert: {}, depth: {} }}",
            self.number, self.uses, self.feather, self.invert, self.layer.depth
        )
    }
}

impl CanvasMask {
    /// The mask `mask` makes by `layer`, the `number`th mask call.
    pub(crate) fn new(layer: Layer, mask: &Mask, number: usize) -> Self {
        Self {
            layer,
            uses: mask.uses,
            feather: mask.feather,
            invert: mask.invert,
            number,
        }
    }

    /// The values of this mask over `target`: the layer drawn on it, read, blurred, inverted.
    fn plane(&self, target: &Target) -> Result<Plane, Error> {
        let sigma = (
            self.feather * target.scale.x.pixels(1.0),
            self.feather * target.scale.y.pixels(1.0),
        );
        let reach = (feather::reach(sigma.0), feather::reach(sigma.1));
        feather::fits("feather", target.size(), reach)?;
        // the layer is drawn as far past the target as the blur reaches, so an edge that crosses the target feathers as it
        // would on a larger canvas
        let wide = target.grown(reach);
        let factor = product(&self.layer, &wide)?;
        let landed = land(&self.layer, &wide)?;
        let opacity = self.layer.opacity;
        let mut values = vec![0.0_f32; wide.size().0 * wide.size().1];
        landed.each(&mut |at, pixel| {
            let value = self.uses.of(pixel) * opacity;
            values[at] = factor.as_ref().map_or(value, |factor| value * factor.values[at]);
        });
        let mut plane = Plane {
            size: wide.size(),
            values,
        };
        if reach != (0, 0) {
            feather::blur(&mut plane.values, plane.size, sigma);
            plane = plane.cropped(reach, target.size());
        }
        if self.invert {
            for value in &mut plane.values {
                *value = 1.0 - *value;
            }
        }
        Ok(plane)
    }
}

/// One value from 0 to 1 for each pixel of a target, row after row.
pub(crate) struct Plane {
    size: (usize, usize),
    pub(crate) values: Vec<f32>,
}

impl Plane {
    /// The part of this plane `margin` pixels in from each side, `size` pixels.
    fn cropped(&self, margin: (usize, usize), size: (usize, usize)) -> Self {
        let mut values = Vec::with_capacity(size.0 * size.1);
        for row in margin.1..margin.1 + size.1 {
            let start = row * self.size.0 + margin.0;
            values.extend_from_slice(&self.values[start..start + size.0]);
        }
        Self { size, values }
    }
}

/// What the canvas masks of `layer` multiply it by over `target`: their product, one value a pixel, or `None` for a layer
/// with none. An error from a mask's layer is addressed under the mask (`mask.size`, `mask#2.feather`).
pub(crate) fn product(layer: &Layer, target: &Target) -> Result<Option<Plane>, Error> {
    let mut product: Option<Plane> = None;
    for mask in &layer.masks {
        let plane = mask
            .plane(target)
            .map_err(|error| error.in_layer(&label(mask.number)))?;
        match &mut product {
            None => product = Some(plane),
            Some(so_far) => {
                for (value, other) in so_far.values.iter_mut().zip(&plane.values) {
                    *value *= other;
                }
            }
        }
    }
    Ok(product)
}
