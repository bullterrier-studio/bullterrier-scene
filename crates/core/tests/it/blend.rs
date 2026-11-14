//! Blend modes: how a layer joins what is below it. The separable modes are checked against the pinned vector crate, which
//! blends sRGB values by the same W3C formulas, at the colours and alphas of a grid that crosses every branch; the
//! non-separable ones against constants worked out by hand from the specification's pseudo-code; the rest by properties
//! and by closed forms of the compositing equation worked out by hand.

use bullterrier_scene::{Blend, Error, Layer, Place, Raster};
use bullterrier_vector as vector;
use proptest::prelude::*;

use crate::common::{Outcome, close, exactly, pixel, scene};

/// The twelve modes vector has, by CSS name.
const SEPARABLE: [&str; 12] = [
    "normal",
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "color-dodge",
    "color-burn",
    "hard-light",
    "soft-light",
    "difference",
    "exclusion",
];

/// Every mode, by CSS name.
const ALL: [&str; 16] = [
    "normal",
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "color-dodge",
    "color-burn",
    "hard-light",
    "soft-light",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
];

fn mode(word: &str) -> Result<Blend, Error> {
    word.parse()
}

/// A layer of one straight RGBA pixel, exact in a float, on a canvas of one pixel.
fn cell(rgba: [f32; 4]) -> Result<Layer, Error> {
    Ok(Layer::picture(Raster::rgba(1, 1, rgba.to_vec())?))
}

/// What the layers give on a canvas of one pixel (straight RGBA).
fn one(layers: impl IntoIterator<Item = Layer>) -> Result<[f32; 4], Error> {
    Ok(pixel(&scene(1, 1, layers)?.render(1)?, 0, 0))
}

/// `rgb` as the bytes over 255 that a `#rrggbb` fill is.
fn levels(bytes: [u8; 3]) -> [f32; 3] {
    bytes.map(|byte| f32::from(byte) / 255.0)
}

fn hex(bytes: [u8; 3], alpha: u8) -> String {
    format!("#{:02x}{:02x}{:02x}{alpha:02x}", bytes[0], bytes[1], bytes[2])
}

// --- names ---

#[test]
fn every_css_name_is_a_mode_and_no_other_word_is() {
    for word in ALL {
        assert!(word.parse::<Blend>().is_ok(), "{word}");
    }
    assert_eq!(mode("normal"), Ok(Blend::Normal));
    assert_eq!(Blend::default(), Blend::Normal);
    // each word is its own mode: sixteen words, sixteen different modes
    let modes: Vec<Blend> = ALL
        .iter()
        .map(|word| mode(word))
        .collect::<Result<_, _>>()
        .unwrap_or_default();
    for (index, one) in modes.iter().enumerate() {
        assert!(!modes[..index].contains(one), "{} repeats a mode", ALL[index]);
    }
}

#[test]
fn a_word_that_is_no_mode_is_refused_naming_blend_the_names_and_the_nearest() {
    for (word, nearest) in [
        ("Multiply", "multiply"),
        ("soft_light", "soft-light"),
        ("softlight", "soft-light"),
        ("colour-dodge", "color-dodge"),
        ("multipy", "multiply"),
        ("SCREEN", "screen"),
    ] {
        let Err(Error::Invalid { address, message }) = word.parse::<Blend>() else {
            unreachable!("{word} was accepted")
        };
        assert_eq!(address, "blend", "{word}");
        for name in ALL {
            assert!(message.contains(name), "{word}: {message}");
        }
        assert!(
            message.contains(&format!("did you mean {nearest:?}")),
            "{word}: {message}"
        );
        assert!(message.contains(&format!("{word:?}")), "{word}: {message}");
    }
}

// --- the separable modes against vector ---

/// Colours whose channels sit on both sides of the branches of the formulas (0.25 and 0.5) and at their ends.
const BACKDROPS: [[u8; 3]; 7] = [
    [0, 64, 128],
    [63, 127, 200],
    [255, 128, 0],
    [200, 255, 63],
    [127, 0, 255],
    [64, 200, 127],
    [128, 63, 64],
];
const SOURCES: [[u8; 3]; 7] = [
    [255, 0, 127],
    [128, 200, 63],
    [0, 255, 64],
    [127, 63, 200],
    [64, 128, 255],
    [200, 64, 0],
    [63, 0, 128],
];

/// What vector draws at the middle of an opaque backdrop with a square of `source` (alpha `alpha`, opacity `opacity`) blended
/// by `word` over it: straight RGBA bytes.
fn drawn_by_vector(
    backdrop: [u8; 3],
    source: [u8; 3],
    alpha: u8,
    opacity: f64,
    word: &str,
) -> Result<[u8; 4], Box<dyn std::error::Error>> {
    let square = |left: f64, size: f64, color: &str| -> Result<vector::Drawing, vector::Error> {
        Ok(vector::Drawing::from(vector::Path::rect(left, left, size, size, 0.0)?).fill(vector::Color::parse(color)?))
    };
    let drawing = vector::Drawing::new([
        square(0.0, 12.0, &hex(backdrop, 255))?,
        square(4.0, 4.0, &hex(source, alpha))?
            .opacity(opacity)
            .blend(vector::Blend::parse(word)?),
    ]);
    let bytes = drawing.to_rgba(vector::Size::px(12.0, 12.0), &vector::Values::new())?;
    let at = (6 * 12 + 6) * 4;
    Ok([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// How far scene's float pixel may be from vector's byte, in levels of 255: the byte's own rounding (half a level) and vector's
/// one level (measured at most 1.498, `exclusion`, with an opaque source). With a translucent source vector holds the source
/// premultiplied in 8 bits, so its colour is off by up to half a level over the alpha; the modes that divide, `color-dodge` and
/// `color-burn`, turn that into 2.14 levels (alpha 0.5: 115.9 against 118, the W3C formula giving 115.86), and no other
/// mode passes 1.44.
const BOUND: f32 = 1.5;

/// The bound for a translucent source in the mode `word`.
fn translucent_bound(word: &str) -> f32 {
    if matches!(word, "color-dodge" | "color-burn") {
        2.5
    } else {
        BOUND
    }
}

/// The largest difference, in levels of 255, between scene's and vector's pixel for one mode, over the grid.
fn most_levels_off(word: &str, alpha: u8, opacity: f64, bound: f32) -> Result<f32, Box<dyn std::error::Error>> {
    let mut most = 0.0_f32;
    for backdrop in BACKDROPS {
        for source in SOURCES {
            let layers = [
                Layer::fill(&hex(backdrop, 255))?,
                Layer::fill(&hex(source, alpha))?.opacity(opacity)?.blend(mode(word)?),
            ];
            let ours = pixel(&scene(12, 12, layers)?.render(12)?, 6, 6);
            let theirs = drawn_by_vector(backdrop, source, alpha, opacity, word)?;
            assert_eq!(theirs[3], 255, "{word}");
            assert!((ours[3] - 1.0).abs() < 1e-6, "{word}: the backdrop is opaque");
            for channel in 0..3 {
                let off = (ours[channel] * 255.0 - f32::from(theirs[channel])).abs();
                assert!(
                    off <= bound,
                    "{word} {backdrop:?} {source:?} alpha {alpha} opacity {opacity}: channel {channel}: ours {} vs vector {}",
                    ours[channel] * 255.0,
                    theirs[channel]
                );
                most = most.max(off);
            }
        }
    }
    Ok(most)
}

#[test]
fn an_opaque_source_blends_as_vector_blends_it_in_every_separable_mode() -> Outcome {
    for word in SEPARABLE {
        most_levels_off(word, 255, 1.0, BOUND)?;
    }
    Ok(())
}

#[test]
fn a_translucent_source_blends_as_vector_blends_it_in_every_separable_mode() -> Outcome {
    for word in SEPARABLE {
        for (alpha, opacity) in [(128, 1.0), (255, 0.5), (204, 0.5)] {
            most_levels_off(word, alpha, opacity, translucent_bound(word))?;
        }
    }
    Ok(())
}

// --- the non-separable modes against the specification ---

/// `(mode, backdrop, source, result)`: the result worked out by hand from the pseudo-code of section 10.2 of the W3C
/// Compositing and Blending Level 1 specification (Lum = 0.3 R + 0.59 G + 0.11 B; `ClipColor`; `SetLum`; `Sat`; `SetSat`; hue =
/// `SetLum`(SetSat(Cs, Sat(Cb)), Lum(Cb)); saturation = SetLum(SetSat(Cb, Sat(Cs)), Lum(Cb)); color = SetLum(Cs, Lum(Cb));
/// luminosity = SetLum(Cb, Lum(Cs))), in a separate script in 64-bit floats, rounded to 7 places. The pairs cross
/// the two branches of `ClipColor` (the 3rd and 4th) and the zero saturation of `SetSat` (the 5th backdrop, the 6th source).
type Row = (&'static str, [f32; 3], [f32; 3], [f32; 3]);
const NON_SEPARABLE: [Row; 28] = [
    ("hue", [0.8, 0.4, 0.2], [0.1, 0.5, 0.9], [0.255, 0.555, 0.855]),
    ("hue", [0.2, 0.6, 0.4], [0.9, 0.3, 0.3], [0.738, 0.338, 0.338]),
    ("hue", [0.9, 0.9, 0.1], [0.2, 0.2, 0.8], [0.788_764, 0.788_764, 1.0]),
    (
        "hue",
        [0.1, 0.05, 0.0],
        [1.0, 0.9, 0.0],
        [0.071_600_5, 0.064_440_4, 0.0],
    ),
    ("hue", [0.5, 0.5, 0.5], [0.3, 0.7, 0.1], [0.5, 0.5, 0.5]),
    ("hue", [0.6, 0.3, 0.9], [0.4, 0.4, 0.4], [0.456, 0.456, 0.456]),
    (
        "saturation",
        [0.8, 0.4, 0.2],
        [0.1, 0.5, 0.9],
        [0.900_666_7, 0.367_333_3, 0.100_666_7],
    ),
    ("saturation", [0.2, 0.6, 0.4], [0.9, 0.3, 0.3], [0.071, 0.671, 0.371]),
    ("saturation", [0.9, 0.9, 0.1], [0.2, 0.2, 0.8], [0.878, 0.878, 0.278]),
    ("saturation", [0.1, 0.05, 0.0], [1.0, 0.9, 0.0], [0.1, 0.05, 0.0]),
    ("saturation", [0.5, 0.5, 0.5], [0.3, 0.7, 0.1], [0.5, 0.5, 0.5]),
    ("saturation", [0.6, 0.3, 0.9], [0.4, 0.4, 0.4], [0.456, 0.456, 0.456]),
    ("color", [0.8, 0.4, 0.2], [0.1, 0.5, 0.9], [0.174, 0.574, 0.974]),
    ("color", [0.2, 0.6, 0.4], [0.9, 0.3, 0.3], [0.878, 0.278, 0.278]),
    ("color", [0.9, 0.9, 0.1], [0.2, 0.2, 0.8], [0.788_764, 0.788_764, 1.0]),
    (
        "color",
        [0.1, 0.05, 0.0],
        [1.0, 0.9, 0.0],
        [0.071_600_5, 0.064_440_4, 0.0],
    ),
    ("color", [0.5, 0.5, 0.5], [0.3, 0.7, 0.1], [0.286, 0.686, 0.086]),
    ("color", [0.6, 0.3, 0.9], [0.4, 0.4, 0.4], [0.456, 0.456, 0.456]),
    ("luminosity", [0.8, 0.4, 0.2], [0.1, 0.5, 0.9], [0.726, 0.326, 0.126]),
    ("luminosity", [0.2, 0.6, 0.4], [0.9, 0.3, 0.3], [0.222, 0.622, 0.422]),
    (
        "luminosity",
        [0.9, 0.9, 0.1],
        [0.2, 0.2, 0.8],
        [0.298_876_4, 0.298_876_4, 0.0],
    ),
    (
        "luminosity",
        [0.1, 0.05, 0.0],
        [1.0, 0.9, 0.0],
        [0.8715, 0.8215, 0.7715],
    ),
    ("luminosity", [0.5, 0.5, 0.5], [0.3, 0.7, 0.1], [0.514, 0.514, 0.514]),
    ("luminosity", [0.6, 0.3, 0.9], [0.4, 0.4, 0.4], [0.544, 0.244, 0.844]),
    // `ClipColor` with a channel only just outside 0 to 1 (so a threshold moved off 0 or 1 shows): the brightness 0.2379 moves
    // (0.5, 0.3, 0.1) by -0.1001, to (0.3999, 0.1999, -0.0001), which the clip scales to (0.3998319, 0.199916, 0); 0.7791 moves
    // (0.9, 0.6, 0.5) by +0.1001, to (1.0001, 0.7001, 0.6001), which the clip scales to (1, 0.7001358, 0.600181)
    (
        "luminosity",
        [0.5, 0.3, 0.1],
        [0.2379, 0.2379, 0.2379],
        [0.399_831_9, 0.199_916, 0.0],
    ),
    (
        "luminosity",
        [0.9, 0.6, 0.5],
        [0.7791, 0.7791, 0.7791],
        [1.0, 0.700_135_8, 0.600_181],
    ),
    (
        "color",
        [0.2379, 0.2379, 0.2379],
        [0.5, 0.3, 0.1],
        [0.399_831_9, 0.199_916, 0.0],
    ),
    (
        "color",
        [0.7791, 0.7791, 0.7791],
        [0.9, 0.6, 0.5],
        [1.0, 0.700_135_8, 0.600_181],
    ),
];

#[test]
fn the_non_separable_modes_give_what_the_specifications_pseudo_code_gives() -> Outcome {
    for (word, backdrop, source, expected) in NON_SEPARABLE {
        let got = one([
            cell([backdrop[0], backdrop[1], backdrop[2], 1.0])?,
            cell([source[0], source[1], source[2], 1.0])?.blend(mode(word)?),
        ])?;
        assert!(
            close(got, [expected[0], expected[1], expected[2], 1.0], 2e-6),
            "{word} {backdrop:?} over {source:?}: wanted {expected:?}, got {got:?}"
        );
    }
    Ok(())
}

// --- the compositing equation ---

/// The equation of section 9 of the specification with a translucent backdrop, by hand. Multiply, backdrop (1, 0.5, 0) at
/// alpha 0.5, source (0.5, 0.5, 0.5) at alpha 0.5: B = (0.5, 0.25, 0); Cs' = (1 - 0.5) Cs + 0.5 B = (0.5, 0.375, 0.25);
/// premultiplied result = 0.5 Cs' + 0.5 Cb (1 - 0.5) = (0.5, 0.3125, 0.125); alpha = 0.5 + 0.5 (1 - 0.5) = 0.75.
/// Difference, backdrop (0.8, 0.2, 0.4) at 0.25, source (0.3, 0.6, 0.4) at 0.75: B = (0.5, 0.4, 0); Cs' = (0.35, 0.55, 0.3);
/// premultiplied result = (0.3125, 0.425, 0.25); alpha = 0.8125.
#[test]
fn a_translucent_backdrop_and_source_composite_as_the_specifications_equation_says() -> Outcome {
    let got = one([
        cell([1.0, 0.5, 0.0, 0.5])?,
        cell([0.5, 0.5, 0.5, 0.5])?.blend(Blend::Multiply),
    ])?;
    assert!(
        close(got, [0.5 / 0.75, 0.3125 / 0.75, 0.125 / 0.75, 0.75], 1e-6),
        "{got:?}"
    );
    let got = one([
        cell([0.8, 0.2, 0.4, 0.25])?,
        cell([0.3, 0.6, 0.4, 0.75])?.blend(Blend::Difference),
    ])?;
    assert!(
        close(got, [0.3125 / 0.8125, 0.425 / 0.8125, 0.25 / 0.8125, 0.8125], 1e-6),
        "{got:?}"
    );
    Ok(())
}

/// Stages 6, 8 and 9: a mask of 0.5 and an opacity of 0.5 scale the source's alpha (1) to 0.25 before it is blended.
/// Multiply of (1, 0.5, 0) over the opaque (0.5, 0.5, 1): B = (0.5, 0.25, 0); result = 0.75 Cb + 0.25 B = (0.5, 0.4375, 0.75).
#[test]
fn a_mask_and_an_opacity_scale_the_alpha_that_is_blended() -> Outcome {
    let backdrop = || cell([0.5, 0.5, 1.0, 1.0]);
    let source = || Ok::<_, Error>(cell([1.0, 0.5, 0.0, 1.0])?.blend(Blend::Multiply));
    let expected = [0.5, 0.4375, 0.75, 1.0];
    let half = || Layer::fill("#ffffff")?.opacity(0.5);
    assert!(close(
        one([backdrop()?, source()?.mask(half()?)?.opacity(0.5)?])?,
        expected,
        1e-6
    ));
    // the same alpha made by either one alone at 0.25
    assert!(close(one([backdrop()?, source()?.opacity(0.25)?])?, expected, 1e-6));
    let quarter = Layer::fill("#ffffff")?.opacity(0.25)?;
    assert!(close(one([backdrop()?, source()?.mask(quarter)?])?, expected, 1e-6));
    Ok(())
}

#[test]
fn a_blend_is_applied_where_the_layer_lands_and_at_the_alpha_the_warp_gives_its_edge() -> Outcome {
    // a white square one pixel wide, moved by half a pixel: each of the two pixels it reaches is covered by half
    let base = Layer::fill("#204060")?;
    let square = Layer::picture(Raster::rgba(1, 1, vec![1.0, 0.5, 0.0, 1.0])?)
        .place(Place::new().dx(1.5))?
        .blend(Blend::Multiply);
    let picture = scene(4, 1, [base, square])?.render(4)?;
    let backdrop = levels([0x20, 0x40, 0x60]);
    assert!(
        close(
            pixel(&picture, 0, 0),
            [backdrop[0], backdrop[1], backdrop[2], 1.0],
            1e-6
        ),
        "untouched"
    );
    assert!(
        close(
            pixel(&picture, 3, 0),
            [backdrop[0], backdrop[1], backdrop[2], 1.0],
            1e-6
        ),
        "untouched"
    );
    for x in [1, 2] {
        // half coverage: 0.5 Cb + 0.5 (Cb x Cs)
        let expected = [
            backdrop[0] * 0.5 + backdrop[0] * 1.0 * 0.5,
            backdrop[1] * 0.5 + backdrop[1] * 0.5 * 0.5,
            backdrop[2] * 0.5,
            1.0,
        ];
        assert!(
            close(pixel(&picture, x, 0), expected, 1e-5),
            "x {x}: {:?}",
            pixel(&picture, x, 0)
        );
    }
    Ok(())
}

// --- properties ---

fn channel() -> impl Strategy<Value = f32> {
    prop_oneof![Just(0.0_f32), Just(1.0_f32), Just(0.5_f32), 0.0_f32..=1.0]
}

fn colour() -> impl Strategy<Value = [f32; 3]> {
    [channel(), channel(), channel()]
}

proptest! {
    /// `normal` is the "over" of Porter and Duff, in closed form, and the same as no blend call.
    #[test]
    fn normal_is_source_over_and_the_same_as_no_call(
        cb in colour(), ab in 0.0_f32..=1.0, cs in colour(), a_s in 0.0_f32..=1.0,
    ) {
        let below = [cb[0], cb[1], cb[2], ab];
        let above = [cs[0], cs[1], cs[2], a_s];
        let plain = one([cell(below)?, cell(above)?])?;
        let said = one([cell(below)?, cell(above)?.blend(Blend::Normal)])?;
        prop_assert!(exactly(plain, said));
        let alpha = a_s + ab * (1.0 - a_s);
        prop_assert!((plain[3] - alpha).abs() < 1e-6);
        for channel in 0..3 {
            let premultiplied = cs[channel] * a_s + cb[channel] * ab * (1.0 - a_s);
            if alpha > 1e-4 {
                prop_assert!((plain[channel] - premultiplied / alpha).abs() < 1e-4, "{plain:?}");
            }
        }
    }

    /// Multiply by white and screen by black are the identity; multiply by black is black; a colour differs from itself
    /// by nothing; darken and lighten with itself change nothing.
    #[test]
    fn the_identities_and_annihilators_of_the_separable_modes(cb in colour()) {
        let below = || cell([cb[0], cb[1], cb[2], 1.0]);
        let over = |color: [f32; 3], blend| -> Result<[f32; 4], TestCaseError> {
            Ok(one([below()?, cell([color[0], color[1], color[2], 1.0])?.blend(blend)])?)
        };
        let same = [cb[0], cb[1], cb[2], 1.0];
        prop_assert!(close(over([1.0; 3], Blend::Multiply)?, same, 1e-6));
        prop_assert!(close(over([0.0; 3], Blend::Screen)?, same, 1e-6));
        prop_assert!(close(over([0.0; 3], Blend::Multiply)?, [0.0, 0.0, 0.0, 1.0], 1e-6));
        prop_assert!(close(over(cb, Blend::Difference)?, [0.0, 0.0, 0.0, 1.0], 1e-6));
        prop_assert!(close(over(cb, Blend::Darken)?, same, 1e-6));
        prop_assert!(close(over(cb, Blend::Lighten)?, same, 1e-6));
    }

    /// These six treat the two colours alike, so the order of two opaque layers does not matter.
    #[test]
    fn multiply_screen_difference_exclusion_darken_and_lighten_commute_on_opaque_colours(
        a in colour(), b in colour(),
    ) {
        for blend in [Blend::Multiply, Blend::Screen, Blend::Difference, Blend::Exclusion, Blend::Darken, Blend::Lighten] {
            let (a, b) = ([a[0], a[1], a[2], 1.0], [b[0], b[1], b[2], 1.0]);
            let one_way = one([cell(a)?, cell(b)?.blend(blend)])?;
            let other = one([cell(b)?, cell(a)?.blend(blend)])?;
            prop_assert!(close(one_way, other, 1e-6), "{blend:?}: {one_way:?} {other:?}");
        }
    }

    /// Over a backdrop with no alpha the source is what is drawn, whatever the mode; a source with no alpha changes nothing.
    #[test]
    fn a_transparent_backdrop_gives_the_source_and_a_transparent_source_leaves_the_backdrop(
        cb in colour(), cs in colour(), a_s in 0.0_f32..=1.0, ab in 0.0_f32..=1.0,
    ) {
        for word in ALL {
            let source = cell([cs[0], cs[1], cs[2], a_s])?.blend(mode(word)?);
            let got = one([cell([cb[0], cb[1], cb[2], 0.0])?, source])?;
            let wanted = if a_s > 0.0 { [cs[0], cs[1], cs[2], a_s] } else { [0.0; 4] };
            prop_assert!(close(got, wanted, 1e-5), "{word}: {got:?} vs {wanted:?}");

            let below = [cb[0], cb[1], cb[2], ab];
            let nothing = cell([cs[0], cs[1], cs[2], 0.0])?.blend(mode(word)?);
            prop_assert!(exactly(one([cell(below)?, nothing])?, one([cell(below)?])?), "{word}");
        }
    }

    /// An opacity scales the source's alpha before the blend: a source of alpha `a` at opacity `o` is a source of alpha `a o`.
    #[test]
    fn an_opacity_is_a_smaller_source_alpha(
        cb in colour(), ab in 0.0_f32..=1.0, cs in colour(), a_s in 0.0_f32..=1.0, opacity in 0.0_f64..=1.0,
    ) {
        for word in ALL {
            let below = cell([cb[0], cb[1], cb[2], ab])?;
            let faded = cell([cs[0], cs[1], cs[2], a_s])?.blend(mode(word)?).opacity(opacity)?;
            #[expect(clippy::cast_possible_truncation, reason = "an opacity is a 32-bit float in the core")]
            let thinner = cell([cs[0], cs[1], cs[2], a_s * opacity as f32])?.blend(mode(word)?);
            let (got, wanted) = (one([below.clone(), faded])?, one([below, thinner])?);
            prop_assert!(close(got, wanted, 1e-5), "{word}: {got:?} vs {wanted:?}");
        }
    }

    /// `color` of A over B is `luminosity` of B over A (the specification's Lum and SetLum swap the two).
    #[test]
    fn color_and_luminosity_are_each_others_swap(a in colour(), b in colour()) {
        let (a, b) = ([a[0], a[1], a[2], 1.0], [b[0], b[1], b[2], 1.0]);
        let color = one([cell(b)?, cell(a)?.blend(Blend::Color)])?;
        let luminosity = one([cell(a)?, cell(b)?.blend(Blend::Luminosity)])?;
        prop_assert!(close(color, luminosity, 1e-5), "{color:?} {luminosity:?}");
    }

    /// Whatever the colours, their alphas and the mode (the ends of division in color-dodge and color-burn, the roots of
    /// soft-light, the clip of the non-separable modes included), the result is a number from 0 to 1.
    #[test]
    fn every_mode_gives_numbers_from_0_to_1(
        cb in colour(), ab in prop_oneof![Just(0.0_f32), Just(1.0_f32), 0.0_f32..=1.0],
        cs in colour(), a_s in prop_oneof![Just(0.0_f32), Just(1.0_f32), 0.0_f32..=1.0],
    ) {
        for word in ALL {
            let got = one([cell([cb[0], cb[1], cb[2], ab])?, cell([cs[0], cs[1], cs[2], a_s])?.blend(mode(word)?)])?;
            prop_assert!(got.iter().all(|value| (0.0..=1.0).contains(value)), "{word}: {got:?}");
        }
    }
}

// --- numbers that are always numbers ---

/// A picture of 256 by 256 pixels whose pixel (x, y) is the grey `grey(x, y)` over 255, at `alpha`.
fn grey_plane(alpha: f32, grey: impl Fn(usize, usize) -> usize) -> Result<Layer, Error> {
    let mut values = Vec::with_capacity(256 * 256 * 4);
    for y in 0..256 {
        for x in 0..256 {
            #[expect(clippy::cast_precision_loss, reason = "a level from 0 to 255")]
            let level = grey(x, y) as f32 / 255.0;
            values.extend([level, level, level, alpha]);
        }
    }
    Ok(Layer::picture(Raster::rgba(256, 256, values)?))
}

/// Every grey over every grey, in every mode, at several alphas: the backdrop's grey runs along x and the source's along y, so
/// one render holds the whole state space of the two colours. A colour is a number from 0 to 1, no premultiplied colour passes
/// its alpha, and where there is no alpha there is no colour. (`color` and `luminosity` of a grey gave NaN: a grey's brightness
/// came out an ulp off its grey, the colour moved to it fell an ulp below 0, and the clip divided 0 by 0.)
#[test]
fn every_grey_over_every_grey_gives_numbers_in_every_mode() -> Outcome {
    for word in ALL {
        for backdrop_alpha in [0.0_f32, 0.5, 1.0] {
            for source_alpha in [0.5_f32, 1.0] {
                let layers = [
                    grey_plane(backdrop_alpha, |x, _| x)?,
                    grey_plane(source_alpha, |_, y| y)?.blend(mode(word)?),
                ];
                let picture = scene(256, 256, layers)?.render(256)?;
                for (index, got) in crate::common::pixels(&picture).enumerate() {
                    let (backdrop, source) = (index % 256, index / 256);
                    let numbers = got.iter().all(|value| (0.0..=1.0).contains(value));
                    assert!(
                        numbers,
                        "{word}: backdrop {backdrop} at {backdrop_alpha}, source {source} at {source_alpha}: {got:?}"
                    );
                    assert!(
                        got[3] > 0.0 || got[..3] == [0.0; 3],
                        "{word}: {backdrop} {source}: colour with no alpha {got:?}"
                    );
                }
            }
        }
    }
    Ok(())
}

/// The four modes that mix channels, on greys: no hue and no saturation to take, so over an opaque grey backdrop `hue`,
/// `saturation` and `color` give the backdrop (`SetSat` of a grey is black, and `SetLum` to the backdrop's brightness, a
/// grey's own level, gives the backdrop back), and `luminosity` gives the source (`SetLum(Cb, Lum(Cs))` with `Lum` of a grey
/// its level). Specification, section 10.2.
#[test]
fn the_modes_that_mix_channels_on_greys_give_the_backdrop_or_the_source_by_the_specifications_identities() -> Outcome {
    for word in ["hue", "saturation", "color", "luminosity"] {
        let layers = [
            grey_plane(1.0, |x, _| x)?,
            grey_plane(1.0, |_, y| y)?.blend(mode(word)?),
        ];
        let picture = scene(256, 256, layers)?.render(256)?;
        for (index, got) in crate::common::pixels(&picture).enumerate() {
            let (backdrop, source) = (index % 256, index / 256);
            let wanted = if word == "luminosity" { source } else { backdrop };
            #[expect(clippy::cast_precision_loss, reason = "a level from 0 to 255")]
            let level = wanted as f32 / 255.0;
            assert!(
                close(got, [level, level, level, 1.0], 2e-6),
                "{word}: backdrop {backdrop}, source {source}: wanted {level}, got {got:?}"
            );
        }
    }
    Ok(())
}

/// A colour that crosses the places the arithmetic is thin: greys (channels equal, where `Sat` is 0 and a brightness is the
/// level), near-greys (channels an ulp apart), the corners of the cube, and any colour.
fn nasty() -> impl Strategy<Value = [f32; 3]> {
    let level = prop_oneof![
        Just(0.0_f32),
        Just(1.0_f32),
        (0_u8..=255).prop_map(|byte| f32::from(byte) / 255.0),
        0.0_f32..=1.0
    ];
    let near = |level: f32, step: i8| match step {
        1 => level.next_up().min(1.0),
        -1 => level.next_down().max(0.0),
        _ => level,
    };
    prop_oneof![
        level.clone().prop_map(|level| [level; 3]),
        (level.clone(), [-1_i8..=1, -1_i8..=1, -1_i8..=1])
            .prop_map(move |(level, steps)| steps.map(|step| near(level, step))),
        [any::<bool>(), any::<bool>(), any::<bool>()].prop_map(|corner| corner.map(|on| if on { 1.0 } else { 0.0 })),
        [level.clone(), level.clone(), level],
    ]
}

fn alpha() -> impl Strategy<Value = f32> {
    prop_oneof![Just(0.0_f32), Just(1.0_f32), Just(0.5_f32), 0.0_f32..=1.0]
}

proptest! {
    /// Greys, near-greys, corners and any colour, at any alphas, in every mode: numbers from 0 to 1, no colour without alpha.
    #[test]
    fn every_mode_gives_numbers_from_0_to_1_on_greys_near_greys_and_corners(
        cb in nasty(), ab in alpha(), cs in nasty(), a_s in alpha(),
    ) {
        for word in ALL {
            let got = one([cell([cb[0], cb[1], cb[2], ab])?, cell([cs[0], cs[1], cs[2], a_s])?.blend(mode(word)?)])?;
            prop_assert!(got.iter().all(|value| (0.0..=1.0).contains(value)), "{word}: {got:?}");
            prop_assert!(got[3] > 0.0 || got[..3] == [0.0; 3], "{word}: colour with no alpha {got:?}");
        }
    }
}

// --- the value ---

#[test]
fn a_blend_is_a_value_and_a_render_is_deterministic() -> Outcome {
    let plain = || cell([0.25, 0.5, 0.75, 1.0]);
    let blended = plain()?.blend(Blend::Screen);
    let below = || cell([0.5, 0.25, 0.5, 1.0]);
    let before = one([below()?, plain()?])?;
    let first = one([below()?, blended.clone()])?;
    // the layer the call started from is unchanged, and the blended one draws the same twice
    assert!(exactly(one([below()?, plain()?])?, before));
    assert!(exactly(one([below()?, blended])?, first));
    assert!(!exactly(first, before), "screen is not normal");
    Ok(())
}
