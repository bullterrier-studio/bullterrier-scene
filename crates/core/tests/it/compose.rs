//! How layers join: normal blend, opacity, order, and the straight alpha that comes out. Every expected value is worked
//! out by hand or from the closed form of "over", never from the code's own formula. Fills stand for the layers, so a
//! colour is known exactly.

use bullterrier_scene::{Error, Layer};
use proptest::prelude::*;

use crate::common::{Outcome, all_exactly, close, pixels, scene};

fn fill(color: &str) -> Result<Layer, Error> {
    Layer::fill(color)
}

/// `#rrggbbaa` as the straight RGBA a fill of it is: each byte over 255.
fn straight(bytes: [u8; 4]) -> [f32; 4] {
    bytes.map(|byte| f32::from(byte) / 255.0)
}

#[test]
fn an_empty_scene_renders_transparent_at_the_size_asked() -> Outcome {
    let picture = scene(8, 6, [])?.render(16)?;
    assert_eq!((picture.width(), picture.height()), (16, 12));
    assert!(all_exactly(&picture, [0.0; 4]));
    Ok(())
}

/// The output of `ceil(side * size / long side)`, each worked out by hand: the long side is `size` and the other keeps
/// the canvas's aspect, rounded up.
#[test]
fn the_output_keeps_the_aspect_and_its_long_side_is_the_size() -> Outcome {
    let rows = [
        // canvas, size, (width, height)
        ((1280, 720), 640, (640, 360)),
        ((100, 100), 10, (10, 10)),
        ((720, 1280), 1280, (720, 1280)),
        ((200, 100), 100, (100, 50)),
        ((1000, 333), 500, (500, 167)), // 166.5 rounds up
        ((3, 2), 7, (7, 5)),            // 4.67 rounds up
        ((1, 1), 1, (1, 1)),
        ((9, 16), 1, (1, 1)),
        ((100, 3), 7, (7, 1)), // a thin canvas: 0.21 rounds up to 1 row, which stretches 3 design pixels onto it
    ];
    for ((width, height), size, expected) in rows {
        let picture = scene(width, height, [])?.render(size)?;
        assert_eq!(
            (picture.width(), picture.height()),
            expected,
            "{width}x{height} at {size}"
        );
    }
    Ok(())
}

proptest! {
    /// The long side is the size; each side is the smallest whole number of output pixels that holds its design length
    /// (`out * long >= side * size`, and one pixel less would not).
    #[test]
    fn the_long_side_of_the_output_is_the_size_and_each_side_is_the_least_that_holds_its_length(
        width in 1_u32..2000, height in 1_u32..2000, size in 1_u32..200,
    ) {
        let picture = scene(width, height, [])?.render(size)?;
        prop_assert_eq!(picture.width().max(picture.height()), usize::try_from(size)?);
        let long = u64::from(width.max(height));
        for (design, output) in [(width, picture.width()), (height, picture.height())] {
            let wanted = u64::from(design) * u64::from(size);
            let output = u64::try_from(output)?;
            prop_assert!(output * long >= wanted, "{output} is too short for {design}");
            prop_assert!((output - 1) * long < wanted, "{output} is more than {design} needs");
        }
    }
}

#[test]
fn an_opaque_layer_over_a_base_is_the_layer() -> Outcome {
    let picture = scene(5, 4, [fill("#102030")?, fill("#c0a080")?])?.render(10)?;
    assert!(all_exactly(&picture, straight([0xc0, 0xa0, 0x80, 0xff])));
    Ok(())
}

#[test]
fn a_fully_transparent_layer_changes_nothing() -> Outcome {
    let below = scene(5, 4, [fill("#102030")?])?.render(10)?;
    let above = scene(5, 4, [fill("#102030")?, fill("#ffffff00")?])?.render(10)?;
    assert_eq!(below.pixels(), above.pixels());
    Ok(())
}

#[test]
fn a_layer_with_opacity_zero_changes_nothing() -> Outcome {
    let below = scene(5, 4, [fill("#102030")?])?.render(10)?;
    let above = scene(5, 4, [fill("#102030")?, fill("#ffffff")?.opacity(0.0)?])?.render(10)?;
    assert_eq!(below.pixels(), above.pixels());
    Ok(())
}

/// White at half opacity over black is a half grey: 1 * 0.5 + 0 * 0.5.
#[test]
fn white_at_half_opacity_over_black_is_half_grey_exactly() -> Outcome {
    let picture = scene(3, 3, [fill("#000000")?, fill("#ffffff")?.opacity(0.5)?])?.render(3)?;
    assert!(all_exactly(&picture, [0.5, 0.5, 0.5, 1.0]));
    Ok(())
}

/// Red at half opacity over blue: (1, 0, 0) * 0.5 + (0, 0, 1) * 0.5.
#[test]
fn red_at_half_opacity_over_blue_is_the_even_mix_exactly() -> Outcome {
    let picture = scene(3, 3, [fill("#0000ff")?, fill("#ff0000")?.opacity(0.5)?])?.render(3)?;
    assert!(all_exactly(&picture, [0.5, 0.0, 0.5, 1.0]));
    Ok(())
}

#[test]
fn the_later_layer_wins_so_the_order_matters() -> Outcome {
    let red_then_blue = scene(2, 2, [fill("#ff0000")?, fill("#0000ff")?])?.render(2)?;
    let blue_then_red = scene(2, 2, [fill("#0000ff")?, fill("#ff0000")?])?.render(2)?;
    assert!(all_exactly(&red_then_blue, [0.0, 0.0, 1.0, 1.0]));
    assert!(all_exactly(&blue_then_red, [1.0, 0.0, 0.0, 1.0]));
    Ok(())
}

/// Porter and Duff's "over" for two layers of alpha a and b on nothing: the alpha is 1 - (1 - a)(1 - b), the first
/// layer's colour is kept by (1 - b), and the straight colour divides by that alpha.
#[test]
fn two_half_transparent_layers_stack_by_the_closed_form_of_over() -> Outcome {
    let (a, b) = (128.0_f32 / 255.0, 64.0_f32 / 255.0);
    let picture = scene(2, 2, [fill("#ff000080")?, fill("#0000ff40")?])?.render(2)?;
    let alpha = 1.0 - (1.0 - a) * (1.0 - b);
    let expected = [a * (1.0 - b) / alpha, 0.0, b / alpha, alpha];
    for pixel in pixels(&picture) {
        assert!(close(pixel, expected, 1e-6), "{pixel:?} is not {expected:?}");
    }
    Ok(())
}

/// A single layer comes out as it went in: straight alpha at both boundaries, whatever the alpha.
#[test]
fn a_single_layer_comes_out_as_straight_as_it_went_in() -> Outcome {
    for bytes in [
        [0x10, 0x80, 0xf0, 0x01],
        [0xff, 0xff, 0xff, 0x80],
        [0x01, 0x02, 0x03, 0xfe],
    ] {
        let [r, g, b, a] = bytes;
        let picture = scene(2, 2, [fill(&format!("#{r:02x}{g:02x}{b:02x}{a:02x}"))?])?.render(2)?;
        for pixel in pixels(&picture) {
            assert!(close(pixel, straight(bytes), 1e-6), "{pixel:?} for {bytes:?}");
        }
    }
    Ok(())
}

proptest! {
    /// An opaque colour on its own is exactly its bytes over 255: nothing is lost to the premultiplied inside.
    #[test]
    fn an_opaque_fill_renders_to_exactly_its_colour(red: u8, green: u8, blue: u8) {
        let layer = Layer::fill(&format!("#{red:02x}{green:02x}{blue:02x}"))?;
        let picture = scene(2, 2, [layer])?.render(2)?;
        prop_assert!(all_exactly(&picture, straight([red, green, blue, 255])));
    }

    /// Whatever the layers, every channel that comes out is in 0..=1, and where alpha is 0 so is the colour.
    #[test]
    fn every_value_that_comes_out_is_within_range_and_transparent_means_no_colour(
        layers in proptest::collection::vec((any::<[u8; 4]>(), 0.0_f64..=1.0), 1..6),
    ) {
        let layers = layers
            .into_iter()
            .map(|([r, g, b, a], opacity)| Layer::fill(&format!("#{r:02x}{g:02x}{b:02x}{a:02x}"))?.opacity(opacity))
            .collect::<Result<Vec<_>, _>>()?;
        let picture = scene(2, 2, layers)?.render(2)?;
        for [red, green, blue, alpha] in pixels(&picture) {
            prop_assert!([red, green, blue, alpha].iter().all(|value| (0.0..=1.0).contains(value)));
            prop_assert!(alpha > 0.0 || (red, green, blue) == (0.0, 0.0, 0.0));
        }
    }

    /// Stacking opacity: a fill at opacity `p` over nothing has alpha `p` times its own, whatever `p`.
    #[test]
    fn opacity_multiplies_alpha(opacity in 0.0_f64..=1.0, alpha: u8) {
        let layer = Layer::fill(&format!("#336699{alpha:02x}"))?.opacity(opacity)?;
        let picture = scene(1, 1, [layer])?.render(1)?;
        let expected = f64::from(alpha) / 255.0 * opacity;
        let got = f64::from(picture.pixels()[3]);
        prop_assert!((got - expected).abs() <= 1e-6, "{got} is not {expected}");
    }
}
