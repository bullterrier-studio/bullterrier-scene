//! How a layer joins what is below it: the blend modes of the W3C Compositing and Blending Level 1 specification, by their
//! CSS names, and the compositing equation that applies them.

use std::fmt;
use std::str::FromStr;

use crate::Error;
use crate::suggest::nearest;

/// How a layer joins the layers below it: the W3C compositing modes, by their CSS names. Colours are blended as sRGB values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blend {
    /// `"normal"`: the layer covers what is below it in proportion to its alpha.
    #[default]
    Normal,
    /// `"multiply"`: darkens; white leaves the backdrop as it is, black gives black.
    Multiply,
    /// `"screen"`: lightens; black leaves the backdrop as it is, white gives white.
    Screen,
    /// `"overlay"`: multiplies the dark parts of the backdrop and screens the light ones.
    Overlay,
    /// `"darken"`: the darker of the two colours in each channel.
    Darken,
    /// `"lighten"`: the lighter of the two colours in each channel.
    Lighten,
    /// `"color-dodge"`: brightens the backdrop by the layer.
    ColorDodge,
    /// `"color-burn"`: darkens the backdrop by the layer.
    ColorBurn,
    /// `"hard-light"`: multiplies where the layer is dark and screens where it is light.
    HardLight,
    /// `"soft-light"`: darkens or lightens the backdrop by the layer, gently.
    SoftLight,
    /// `"difference"`: the distance between the two colours in each channel.
    Difference,
    /// `"exclusion"`: like difference, with less contrast.
    Exclusion,
    /// `"hue"`: the layer's hue with the backdrop's saturation and brightness.
    Hue,
    /// `"saturation"`: the layer's saturation with the backdrop's hue and brightness.
    Saturation,
    /// `"color"`: the layer's hue and saturation with the backdrop's brightness.
    Color,
    /// `"luminosity"`: the layer's brightness with the backdrop's hue and saturation.
    Luminosity,
}

/// The CSS name of each mode, in the order of the specification.
const NAMES: [(&str, Blend); 16] = [
    ("normal", Blend::Normal),
    ("multiply", Blend::Multiply),
    ("screen", Blend::Screen),
    ("overlay", Blend::Overlay),
    ("darken", Blend::Darken),
    ("lighten", Blend::Lighten),
    ("color-dodge", Blend::ColorDodge),
    ("color-burn", Blend::ColorBurn),
    ("hard-light", Blend::HardLight),
    ("soft-light", Blend::SoftLight),
    ("difference", Blend::Difference),
    ("exclusion", Blend::Exclusion),
    ("hue", Blend::Hue),
    ("saturation", Blend::Saturation),
    ("color", Blend::Color),
    ("luminosity", Blend::Luminosity),
];

impl Blend {
    /// The CSS names of the modes, in the order of the specification: the choices of the `blend` parameter.
    pub(crate) fn names() -> [&'static str; 16] {
        NAMES.map(|(name, _)| name)
    }
}

impl fmt::Display for Blend {
    /// The CSS name: what [`Blend::from_str`](std::str::FromStr::from_str) reads.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // NAMES is in the order of the variants (every_mode_prints_its_css_name parses each name and prints it back)
        formatter.write_str(NAMES[*self as usize].0)
    }
}

/// The names to suggest for `word`, which is no mode: by the one rule of [`crate::suggest`], after lower-casing it and reading `_` as
/// `-` (a slip of case or underscore).
fn suggestions(word: &str) -> Vec<&'static str> {
    let plain = word.to_ascii_lowercase().replace('_', "-");
    nearest(&plain, NAMES.iter().map(|(name, _)| *name))
}

impl FromStr for Blend {
    type Err = Error;

    /// A CSS name: `"normal"`, `"multiply"`, `"screen"`, `"overlay"`, `"darken"`, `"lighten"`, `"color-dodge"`,
    /// `"color-burn"`, `"hard-light"`, `"soft-light"`, `"difference"`, `"exclusion"`, `"hue"`, `"saturation"`, `"color"`
    /// or `"luminosity"`, in lower case.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `blend` for any other word, listing the names and suggesting the ones it may have meant: those
    /// that begin with the word (lower case, `_` read as `-`), else those fewest edits away if within a third of its letters
    /// (at least one), else none.
    fn from_str(word: &str) -> Result<Self, Error> {
        if let Some((_, blend)) = NAMES.iter().find(|(name, _)| *name == word) {
            return Ok(*blend);
        }
        let names: Vec<&str> = NAMES.iter().map(|(name, _)| *name).collect();
        let quoted: Vec<String> = suggestions(word).iter().map(|name| format!("{name:?}")).collect();
        let hint = match quoted.as_slice() {
            [] => String::new(),
            [one] => format!(" (did you mean {one}?)"),
            several => format!(" (did you mean one of {}?)", several.join(", ")),
        };
        Err(Error::invalid(
            "blend",
            format!("one of {} is wanted{hint}, got {word:?}", names.join(", ")),
        ))
    }
}

impl Blend {
    /// `B(Cb, Cs)` of the specification: the colour a source `source` takes where it lies over a backdrop `backdrop`, both
    /// straight sRGB values from 0 to 1, when both are opaque.
    #[inline]
    fn mix(self, backdrop: [f32; 3], source: [f32; 3]) -> [f32; 3] {
        match self {
            Self::Hue => set_lum(set_sat(source, sat(backdrop)), lum(backdrop)),
            Self::Saturation => set_lum(set_sat(backdrop, sat(source)), lum(backdrop)),
            Self::Color => set_lum(source, lum(backdrop)),
            Self::Luminosity => set_lum(backdrop, lum(source)),
            _ => [0, 1, 2].map(|channel| self.separable(backdrop[channel], source[channel])),
        }
    }

    /// `B(cb, cs)` of one channel for a separable mode (specification, section 10.1).
    #[inline]
    fn separable(self, cb: f32, cs: f32) -> f32 {
        match self {
            Self::Multiply => cb * cs,
            Self::Screen => screen(cb, cs),
            // overlay is hard-light with the two swapped
            Self::Overlay => hard_light(cs, cb),
            Self::Darken => cb.min(cs),
            Self::Lighten => cb.max(cs),
            Self::ColorDodge => {
                if cb <= 0.0 {
                    0.0
                } else if cs >= 1.0 {
                    1.0
                } else {
                    (cb / (1.0 - cs)).min(1.0)
                }
            }
            Self::ColorBurn => {
                if cb >= 1.0 {
                    1.0
                } else if cs <= 0.0 {
                    0.0
                } else {
                    1.0 - ((1.0 - cb) / cs).min(1.0)
                }
            }
            Self::HardLight => hard_light(cb, cs),
            Self::SoftLight => {
                if cs <= 0.5 {
                    cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
                } else {
                    // D(x) of the specification
                    let d = if cb <= 0.25 {
                        ((16.0 * cb - 12.0) * cb + 4.0) * cb
                    } else {
                        cb.sqrt()
                    };
                    cb + (2.0 * cs - 1.0) * (d - cb)
                }
            }
            Self::Difference => (cb - cs).abs(),
            Self::Exclusion => cb + cs - 2.0 * cb * cs,
            // normal, and the modes that mix the channels (`mix` never asks)
            Self::Normal | Self::Hue | Self::Saturation | Self::Color | Self::Luminosity => cs,
        }
    }
}

#[inline]
fn screen(cb: f32, cs: f32) -> f32 {
    cb + cs - cb * cs
}

#[inline]
fn hard_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        cb * 2.0 * cs
    } else {
        screen(cb, 2.0 * cs - 1.0)
    }
}

/// The brightness of a colour, `Lum(C)` of the specification (section 10.2): 0.3 R + 0.59 G + 0.11 B. These are the
/// specification's weights, not the Rec. 709 ones a luminance mask reads.
///
/// A number from 0 to 1 for channels from 0 to 1, by construction: every rounding is monotone, so the sum of channels at most 1
/// is at most the sum for white, which is exactly 1 in 32 bits (0.3 + 0.59 + 0.11 summed in this order), and channels at least 0
/// give at least 0. It is not a grey's own level, to an ulp, and nothing may rely on that: see `clip_color`.
#[inline]
fn lum([red, green, blue]: [f32; 3]) -> f32 {
    0.3 * red + 0.59 * green + 0.11 * blue
}

/// `ClipColor(C)` for a colour whose brightness is `brightness`: pulls a channel outside 0 to 1 back in along the line to the
/// brightness. The specification measures the brightness of `C` here; `set_lum` is the only caller and has just made `C`'s
/// brightness `brightness` (in real numbers: in floats it is that to an ulp), so it is given, not measured again. Then the
/// denominators are strictly positive by construction: a colour with a channel `least` below 0 has `brightness - least > 0`
/// as `brightness` is from 0 up, and one with a channel `most` above 1 has `most - brightness > 0` as `brightness` is at most
/// 1 (`lum`). Measured again, a colour that is nearly a grey and an ulp outside 0 to 1 has its measured brightness equal to its
/// least or largest channel and the quotient is 0 over 0 (`#1d1d1d` through `color` gave NaN).
#[inline]
fn clip_color(color: [f32; 3], brightness: f32) -> [f32; 3] {
    let least = color[0].min(color[1]).min(color[2]);
    let most = color[0].max(color[1]).max(color[2]);
    let mut color = color;
    if least < 0.0 {
        color = color.map(|channel| brightness + (channel - brightness) * brightness / (brightness - least));
    }
    if most > 1.0 {
        color = color.map(|channel| brightness + (channel - brightness) * (1.0 - brightness) / (most - brightness));
    }
    color
}

/// `SetLum(C, l)`: the colour moved to brightness `l`, clipped.
#[inline]
fn set_lum(color: [f32; 3], brightness: f32) -> [f32; 3] {
    let shift = brightness - lum(color);
    clip_color(color.map(|channel| channel + shift), brightness)
}

/// `Sat(C)`: the spread of a colour, its largest channel less its smallest.
#[inline]
fn sat(color: [f32; 3]) -> f32 {
    color[0].max(color[1]).max(color[2]) - color[0].min(color[1]).min(color[2])
}

/// `SetSat(C, s)`: the colour with its spread made `s`: the smallest channel 0, the largest `s`, the middle one in
/// proportion; a colour with no spread is black.
#[inline]
fn set_sat(color: [f32; 3], spread: f32) -> [f32; 3] {
    let mut order = [0, 1, 2];
    order.sort_by(|&a, &b| color[a].total_cmp(&color[b]));
    let [least, middle, most] = order;
    let mut set = [0.0; 3];
    if color[most] > color[least] {
        set[middle] = (color[middle] - color[least]) * spread / (color[most] - color[least]);
        set[most] = spread;
    }
    set
}

/// `source` (straight colour, with `alpha`) laid over `below` (premultiplied red, green, blue and alpha) by `blend`: the
/// general compositing equation of the specification (section 9, with source-over for the alpha):
///
/// ```text
/// Cs' = (1 - ab) Cs + ab B(Cb, Cs)        the source colour, mixed with the blend where the backdrop shows
/// co  = as Cs' + (1 - as) ab Cb           premultiplied; ab Cb is the backdrop's own premultiplied colour
/// ao  = as + ab (1 - as)
/// ```
///
/// With `blend` normal this is the "over" of Porter and Duff. The backdrop's straight colour is its premultiplied one over
/// its alpha, 0 where there is none (the blend is not asked for where nothing is below).
#[inline]
pub(crate) fn join(below: &mut [f32], source: [f32; 3], alpha: f32, blend: Blend) {
    let backdrop_alpha = below[3];
    let backdrop = if backdrop_alpha > 0.0 {
        [0, 1, 2].map(|channel| below[channel] / backdrop_alpha)
    } else {
        [0.0; 3]
    };
    let mixed = blend.mix(backdrop, source);
    let (alone, together, under) = ((1.0 - backdrop_alpha) * alpha, alpha * backdrop_alpha, 1.0 - alpha);
    let result_alpha = alpha + backdrop_alpha * under;
    for channel in 0..3 {
        // in real numbers a premultiplied colour is between 0 and its alpha (the clip of the non-separable modes lands on 0
        // or 1 exactly); rounding can miss by a few ulps, and the straight backdrop of the next layer is this over its alpha
        below[channel] =
            (alone * source[channel] + together * mixed[channel] + under * below[channel]).clamp(0.0, result_alpha);
    }
    below[3] = result_alpha;
}

/// A light layer's pixel (stage 7): `pixel`, straight RGBA, as its colour over black (its premultiplied colour), whose
/// alpha is the largest of red, green and blue of that colour; the straight colour is the colour over that alpha, 0 where
/// there is none. Returns the straight colour and the alpha.
#[inline]
pub(crate) fn lit([red, green, blue, alpha]: [f32; 4]) -> ([f32; 3], f32) {
    let over_black = [red * alpha, green * alpha, blue * alpha];
    let brightest = over_black[0].max(over_black[1]).max(over_black[2]);
    if brightest > 0.0 {
        // each channel is at most the brightest, so the quotient is at most 1
        (over_black.map(|channel| channel / brightest), brightest)
    } else {
        ([0.0; 3], 0.0)
    }
}
