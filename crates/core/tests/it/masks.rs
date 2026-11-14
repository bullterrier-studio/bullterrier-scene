//! Masks: what multiplies a layer's alpha, by a layer drawn on the canvas (stage 6) or by one of the layer's own maps
//! (stage 4), soft or hard, inverted or not. Every expected value is worked out by hand or from a closed form (a mask by
//! an opaque white fill changes nothing; a ramp multiplies; the weights of luminance; the Gaussian's distribution function
//! for a blurred edge), never from the code's own arithmetic.

use bullterrier_scene::{Blend, Deg, Error, Layer, MAX_DEPTH, Map, Mask, Place, Raster, Use};
use proptest::prelude::*;

use crate::common::{Outcome, pixels, rectangle, scene, solid};

const WHITE: &str = "#ffffff";

/// The alpha of every pixel of `layer` alone on a canvas of `width` by `height` design pixels at `size`, row after row.
fn alphas(layer: Layer, (width, height): (u32, u32), size: u32) -> Result<Vec<f32>, Error> {
    Ok(pixels(&scene(width, height, [layer])?.render(size)?)
        .map(|pixel| pixel[3])
        .collect())
}

/// A white opaque fill masked by `by`: its alpha is the mask's value.
fn masked_white(by: impl Into<Mask>, canvas: (u32, u32), size: u32) -> Result<Vec<f32>, Error> {
    alphas(Layer::fill(WHITE)?.mask(by)?, canvas, size)
}

/// A mask layer that is a 1-pixel picture of `rgba`, stretched over the canvas by `fit`.
fn flat_mask(rgba: [f32; 4], canvas: (u32, u32)) -> Result<Layer, Error> {
    Layer::picture(solid(1, 1, rgba)?).fit(
        [0.0, 0.0, f64::from(canvas.0), f64::from(canvas.1)],
        Fit::Stretch,
        [0.5, 0.5],
    )
}

use bullterrier_scene::Fit;

#[test]
fn a_mask_by_an_opaque_white_fill_changes_nothing_and_by_a_transparent_one_hides_everything() -> Outcome {
    let layer = || Layer::picture(crate::common::varied(6, 4).unwrap_or_else(|error| unreachable!("{error}")));
    let plain = scene(6, 4, [layer()])?.render(12)?;
    let kept = scene(6, 4, [layer().mask(Layer::fill(WHITE)?)?])?.render(12)?;
    assert_eq!(plain.pixels(), kept.pixels());
    let hidden = scene(6, 4, [layer().mask(Layer::fill("#ffffff00")?)?])?.render(12)?;
    assert!(pixels(&hidden).all(|pixel| pixel[3] == 0.0));
    Ok(())
}

/// A mask by a black fill read as luminance is 0 (a black picture hides), read as alpha it is 1 (it is drawn).
#[test]
fn black_hides_by_luminance_and_keeps_by_alpha() -> Outcome {
    let black = || Layer::fill("#000000");
    let by_luminance = masked_white(Mask::new(black()?).using(Use::Luminance), (4, 4), 4)?;
    let by_alpha = masked_white(black()?, (4, 4), 4)?;
    assert!(by_luminance.iter().all(|alpha| alpha.to_bits() == 0));
    assert!(by_alpha.iter().all(|alpha| alpha.to_bits() == 1.0_f32.to_bits()));
    Ok(())
}

/// Luminance weighs red 0.2126, green 0.7152, blue 0.0722 of the sRGB values and multiplies by the alpha.
#[test]
fn luminance_is_the_weighted_sum_of_the_colour_times_the_alpha() -> Outcome {
    let rows: [(&str, [f32; 4], f32); 7] = [
        ("red", [1.0, 0.0, 0.0, 1.0], 0.2126),
        ("green", [0.0, 1.0, 0.0, 1.0], 0.7152),
        ("blue", [0.0, 0.0, 1.0, 1.0], 0.0722),
        ("white", [1.0, 1.0, 1.0, 1.0], 1.0),
        ("a grey", [0.5, 0.5, 0.5, 1.0], 0.5),
        (
            "a mix",
            [0.2, 0.4, 0.8, 1.0],
            0.2126 * 0.2 + 0.7152 * 0.4 + 0.0722 * 0.8,
        ),
        ("a half-transparent green", [0.0, 1.0, 0.0, 0.5], 0.7152 * 0.5),
    ];
    for (what, rgba, expected) in rows {
        let value = masked_white(Mask::new(flat_mask(rgba, (2, 2))?).using(Use::Luminance), (2, 2), 2)?;
        assert!(
            value.iter().all(|alpha| (alpha - expected).abs() <= 1e-6),
            "{what}: {value:?} is not {expected}"
        );
    }
    // white is exactly 1, so a white mask changes nothing, bit for bit
    let white = masked_white(Mask::new(flat_mask([1.0; 4], (2, 2))?).using(Use::Luminance), (2, 2), 2)?;
    assert!(white.iter().all(|alpha| alpha.to_bits() == 1.0_f32.to_bits()));
    Ok(())
}

/// A mask by a picture whose alpha rises 0, 1/7 .. 1 along a row, at one output pixel to the design pixel: the layer's
/// alpha is the ramp, exactly.
#[test]
fn a_mask_by_an_alpha_ramp_multiplies_the_alpha_by_the_ramp() -> Outcome {
    let ramp: Vec<f32> = (0..8_u8).flat_map(|x| [0.3, 0.6, 0.9, f32::from(x) / 7.0]).collect();
    let by = Layer::picture(Raster::rgba(8, 1, ramp)?);
    let got = masked_white(by, (8, 1), 8)?;
    for (x, alpha) in got.iter().enumerate() {
        let expected = f32::from(u8::try_from(x)?) / 7.0;
        assert!(
            (alpha - expected).abs() <= 1e-6,
            "column {x}: {alpha} is not {expected}"
        );
    }
    Ok(())
}

proptest! {
    /// A mask and its inverse add up to 1 everywhere, whatever the mask is.
    #[test]
    fn a_mask_and_its_inverse_are_complements(levels in proptest::collection::vec(0.0_f32..=1.0, 12)) {
        let picture = |levels: &[f32]| -> Result<Layer, Error> {
            let values = levels.iter().flat_map(|&level| [1.0, 0.5, 0.0, level]).collect();
            Ok(Layer::picture(Raster::rgba(4, 3, values)?))
        };
        let plain = masked_white(picture(&levels)?, (4, 3), 4)?;
        let inverse = masked_white(Mask::new(picture(&levels)?).invert(), (4, 3), 4)?;
        for (a, b) in plain.iter().zip(&inverse) {
            prop_assert!((a + b - 1.0).abs() <= 1e-6, "{a} + {b} is not 1");
        }
    }

    /// Two masks multiply, in either order: the alpha is the layer's times each.
    #[test]
    fn two_masks_multiply_whatever_their_order(first in 0.0_f32..=1.0, second in 0.0_f32..=1.0, alpha in 0.0_f32..=1.0) {
        let a = flat_mask([1.0, 1.0, 1.0, first], (2, 2))?;
        let b = flat_mask([1.0, 1.0, 1.0, second], (2, 2))?;
        let base = || Layer::picture(solid(2, 2, [0.2, 0.4, 0.6, alpha]).unwrap_or_else(|error| unreachable!("{error}")));
        let ab = alphas(base().mask(a.clone())?.mask(b.clone())?, (2, 2), 2)?;
        let ba = alphas(base().mask(b)?.mask(a)?, (2, 2), 2)?;
        let expected = alpha * first * second;
        for (x, y) in ab.iter().zip(&ba) {
            prop_assert!((x - expected).abs() <= 1e-6 && (y - expected).abs() <= 1e-6, "{x} {y} is not {expected}");
        }
    }
}

/// A mask layer's opacity multiplies the mask; its own masks are part of its alpha.
#[test]
fn a_mask_layers_opacity_and_own_masks_are_part_of_the_mask() -> Outcome {
    let half = masked_white(Layer::fill(WHITE)?.opacity(0.5)?, (2, 2), 2)?;
    assert!(half.iter().all(|alpha| alpha.to_bits() == 0.5_f32.to_bits()));
    // a fill masked by the left half of the canvas is that half
    let left = || Layer::drawing(&rectangle((8, 4), (0.0, 0.0), (4.0, 4.0), WHITE));
    let nested = masked_white(Layer::fill(WHITE)?.mask(left()?)?, (8, 4), 8)?;
    let direct = masked_white(left()?, (8, 4), 8)?;
    assert_eq!(nested, direct);
    assert!(nested.iter().any(|&alpha| alpha > 0.5) && nested.iter().any(|&alpha| alpha < 0.5));
    Ok(())
}

/// A rectangle drawing covers exactly the pixels worked out from its coordinates, at each whole size, and a mask layer's
/// own `place` moves the cut by exactly what it says.
#[test]
fn a_mask_by_a_rectangle_covers_the_hand_counted_pixels_and_moves_with_its_own_place() -> Outcome {
    let rect = || Layer::drawing(&rectangle((8, 4), (2.0, 1.0), (6.0, 3.0), WHITE));
    for scale in [1_usize, 2, 4] {
        let size = u32::try_from(8 * scale)?;
        let got = masked_white(rect()?, (8, 4), size)?;
        for (index, alpha) in got.iter().enumerate() {
            let (x, y) = (index % (8 * scale), index / (8 * scale));
            let inside = (2 * scale..6 * scale).contains(&x) && (scale..3 * scale).contains(&y);
            assert!(
                (alpha - f32::from(u8::from(inside))).abs() == 0.0,
                "({x}, {y}) at {scale}x: {alpha}"
            );
        }
    }
    let moved = masked_white(rect()?.place(Place::new().dx(1.0))?, (8, 4), 8)?;
    for (index, alpha) in moved.iter().enumerate() {
        let (x, y) = (index % 8, index / 8);
        let inside = (3..7).contains(&x) && (1..3).contains(&y);
        assert!(
            (alpha - f32::from(u8::from(inside))).abs() == 0.0,
            "({x}, {y}) moved: {alpha}"
        );
    }
    Ok(())
}

/// The mask layer's looks apply to its colour, which luminance reads: a grey of 0.25 given +1 stop is 0.5.
#[test]
fn a_mask_layers_look_changes_the_luminance_it_is_read_at() -> Outcome {
    let grey =
        Layer::picture(solid(1, 1, [0.25, 0.25, 0.25, 1.0])?).fit([0.0, 0.0, 2.0, 2.0], Fit::Stretch, [0.5, 0.5])?;
    let stop = r#"{"steps": [{"effect": "grade", "params": {"exposure": 1.0}}]}"#;
    let plain = masked_white(Mask::new(grey.clone()).using(Use::Luminance), (2, 2), 2)?;
    let lit = masked_white(Mask::new(grey.look(stop)?).using(Use::Luminance), (2, 2), 2)?;
    assert!(plain.iter().all(|alpha| (alpha - 0.25).abs() <= 1e-6));
    assert!(lit.iter().all(|alpha| (alpha - 0.5).abs() <= 1e-6));
    Ok(())
}

/// Masks work on every kind of layer: a picture, a drawing and a fill each lose what the half-plane mask hides.
#[test]
fn every_kind_of_layer_takes_a_mask() -> Outcome {
    let left = || Layer::drawing(&rectangle((4, 2), (0.0, 0.0), (2.0, 2.0), WHITE));
    let layers = [
        ("a picture", Layer::picture(solid(4, 2, [0.1, 0.2, 0.3, 1.0])?)),
        (
            "a drawing",
            Layer::drawing(&rectangle((4, 2), (0.0, 0.0), (4.0, 2.0), "#336699"))?,
        ),
        ("a fill", Layer::fill("#336699")?),
    ];
    for (what, layer) in layers {
        let got = alphas(layer.mask(left()?)?, (4, 2), 4)?;
        let expected = [1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0];
        assert_eq!(got, expected, "{what}");
    }
    Ok(())
}

/// The mask stays on the canvas when the layer moves: a layer placed by `dx` is cut where the canvas's mask is.
#[test]
fn a_mask_is_in_canvas_space_whatever_the_layer_does() -> Outcome {
    let left = Layer::drawing(&rectangle((8, 2), (0.0, 0.0), (4.0, 2.0), WHITE))?;
    let band = Layer::fill(WHITE)?.fit([0.0, 0.0, 6.0, 2.0], Fit::Stretch, [0.5, 0.5])?;
    let moved = band.place(Place::new().dx(2.0))?.mask(left)?;
    // the band covers columns 2..8 and the mask columns 0..4: the layer shows in columns 2..4
    let got = alphas(moved, (8, 2), 8)?;
    for (index, alpha) in got.iter().enumerate() {
        let x = index % 8;
        assert_eq!(*alpha, f32::from(u8::from((2..4).contains(&x))), "column {x}");
    }
    Ok(())
}

// --- feather ------------------------------------------------------------------------------------------------------

/// The error function (Abramowitz and Stegun 7.1.26, absolute error below 1.5e-7).
fn erf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let poly =
        ((((1.061_405_429 * t - 1.453_152_027) * t + 1.421_413_741) * t - 0.284_496_736) * t + 0.254_829_592) * t;
    (1.0 - poly * (-x * x).exp()).copysign(x)
}

/// The Gaussian distribution function: the share of a Gaussian of standard deviation `sigma` below `x`.
fn gauss(x: f64, sigma: f64) -> f64 {
    f64::midpoint(1.0, erf(x / (sigma * std::f64::consts::SQRT_2)))
}

/// What three passes of a box are from the Gaussian's distribution function, at most: 0.0099 (`docs/design-decisions.md`,
/// "The feather"), and a little for the rounding of 32-bit floats and of `erf`.
const WITHIN: f64 = 0.0099 + 1e-5;

/// What the Gaussian puts past the top and bottom edges of a tall shape for a row `distance` design pixels from each (the
/// hand oracles of the half planes below blur along one axis; the shapes are made tall enough that the other is a tail this
/// small), the two edges together.
fn tails(distance: f64, sigma: f64) -> f64 {
    2.0 * gauss(-distance, sigma)
}

/// A tall rectangle from the canvas's left edge to column 100 of a canvas 200 wide, so far above and below the four rows that
/// the canvas shows that its top and bottom edges do not touch them.
fn half_plane() -> Result<Layer, Error> {
    Layer::drawing(&rectangle((200, 200), (0.0, 0.0), (100.0, 200.0), WHITE))?.place(Place::new().dy(-100.0))
}

/// A half plane blurred by sigma equals the Gaussian's distribution function sampled at the pixel centres: the value at pixel
/// `i` is the share of a Gaussian centred on it that falls in the shape, which runs from the canvas's left edge to column 100.
#[test]
fn a_feathered_edge_is_the_gaussians_distribution_function_at_the_pixel_centres() -> Outcome {
    for sigma in [1.5_f64, 1.75, 2.0, 3.5, 10.0, 30.0] {
        let got = masked_white(Mask::new(half_plane()?).feather(sigma), (200, 4), 200)?;
        for x in 0..200_u16 {
            let centre = f64::from(x) + 0.5;
            // the shape's left edge is the canvas's: nothing is drawn beyond it, so it is blurred as well
            let expected = gauss(100.0 - centre, sigma) - gauss(-centre, sigma);
            let value = f64::from(got[usize::from(x)]);
            assert!(
                (value - expected).abs() <= WITHIN + tails(96.0, sigma),
                "sigma {sigma}, column {x}: {value} is not {expected}"
            );
        }
    }
    Ok(())
}

/// The claim of 0.0099 is from a sigma of 1.5 output pixels up (dense scan from 1.5 to 41: at most 0.00985): below it the kernel is
/// a few pixels wide and the sampled Gaussian is a coarser thing, off by up to 0.012 at sigma 1, 0.049 at 0.5 and 0.013 at 0.2.
/// Pinned so that the floor in `api.md` is the true one.
#[test]
fn the_kernel_is_off_the_gaussian_by_more_than_the_claim_below_a_sigma_of_1_5_pixels() -> Outcome {
    let worst = |sigma: f64| -> Result<f64, Box<dyn std::error::Error>> {
        let got = masked_white(Mask::new(half_plane()?).feather(sigma), (200, 4), 200)?;
        Ok((90..110_u16)
            .map(|x| {
                let centre = f64::from(x) + 0.5;
                (f64::from(got[usize::from(x)]) - (gauss(100.0 - centre, sigma) - gauss(-centre, sigma))).abs()
            })
            .fold(0.0, f64::max))
    };
    assert!(worst(1.5)? <= WITHIN, "{}", worst(1.5)?);
    let coarse = worst(0.5)?;
    assert!(coarse > 0.04 && coarse < 0.06, "{coarse}");
    Ok(())
}

/// The blur is symmetric about an edge: the values k pixels before it and k pixels after add up to 1.
#[test]
fn a_feathered_edge_is_symmetric_about_the_edge() -> Outcome {
    let got = masked_white(Mask::new(half_plane()?).feather(10.0), (200, 4), 200)?;
    for k in 0..40 {
        let (before, after) = (f64::from(got[99 - k]), f64::from(got[100 + k]));
        assert!((before + after - 1.0).abs() <= 2e-6, "{k}: {before} + {after}");
    }
    Ok(())
}

/// A feather of 0 is no feather: the same pixels bit for bit.
#[test]
fn a_feather_of_zero_is_the_mask_with_no_feather() -> Outcome {
    let plain = masked_white(half_plane()?, (200, 4), 200)?;
    let zero = masked_white(Mask::new(half_plane()?).feather(0.0), (200, 4), 200)?;
    assert_eq!(
        plain.iter().map(|alpha| alpha.to_bits()).collect::<Vec<_>>(),
        zero.iter().map(|alpha| alpha.to_bits()).collect::<Vec<_>>()
    );
    Ok(())
}

/// The blur conserves what it blurs: the mass of a shape well inside the canvas is the same before and after (the sum of the
/// values, in 64 bits), to the rounding of 32-bit floats.
#[test]
fn a_feather_conserves_the_mass_of_a_shape_away_from_the_edges() -> Outcome {
    let shape = || Layer::drawing(&rectangle((120, 80), (50.0, 30.0), (70.0, 50.0), WHITE));
    let mass = |values: &[f32]| values.iter().map(|&value| f64::from(value)).sum::<f64>();
    let hard = mass(&masked_white(shape()?, (120, 80), 120)?);
    assert!((hard - 400.0).abs() <= 1e-9, "{hard}");
    for sigma in [2.0, 3.0, 5.0] {
        let soft = mass(&masked_white(Mask::new(shape()?).feather(sigma), (120, 80), 120)?);
        assert!(
            (soft - hard).abs() <= 1e-4 * hard,
            "sigma {sigma}: {soft} is not {hard}"
        );
    }
    Ok(())
}

/// A feather of a mask that fills the canvas leaves it constant, edge included, bit for bit: a fill reaches into the margin
/// the blur needs, so nothing falls off at the canvas's edge. A fill is drawn to the edge of what is drawn.
#[test]
fn a_feathered_mask_that_fills_the_canvas_stays_one_to_the_edge() -> Outcome {
    for sigma in [0.5, 2.0, 7.5, 40.0] {
        let got = masked_white(Mask::new(Layer::fill(WHITE)?).feather(sigma), (30, 20), 30)?;
        assert!(
            got.iter().all(|alpha| alpha.to_bits() == 1.0_f32.to_bits()),
            "sigma {sigma}"
        );
        let none = masked_white(Mask::new(Layer::fill("#ffffff00")?).feather(sigma), (30, 20), 30)?;
        assert!(none.iter().all(|alpha| alpha.to_bits() == 0));
    }
    Ok(())
}

/// A shape that ends at the canvas's edge feathers there as on a larger canvas: half of it is off the edge, so the value at
/// the edge is the Gaussian's share about the pixel's centre.
#[test]
fn a_shape_that_ends_at_the_canvas_edge_is_soft_there() -> Outcome {
    let sigma = 6.0;
    let got = masked_white(Mask::new(half_plane()?).feather(sigma), (200, 4), 200)?;
    let edge = f64::from(got[0]);
    assert!(
        (edge - gauss(0.5, sigma)).abs() <= WITHIN + tails(96.0, sigma) && edge < 0.6,
        "{edge}"
    );
    Ok(())
}

/// A shape that crosses the canvas's edge is blurred as it would on a larger canvas: a rectangle of a frame wider than the
/// canvas, from column 150 to its end at 300, blurred on a canvas 200 wide, is nearly 1 at the canvas's right edge. Zero
/// beyond the canvas, or its edge repeated, would give 0.5 and 1.
#[test]
fn a_shape_that_crosses_the_canvas_edge_is_blurred_as_on_a_larger_canvas() -> Outcome {
    let sigma = 12.0;
    let wide =
        Layer::drawing(&rectangle((300, 200), (150.0, 0.0), (300.0, 200.0), WHITE))?.place(Place::new().dy(-100.0))?;
    let got = masked_white(Mask::new(wide).feather(sigma), (200, 4), 200)?;
    for x in 130..200_u16 {
        let centre = f64::from(x) + 0.5;
        let expected = gauss(300.0 - centre, sigma) - gauss(150.0 - centre, sigma);
        let value = f64::from(got[usize::from(x)]);
        assert!(
            (value - expected).abs() <= WITHIN + tails(96.0, sigma),
            "column {x}: {value} is not {expected}"
        );
    }
    assert!(got[199] > 0.99, "{}", got[199]);
    Ok(())
}

/// What the feather is measured in: design pixels, so the same blur at twice the size is twice as many output pixels, the
/// same picture in more detail.
#[test]
fn the_feather_is_in_design_pixels_so_the_same_at_every_size() -> Outcome {
    let sigma = 8.0;
    let half =
        || Layer::drawing(&rectangle((100, 100), (0.0, 0.0), (50.0, 100.0), WHITE))?.place(Place::new().dy(-50.0));
    for size in [100_u16, 200] {
        let got = masked_white(Mask::new(half()?).feather(sigma), (100, 4), u32::from(size))?;
        let across = f64::from(size) / 100.0;
        for x in 0..size {
            let centre = (f64::from(x) + 0.5) / across;
            let expected = gauss(50.0 - centre, sigma) - gauss(-centre, sigma);
            let value = f64::from(got[usize::from(x)]);
            assert!(
                (value - expected).abs() <= WITHIN + tails(46.0, sigma),
                "size {size}, column {x}: {value} is not {expected}"
            );
        }
    }
    Ok(())
}

// --- what is refused -----------------------------------------------------------------------------------------------

/// A `mask` call is refused, naming the field, for a feather outside its range (and not a number), a mask layer that has a
/// name, a map that is no map, a map read as luminance, a name given to a layer with no picture. The known-good rows are taken.
#[test]
fn what_a_mask_call_cannot_take_is_refused_naming_the_field() -> Outcome {
    let picture = || Layer::picture(solid(2, 2, [0.5, 0.5, 0.5, 1.0]).unwrap_or_else(|error| unreachable!("{error}")));
    let with_map = || {
        picture().map(
            "matte",
            Map::new(2, 2, vec![1.0; 4]).unwrap_or_else(|error| unreachable!("{error}")),
        )
    };
    let fill = |color: &str| Layer::fill(color);
    let rows: [(&str, Result<Layer, Error>, Option<&str>); 17] = [
        ("a layer", fill(WHITE)?.mask(fill(WHITE)?), None),
        (
            "a feather of 5",
            fill(WHITE)?.mask(Mask::new(fill(WHITE)?).feather(5.0)),
            None,
        ),
        (
            "the largest feather",
            fill(WHITE)?.mask(Mask::new(fill(WHITE)?).feather(1e6)),
            None,
        ),
        (
            "a feather past it",
            fill(WHITE)?.mask(Mask::new(fill(WHITE)?).feather(1e9)),
            Some("feather"),
        ),
        (
            "a negative feather",
            fill(WHITE)?.mask(Mask::new(fill(WHITE)?).feather(-1.0)),
            Some("feather"),
        ),
        (
            "a feather that is no number",
            fill(WHITE)?.mask(Mask::new(fill(WHITE)?).feather(f64::NAN)),
            Some("feather"),
        ),
        (
            "a feather of infinity",
            fill(WHITE)?.mask(Mask::new(fill(WHITE)?).feather(f64::INFINITY)),
            Some("feather"),
        ),
        (
            "a named mask layer",
            fill(WHITE)?.mask(fill(WHITE)?.named("shape")?),
            Some("mask"),
        ),
        (
            "a mask layer with a blend",
            fill(WHITE)?.mask(fill(WHITE)?.blend(Blend::Multiply)),
            Some("mask"),
        ),
        (
            "a mask layer with the normal blend",
            fill(WHITE)?.mask(fill(WHITE)?.blend(Blend::Normal)),
            None,
        ),
        (
            "a mask layer that is light",
            fill(WHITE)?.mask(fill(WHITE)?.as_light(true)),
            Some("mask"),
        ),
        (
            "a mask layer that is not light",
            fill(WHITE)?.mask(fill(WHITE)?.as_light(false)),
            None,
        ),
        ("a map of the layer", with_map()?.mask("matte"), None),
        ("a map it has not", with_map()?.mask("subject"), Some("mask")),
        (
            "a map as luminance",
            with_map()?.mask(Mask::new("matte").using(Use::Luminance)),
            Some("use"),
        ),
        ("a name on a fill", fill(WHITE)?.mask("matte"), Some("mask")),
        ("a name given before the map", picture().mask("matte"), Some("mask")),
    ];
    for (what, made, refused) in rows {
        match (made, refused) {
            (Ok(_), None) => {}
            (Err(Error::Invalid { address, .. }), Some(field)) => assert_eq!(address, field, "{what}"),
            (other, _) => return Err(format!("{what}: {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn a_mask_layer_with_a_blend_or_a_light_is_refused_saying_which() -> Outcome {
    for (made, word) in [
        (Layer::fill(WHITE)?.blend(Blend::Screen), "blend"),
        (Layer::fill(WHITE)?.as_light(true), "light"),
    ] {
        let Err(Error::Invalid { address, message }) = Layer::fill(WHITE)?.mask(made) else {
            return Err(format!("a mask layer with a {word} was accepted").into());
        };
        assert_eq!(address, "mask");
        assert!(message.contains(word), "{message}");
    }
    Ok(())
}

#[test]
fn a_mask_word_that_is_not_a_use_is_refused_naming_use() {
    for (word, good) in [
        ("alpha", true),
        ("luminance", true),
        ("Alpha", false),
        ("light", false),
        ("", false),
    ] {
        let parsed = word.parse::<Use>();
        assert_eq!(parsed.is_ok(), good, "{word:?}");
        if let Err(Error::Invalid { address, .. }) = parsed {
            assert_eq!(address, "use");
        }
    }
}

/// A feather whose blur could not be drawn is refused when the scene renders, before anything of its size is allocated,
/// naming the mask and the pixel limit: a feather of 100,000 design pixels on a canvas of 100.
#[test]
fn a_feather_too_wide_to_draw_is_refused_at_render_naming_the_mask_and_the_limit() -> Outcome {
    let layer = Layer::fill(WHITE)?
        .mask(Mask::new(Layer::fill(WHITE)?).feather(1e5))?
        .named("photo")?;
    let refused = scene(100, 100, [layer])?.render(100);
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message })
            if address == "photo.mask.feather" && message.contains("134217728")),
        "{refused:?}"
    );
    // the second mask is addressed as the second
    let two = Layer::fill(WHITE)?
        .mask(Layer::fill(WHITE)?)?
        .mask(Mask::new(Layer::fill(WHITE)?).feather(1e5))?
        .named("photo")?;
    let refused = scene(100, 100, [two])?.render(100);
    assert!(
        matches!(&refused, Err(Error::Invalid { address, .. }) if address == "photo.mask#2.feather"),
        "{refused:?}"
    );
    Ok(())
}

/// A mask layer is drawn like any layer, so what it cannot draw is refused naming it, under the mask.
#[test]
fn a_mask_layer_that_cannot_be_drawn_is_refused_at_render_under_the_mask() -> Outcome {
    let by = Layer::picture(solid(2, 2, [0.5; 4])?).map_hole("subject")?;
    let refused = scene(2, 2, [Layer::fill(WHITE)?.mask(by)?.named("photo")?])?.render(2);
    assert!(
        matches!(&refused, Err(Error::Invalid { address, .. }) if address == "photo.mask.map.subject"),
        "{refused:?}"
    );
    Ok(())
}

// --- depth ---------------------------------------------------------------------------------------------------------

/// A nest of masks `depth` deep: each layer is masked by the one below it, down to a fill.
fn nest(depth: usize) -> Result<Layer, Error> {
    let mut layer = Layer::fill(WHITE)?;
    for _ in 0..depth {
        layer = Layer::fill(WHITE)?.mask(layer)?;
    }
    Ok(layer)
}

#[test]
fn masks_nest_to_the_limit_and_one_deeper_is_refused_at_the_call() -> Outcome {
    assert_eq!(MAX_DEPTH, 16);
    let deepest = nest(MAX_DEPTH)?;
    let refused = Layer::fill(WHITE)?.mask(deepest);
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message }) if address == "mask" && message.contains("16")),
        "{refused:?}"
    );
    assert!(Layer::fill(WHITE)?.mask(nest(MAX_DEPTH - 1)?).is_ok());
    Ok(())
}

/// The most mask layers a layer holds is a thousand, a shared one counted each time: a layer that masks by two copies of the
/// layer below it would draw it twice at every level.
#[test]
fn the_mask_layers_of_a_layer_are_counted_each_time_and_bounded() -> Outcome {
    let mut layer = Layer::fill(WHITE)?;
    let mut refused = None;
    for level in 0..16 {
        match Layer::fill(WHITE)?
            .mask(layer.clone())
            .and_then(|next| next.mask(layer.clone()))
        {
            Ok(next) => layer = next,
            Err(error) => {
                refused = Some((level, error));
                break;
            }
        }
    }
    // each level doubles the mask layers (2^k - 1 + ...): the ninth level passes a thousand
    assert!(
        matches!(&refused, Some((level, Error::Invalid { address, message })) if *level >= 8 && address == "mask" && message.contains("1000")),
        "{refused:?}"
    );
    Ok(())
}

/// A mask nest at the depth limit is built, drawn, printed, cloned and dropped on a thread of 256 KB, the smallest stack the
/// library supports: nothing here recurses deeper than the nest, and a mask's `Debug` does not print the layers it nests.
#[test]
fn a_nest_at_the_depth_limit_is_drawn_printed_and_dropped_on_a_256_kb_thread() -> Outcome {
    let worked = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| -> Result<(usize, usize), String> {
            let layer = nest(MAX_DEPTH).map_err(|error| error.to_string())?;
            let shown = format!("{layer:?}");
            let copy = layer.clone();
            let drawn = scene(4, 4, [copy])
                .and_then(|scene| scene.render(4))
                .map_err(|error| error.to_string())?;
            drop(layer);
            Ok((shown.len(), drawn.pixels().len()))
        })?;
    let (shown, drawn) = worked.join().map_err(|_| "the thread panicked")??;
    assert!(shown < 2_000, "the nest prints {shown} bytes");
    assert_eq!(drawn, 64);
    Ok(())
}

// --- masks by a map of the layer's own picture (stage 4) -----------------------------------------------------------

/// A picture of `width` by `height` opaque pixels, mid grey, with a map `m` whose value at column `x`, row `y` is
/// `value(x, y)`.
fn with_map(width: usize, height: usize, value: impl Fn(usize, usize) -> f32) -> Result<Layer, Error> {
    let values = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| value(x, y))
        .collect();
    Layer::picture(solid(width, height, [0.4, 0.5, 0.6, 1.0])?).map("m", Map::new(width, height, values)?)
}

/// The map is cut and fitted with the picture: a picture 8 wide cut to 4 by `cover` shows the map's columns the picture's
/// columns are, 2 to 5 for the middle, whatever the map holds there.
#[test]
fn a_map_is_cut_by_cover_with_its_picture() -> Outcome {
    let hand = |x: usize, y: usize| f32::from(u8::try_from(x + 8 * y + 1).unwrap_or(0)) / 40.0;
    let nothing = r#"{"steps": [{"effect": "grade"}]}"#;
    // with no look the map is multiplied into the picture and the warp cuts both; with a look the map is cut to the window
    // the picture is drawn at: the same cut
    for looked in [false, true] {
        for (align, first) in [(0.0, 0), (0.5, 2), (1.0, 4)] {
            let mut layer = with_map(8, 4, hand)?
                .fit([0.0, 0.0, 4.0, 4.0], Fit::Cover, [align, 0.5])?
                .mask("m")?;
            if looked {
                layer = layer.look(nothing)?;
            }
            let got = alphas(layer, (4, 4), 4)?;
            for (index, alpha) in got.iter().enumerate() {
                let (x, y) = (index % 4, index / 4);
                assert_eq!(
                    alpha.to_bits(),
                    hand(first + x, y).to_bits(),
                    "align {align}, column {x}, row {y}, looked {looked}"
                );
            }
        }
    }
    Ok(())
}

/// The map goes where the picture goes: stretched, flipped and moved with it.
#[test]
fn a_map_follows_its_picture_through_a_stretch_a_flip_and_a_move() -> Outcome {
    let ramp = |x: usize, _: usize| f32::from(u8::try_from(x).unwrap_or(0)) / 4.0;
    // 4 columns of the map 0, .25, .5, .75 stretched to 8 columns: bilinear between pixel centres
    let stretched = alphas(
        with_map(4, 1, ramp)?
            .fit([0.0, 0.0, 8.0, 1.0], Fit::Stretch, [0.5, 0.5])?
            .mask("m")?,
        (8, 1),
        8,
    )?;
    for (x, alpha) in stretched.iter().enumerate() {
        let at = (f32::midpoint(f32::from(u8::try_from(x)?), 0.5) - 0.5).clamp(0.0, 3.0);
        assert!(
            (alpha - at / 4.0).abs() <= 1e-6,
            "column {x}: {alpha} is not {}",
            at / 4.0
        );
    }
    let flipped = alphas(
        with_map(4, 1, ramp)?.mask("m")?.place(Place::new().flip_x(true))?,
        (4, 1),
        4,
    )?;
    assert_eq!(flipped, [0.75, 0.5, 0.25, 0.0]);
    let moved = alphas(with_map(4, 1, ramp)?.mask("m")?.place(Place::new().dx(1.0))?, (4, 1), 4)?;
    assert_eq!(moved, [0.0, 0.0, 0.25, 0.5]);
    Ok(())
}

/// A mask by a map is the map's value, inverted is its complement, two of them multiply.
#[test]
fn a_map_mask_inverts_and_multiplies_like_any_mask() -> Outcome {
    let left = |x: usize, _: usize| f32::from(u8::from(x < 2)) * 0.75;
    let top = |_: usize, y: usize| if y == 0 { 0.5 } else { 1.0 };
    let plain = alphas(with_map(4, 2, left)?.mask("m")?, (4, 2), 4)?;
    let inverse = alphas(with_map(4, 2, left)?.mask(Mask::new("m").invert())?, (4, 2), 4)?;
    for (a, b) in plain.iter().zip(&inverse) {
        assert!((a + b - 1.0).abs() <= 1e-7, "{a} + {b}");
    }
    let values: Vec<f32> = (0..2).flat_map(|y| (0..4).map(move |x| top(x, y))).collect();
    let both = with_map(4, 2, left)?
        .map("top", Map::new(4, 2, values)?)?
        .mask("m")?
        .mask("top")?;
    let got = alphas(both, (4, 2), 4)?;
    assert_eq!(got, [0.375, 0.375, 0.0, 0.0, 0.75, 0.75, 0.0, 0.0]);
    Ok(())
}

/// With no look a map is multiplied into the picture's own pixels (and blurred there); with a look the picture is resampled to
/// the size the look runs at and the map with it, blurred there and multiplied after the look. At one output pixel to the
/// design pixel the two agree to the bit in alpha, with a feather and without. Where the picture is reduced by two they
/// agree to 1e-6 without a feather (the area average of a map is the same number either way). With a feather they are two
/// kernels (3 box passes of sigma 2 before the reduction, of sigma 1 after it), each within 0.0099 of the Gaussian's
/// distribution function, so across an edge they differ by about two of those and the reduction's rounding: measured 0.0218,
/// and the tolerance is 0.025.
#[test]
fn a_look_that_changes_nothing_gives_the_alpha_a_layer_has_with_no_look() -> Outcome {
    let nothing = r#"{"steps": [{"effect": "grade"}]}"#;
    let wavy = |x: usize, y: usize| f32::from(u8::try_from((x * 7 + y * 3) % 10).unwrap_or(0)) / 10.0;
    let edge = |x: usize, _: usize| f32::from(u8::from(x < 8));
    let rows: [(&str, f32, u32, bool, f32); 4] = [
        ("one to one, hard", 0.0, 16, false, 0.0),
        ("one to one, soft", 3.0, 16, false, 0.0),
        ("reduced by two, hard", 0.0, 8, false, 1e-6),
        ("reduced by two, soft, across an edge", 2.0, 8, true, 0.025),
    ];
    for (what, feather, size, across_an_edge, tolerance) in rows {
        let map: &dyn Fn(usize, usize) -> f32 = if across_an_edge { &edge } else { &wavy };
        let layer =
            || -> Result<Layer, Error> { with_map(16, 16, map)?.mask(Mask::new("m").feather(f64::from(feather))) };
        let plain = alphas(layer()?, (16, 16), size)?;
        let looked = alphas(layer()?.look(nothing)?, (16, 16), size)?;
        let worst = plain
            .iter()
            .zip(&looked)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(worst <= tolerance + 1e-6, "{what}: the alphas differ by {worst}");
    }
    Ok(())
}

/// A map held constant stays so under any feather, to the bit, at the picture's edge too: the edge is repeated.
#[test]
fn a_feathered_constant_map_stays_constant_to_the_edge_of_the_picture() -> Outcome {
    for (level, sigma) in [(1.0_f32, 5.0), (0.0, 5.0), (0.25, 3.0), (1.0, 0.7)] {
        let got = alphas(
            with_map(9, 7, |_, _| level)?.mask(Mask::new("m").feather(sigma))?,
            (9, 7),
            9,
        )?;
        assert!(
            got.iter().all(|alpha| alpha.to_bits() == level.to_bits()),
            "level {level}, sigma {sigma}"
        );
    }
    Ok(())
}

/// A matte that reaches the picture's edge is blurred as if it went on: the left half of a picture, from its left edge to
/// column 100 of 200, has the Gaussian's share past the edge at 100 and none of the fall-off a zero beyond the edge would give
/// at 0.
#[test]
fn a_map_blurs_with_its_edge_repeated_not_as_a_shape_that_ends_there() -> Outcome {
    let sigma = 10.0;
    let got = alphas(
        with_map(200, 2, |x, _| f32::from(u8::from(x < 100)))?.mask(Mask::new("m").feather(sigma))?,
        (200, 2),
        200,
    )?;
    for x in 0..200_u16 {
        let expected = gauss(100.0 - (f64::from(x) + 0.5), sigma);
        let value = f64::from(got[usize::from(x)]);
        assert!(
            (value - expected).abs() <= WITHIN,
            "column {x}: {value} is not {expected}"
        );
    }
    assert!(got[0] > 0.999, "{}", got[0]);
    Ok(())
}

/// A map's feather is in design pixels as the layer is fitted: a picture 100 wide stretched to 200 design pixels is
/// enlarged twice, and a feather of 8 design pixels is 4 pixels of the picture, 8 of the output.
#[test]
fn a_map_feather_is_in_design_pixels_of_the_fitted_layer_not_in_picture_pixels() -> Outcome {
    let sigma = 8.0;
    let layer = with_map(100, 2, |x, _| f32::from(u8::from(x < 50)))?
        .fit([0.0, 0.0, 200.0, 2.0], Fit::Stretch, [0.5, 0.5])?
        .mask(Mask::new("m").feather(sigma))?;
    let got = alphas(layer, (200, 2), 200)?;
    for x in 0..200_u16 {
        let expected = gauss(100.0 - (f64::from(x) + 0.5), sigma);
        let value = f64::from(got[usize::from(x)]);
        // the blur is in picture pixels (sigma 4) and the picture is then enlarged by bilinear interpolation
        assert!(
            (value - expected).abs() <= WITHIN + 0.004,
            "column {x}: {value} is not {expected}"
        );
    }
    Ok(())
}

/// A feathered mask and its inverse add up to 1 everywhere, the canvas's edge included: the blur is of the mask, then the
/// inverse is 1 minus it (an inverse taken before the blur would turn the margin's nothing into something).
#[test]
fn a_feathered_mask_and_its_inverse_are_complements_to_the_edge() -> Outcome {
    for sigma in [2.0, 7.0] {
        let plain = masked_white(Mask::new(half_plane()?).feather(sigma), (200, 4), 200)?;
        let inverse = masked_white(Mask::new(half_plane()?).feather(sigma).invert(), (200, 4), 200)?;
        for (x, (a, b)) in plain.iter().zip(&inverse).enumerate() {
            assert!((a + b - 1.0).abs() <= 1e-6, "sigma {sigma}, pixel {x}: {a} + {b}");
        }
    }
    Ok(())
}

/// A canvas whose design pixel is not square (100 by 4 at size 10 is 10 by 1 pixels: 0.1 across, 0.25 down) has a margin of
/// its own along each axis: a shape that crosses the left edge and is far taller than the canvas is blurred by sigma along x
/// as the Gaussian says, whatever the blur along y reaches.
#[test]
fn a_feather_on_a_canvas_with_two_scales_has_a_margin_along_each_axis() -> Outcome {
    let sigma = 20.0;
    // a fill (one pixel, whatever the scale) fitted to the box from x = -50 to 50 and y = -75 to 75
    let shape = Layer::fill(WHITE)?.fit([-50.0, -75.0, 100.0, 150.0], Fit::Stretch, [0.5, 0.5])?;
    let got = masked_white(Mask::new(shape).feather(sigma), (100, 4), 10)?;
    assert_eq!(got.len(), 10);
    for (x, value) in got.iter().enumerate() {
        let centre = (f64::from(u8::try_from(x)?) + 0.5) / 0.1;
        let expected = gauss(50.0 - centre, sigma) - gauss(-50.0 - centre, sigma);
        assert!(
            (f64::from(*value) - expected).abs() <= WITHIN + tails(71.0, sigma),
            "column {x}: {value} is not {expected}"
        );
    }
    Ok(())
}

#[test]
fn a_mask_call_leaves_the_layer_it_was_made_on_as_it_was_and_a_render_is_repeatable() -> Outcome {
    let base = Layer::fill("#336699")?;
    let masked = base.clone().mask(Mask::new(half_plane()?).feather(5.0))?;
    let before = scene(200, 4, [base.clone()])?.render(200)?;
    assert!(crate::common::all_exactly(&before, [0.2, 0.4, 0.6, 1.0]));
    let first = scene(200, 4, [masked.clone()])?.render(200)?;
    assert_eq!(first.pixels(), scene(200, 4, [masked])?.render(200)?.pixels());
    assert!(
        first
            .pixels()
            .iter()
            .skip(3)
            .step_by(4)
            .any(|&alpha| alpha > 0.0 && alpha < 1.0)
    );
    assert_eq!(scene(200, 4, [base])?.render(200)?.pixels(), before.pixels());
    Ok(())
}

/// A fill that is placed or fitted is the shape `place` and `fit` make of the canvas box, whatever its feather: a blur adds
/// margin to the plane the mask is drawn on, and the margin must not change the box that `scale`, `aspect`, `rotate` and
/// `contain` act on. Each row's feathered mass equals its hard mass (the shape is well inside the canvas).
#[test]
fn a_feathered_fill_that_is_placed_or_fitted_has_the_shape_it_has_unfeathered() -> Outcome {
    let rows: [(&str, Layer); 4] = [
        ("scaled by a half", Layer::fill(WHITE)?.place(Place::new().scale(0.5))?),
        (
            "squashed",
            Layer::fill(WHITE)?.place(Place::new().aspect(0.5).scale(0.5))?,
        ),
        (
            "contained in a box of another shape",
            Layer::fill(WHITE)?.fit([20.0, 10.0, 80.0, 60.0], Fit::Contain, [0.5, 0.5])?,
        ),
        (
            "turned a quarter and halved",
            Layer::fill(WHITE)?.place(Place::new().rotate(bullterrier_scene::Deg(90.0)).scale(0.4))?,
        ),
    ];
    let mass = |values: &[f32]| values.iter().map(|&value| f64::from(value)).sum::<f64>();
    for (what, shape) in rows {
        let hard = mass(&masked_white(shape.clone(), (120, 80), 120)?);
        assert!(hard > 1_000.0, "{what}: the shape is {hard}");
        let soft = mass(&masked_white(Mask::new(shape).feather(3.0), (120, 80), 120)?);
        assert!((soft - hard).abs() <= 1e-4 * hard, "{what}: {soft} is not {hard}");
    }
    // and the scaled one is the half-size rectangle in the middle: 60 by 40 pixels, exactly
    let half = masked_white(Layer::fill(WHITE)?.place(Place::new().scale(0.5))?, (120, 80), 120)?;
    assert!((half.iter().map(|&value| f64::from(value)).sum::<f64>() - 2_400.0).abs() <= 1e-9);
    Ok(())
}

// --- what the review of C3 found alive -----------------------------------------------------------------------------

/// A layer's own opacity and its masks multiply: opacity 0.5 under a mask of 0.5 is 0.25, in either order of the calls.
#[test]
fn a_layers_opacity_multiplies_what_its_mask_leaves() -> Outcome {
    let half = || flat_mask([1.0, 1.0, 1.0, 0.5], (2, 2));
    let masked_then_faded = alphas(Layer::fill(WHITE)?.mask(half()?)?.opacity(0.5)?, (2, 2), 2)?;
    let faded_then_masked = alphas(Layer::fill(WHITE)?.opacity(0.5)?.mask(half()?)?, (2, 2), 2)?;
    assert!(
        masked_then_faded
            .iter()
            .chain(&faded_then_masked)
            .all(|alpha| alpha.to_bits() == 0.25_f32.to_bits())
    );
    Ok(())
}

/// The Gaussian blur of `values` by `sigma` (a sum of Gaussian weights out to 6 sigma, the edge repeated): an oracle that shares
/// nothing with the box passes.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "a test oracle over lines of a few dozen values"
)]
fn gaussian(values: &[f64], sigma: f64) -> Vec<f64> {
    let reach = (6.0 * sigma).ceil() as i64;
    let weights: Vec<f64> = (-reach..=reach)
        .map(|at| (-(at as f64).powi(2) / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    let last = values.len() as i64 - 1;
    (0..values.len() as i64)
        .map(|at| {
            weights
                .iter()
                .zip(-reach..=reach)
                .map(|(weight, step)| weight * values[(at + step).clamp(0, last) as usize])
                .sum::<f64>()
                / total
        })
        .collect()
}

/// `values` sampled at the centres of `count` pixels that stretch it: position `(i + 0.5) * length / count - 0.5`, linear
/// between neighbours, the edge repeated.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "a test oracle over lines of a few dozen values"
)]
fn stretched(values: &[f64], count: usize) -> Vec<f64> {
    let last = values.len() as f64 - 1.0;
    (0..count)
        .map(|i| {
            let at = ((i as f64 + 0.5) * values.len() as f64 / count as f64 - 0.5).clamp(0.0, last);
            let low = at.floor();
            let high = (low + 1.0).min(last);
            values[low as usize] * (1.0 - (at - low)) + values[high as usize] * (at - low)
        })
        .collect()
}

/// A map's feather is in design pixels as the layer is fitted, along each axis by that axis's own fit: an 8 by 8 map stretched to
/// 8 by 32 (1 along x, 4 along y) with a feather of 8 is a blur of 8 pixels of the map along x and 2 along y if the picture
/// is blurred before it is drawn (no look), and of 8 and 8 pixels of the drawn picture if a look makes it be drawn first. Each is
/// compared with the Gaussian worked out by hand, along each axis, within the kernel's 0.0099 for each axis.
#[test]
fn a_map_feather_under_stretch_follows_each_axis_on_both_paths() -> Outcome {
    let quadrant: Vec<f64> = (0..8).map(|at| f64::from(u8::from(at < 4))).collect();
    let map: Vec<f32> = (0..64)
        .map(|at| f32::from(u8::from(at % 8 < 4 && at / 8 < 4)))
        .collect();
    let nothing = r#"{"steps": [{"effect": "grade"}]}"#;
    let layer = || -> Result<Layer, Error> {
        Layer::picture(solid(8, 8, [0.4, 0.5, 0.6, 1.0])?)
            .map("m", Map::new(8, 8, map.clone())?)?
            .fit([0.0, 0.0, 8.0, 32.0], Fit::Stretch, [0.5, 0.5])?
            .mask(Mask::new("m").feather(8.0))
    };
    // no look: blurred at the picture's size (sigma 8 along x, 2 along y), then drawn 4 times as tall
    let across = gaussian(&quadrant, 8.0);
    let down = gaussian(&quadrant, 2.0);
    let rows = stretched(&down, 32);
    let plain = alphas(layer()?, (8, 32), 32)?;
    for (index, alpha) in plain.iter().enumerate() {
        let expected = across[index % 8] * rows[index / 8];
        assert!(
            (f64::from(*alpha) - expected).abs() <= 0.0099 * 2.0 + 1e-3,
            "no look, pixel {index}: {alpha} is not {expected}"
        );
    }
    // a look: the picture is drawn 8 by 32 first (the map stretched), then blurred by 8 along each axis
    let across_drawn = gaussian(&quadrant, 8.0);
    let tall = stretched(&quadrant, 32);
    let down_drawn = gaussian(&tall, 8.0);
    let looked = alphas(layer()?.look(nothing)?, (8, 32), 32)?;
    for (index, alpha) in looked.iter().enumerate() {
        let expected = across_drawn[index % 8] * down_drawn[index / 8];
        assert!(
            (f64::from(*alpha) - expected).abs() <= 0.0099 * 2.0 + 1e-3,
            "a look, pixel {index}: {alpha} is not {expected}"
        );
    }
    Ok(())
}

/// Thin canvases, whose design pixel is not square (100 by 4 at size 10 is 0.1 by 0.25, and 4 by 100 the other way), have a
/// margin of their own along each axis: a feathered fill that is not fitted or placed stays 1 to every edge.
#[test]
fn a_feathered_fill_stays_one_to_the_edge_on_a_thin_canvas_either_way_round() -> Outcome {
    for canvas in [(100, 4), (4, 100)] {
        let got = masked_white(Mask::new(Layer::fill(WHITE)?).feather(40.0), canvas, 10)?;
        assert!(got.len() >= 10);
        assert!(
            got.iter().all(|alpha| alpha.to_bits() == 1.0_f32.to_bits()),
            "{canvas:?}: {got:?}"
        );
    }
    Ok(())
}

/// The blur along y of a canvas whose y scale is not its x scale (4 by 100 at size 10: 0.25 across, 0.1 down) is by the y scale:
/// a fill that ends half way down, blurred by 30 design pixels (3 rows), is the Gaussian's share row by row (two edges inside
/// the reach, each within 0.0099).
#[test]
fn a_feather_on_a_tall_thin_canvas_blurs_by_the_y_scale() -> Outcome {
    let sigma = 30.0;
    let shape = Layer::fill(WHITE)?.fit([-100.0, 0.0, 300.0, 50.0], Fit::Stretch, [0.5, 0.5])?;
    let got = masked_white(Mask::new(shape).feather(sigma), (4, 100), 10)?;
    assert_eq!(got.len(), 10);
    for (row, value) in got.iter().enumerate() {
        let centre = (f64::from(u8::try_from(row)?) + 0.5) / 0.1;
        let expected = gauss(50.0 - centre, sigma) - gauss(-centre, sigma);
        assert!(
            (f64::from(*value) - expected).abs() <= 2.0 * WITHIN,
            "row {row}: {value} is not {expected}"
        );
    }
    Ok(())
}

/// How a layer of the stage 4 table is put: its fit, place or perspective.
type Build = fn(Layer) -> Result<Layer, Error>;

/// A row of the stage 4 table: what it is, the feather, the canvas, the size, how the layer is put, the bound.
type Case = (&'static str, f64, (u32, u32), u32, Build, f32);

/// How far the two paths of stage 4 are apart, case by case, in the largest difference of alpha: a picture 16 by 16 with a map of 0 and 1
/// that changes every few pixels, masked by it (feathered where the row says), with no look and with a look that changes nothing. Where the picture is
/// drawn as it is (nothing resampled before the one warp that cuts and fits it: a plain cover or contain at one output pixel to
/// the design pixel, a move, a skew) the two agree to the bit. Where the picture is resampled and then warped again by a turn, a
/// perspective or a reduction, the no-look path resamples picture and map once and the look path twice, and they differ by what
/// two resamples of a map of 0 and 1 that changes every few pixels differ by. A blur is made at the picture's size on one path
/// and at the drawn size on the other, so a feathered case differs by the two kernels as well. The bound of each row is its
/// measurement rounded up.
#[expect(clippy::too_many_lines, reason = "a table of cases, one row each")]
#[test]
fn the_two_paths_of_stage_4_agree_where_nothing_is_resampled_twice_and_by_these_amounts_elsewhere() -> Outcome {
    let nothing = r#"{"steps": [{"effect": "grade"}]}"#;
    let wavy = |x: usize, y: usize| f32::from(u8::from((x * 7 + y * 3) % 10 < 5));
    // (what, feather, canvas, size, how the layer is put, the bound)
    let cases: [Case; 11] = [
        ("plain, 1:1", 0.0, (16, 16), 16, Ok, 0.0),
        (
            "cover, 1:1",
            0.0,
            (16, 8),
            16,
            |l| l.fit([0.0, 0.0, 16.0, 8.0], Fit::Cover, [0.5, 0.5]),
            0.0,
        ),
        (
            "contain, 1:1",
            0.0,
            (16, 16),
            16,
            |l| l.fit([0.0, 0.0, 8.0, 16.0], Fit::Contain, [0.5, 0.5]),
            0.0,
        ),
        (
            "skew, 1:1",
            0.0,
            (16, 16),
            16,
            |l| l.place(Place::new().skew_x(Deg(20.0))),
            0.0,
        ),
        (
            "turned 17 degrees and scaled 0.9",
            0.0,
            (16, 16),
            16,
            |l| l.place(Place::new().rotate(Deg(17.0)).scale(0.9)),
            0.22,
        ),
        (
            "in perspective",
            0.0,
            (16, 16),
            16,
            |l| l.perspective([[2.0, 1.0], [14.0, 3.0], [15.0, 14.0], [1.0, 12.0]]),
            0.18,
        ),
        (
            "turned 30 degrees at half size",
            0.0,
            (16, 16),
            8,
            |l| l.place(Place::new().rotate(Deg(30.0))),
            0.23,
        ),
        (
            "turned 30 degrees at half size, feather 2",
            2.0,
            (16, 16),
            8,
            |l| l.place(Place::new().rotate(Deg(30.0))),
            0.014,
        ),
        (
            "turned 30 degrees at half size, feather 2, an edge",
            2.0,
            (16, 16),
            8,
            |l| l.place(Place::new().rotate(Deg(30.0))),
            0.019,
        ),
        (
            "cover crop, feather 2",
            2.0,
            (8, 4),
            8,
            |l| l.fit([0.0, 0.0, 8.0, 4.0], Fit::Cover, [0.5, 0.5]),
            0.12,
        ),
        (
            "cover crop, feather 6",
            6.0,
            (8, 4),
            8,
            |l| l.fit([0.0, 0.0, 8.0, 4.0], Fit::Cover, [0.5, 0.5]),
            0.10,
        ),
    ];
    for (what, feather, canvas, size, put, bound) in cases {
        let map: &dyn Fn(usize, usize) -> f32 = if what.ends_with("an edge") {
            &|x, _| f32::from(u8::from(x < 8))
        } else {
            &wavy
        };
        let layer = || -> Result<Layer, Error> { put(with_map(16, 16, map)?.mask(Mask::new("m").feather(feather))?) };
        let plain = alphas(layer()?, canvas, size)?;
        let looked = alphas(layer()?.look(nothing)?, canvas, size)?;
        let worst = plain
            .iter()
            .zip(&looked)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(worst <= bound, "{what}: the paths differ by {worst}");
    }
    Ok(())
}

/// The trap: a mask layer that is a picture the size of the canvas ends at the canvas edge, so feathered it is a shape that
/// ends there (0.5 at the edge), while a fill covers the margin and a map is repeated, and both stay 1. To keep a picture's
/// border use a fill, a map (`mask("name")`), or a picture drawn larger than the canvas.
#[test]
fn a_canvas_sized_opaque_picture_as_a_feathered_mask_darkens_the_border_and_a_fill_does_not() -> Outcome {
    let sigma = 6.0;
    let picture = Layer::picture(solid(40, 40, [1.0; 4])?);
    let dark = masked_white(Mask::new(picture).feather(sigma), (40, 40), 40)?;
    let edge = f64::from(dark[20 * 40]);
    assert!((edge - gauss(0.5, sigma)).abs() <= WITHIN && edge < 0.6, "{edge}");
    assert!(f64::from(dark[20 * 40 + 20]) > 0.99, "the middle is untouched");
    let kept = masked_white(Mask::new(Layer::fill(WHITE)?).feather(sigma), (40, 40), 40)?;
    assert!(kept.iter().all(|alpha| alpha.to_bits() == 1.0_f32.to_bits()));
    // a picture drawn larger than the canvas has the margin's worth of picture past the edge
    let larger = Layer::picture(solid(40, 40, [1.0; 4])?).fit([-20.0, -20.0, 80.0, 80.0], Fit::Stretch, [0.5, 0.5])?;
    let bigger = masked_white(Mask::new(larger).feather(sigma), (40, 40), 40)?;
    assert!(bigger.iter().all(|&alpha| alpha > 0.999));
    Ok(())
}

/// A mask layer's opacity multiplies the mask before `invert`: opacity 0.25 inverted is 0.75 where the layer is drawn and 1
/// where it is not.
#[test]
fn a_mask_layers_opacity_applies_before_invert() -> Outcome {
    let drawn = Layer::fill(WHITE)?.opacity(0.25)?;
    let got = masked_white(Mask::new(drawn).invert(), (2, 2), 2)?;
    assert!(got.iter().all(|alpha| alpha.to_bits() == 0.75_f32.to_bits()), "{got:?}");
    let nothing = Layer::drawing(&rectangle((2, 2), (0.0, 0.0), (1.0, 2.0), WHITE))?.opacity(0.25)?;
    let got = masked_white(Mask::new(nothing).invert(), (2, 2), 2)?;
    assert_eq!(got, [0.75, 1.0, 0.75, 1.0]);
    Ok(())
}

/// A map is blurred line by line and makes no plane, so a picture whose blur's reach would make a plane of more than 2^27 pixels
/// is blurred all the same (2,000 by 2,000 with a feather of 1,600 would, past a reach of 4,800 pixels): only a line too long to
/// hold (a feather of 1e9 picture pixels) is refused.
#[test]
fn a_map_feather_is_bounded_by_its_line_and_not_by_a_plane_it_does_not_make() -> Outcome {
    let size = 2_000;
    let layer = Layer::picture(solid(size, size, [0.4, 0.5, 0.6, 1.0])?)
        .map("m", Map::new(size, size, vec![1.0; size * size])?)?
        .mask(Mask::new("m").feather(1_600.0))?;
    let got = scene(2_000, 2_000, [layer])?.render(10)?;
    assert!(
        pixels(&got).all(|pixel| (pixel[3] - 1.0).abs() <= 1e-6),
        "a constant map stays 1 (to the area average of the reduction)"
    );
    // a tiny box makes a design pixel 1,000 picture pixels: the largest feather is 1e9 pixels of the picture
    let tiny = Layer::picture(solid(2, 2, [0.4, 0.5, 0.6, 1.0])?)
        .map("m", Map::new(2, 2, vec![1.0; 4])?)?
        .fit([0.0, 0.0, 0.002, 0.002], Fit::Stretch, [0.5, 0.5])?
        .mask(Mask::new("m").feather(1_000_000.0))?
        .named("photo")?;
    let refused = scene(1, 1, [tiny])?.render(1);
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message }) if address == "photo.mask.feather" && message.contains("line")),
        "{refused:?}"
    );
    Ok(())
}
