//! Light layers (stage 7): the layer's colour over black is its colour, and its alpha is the largest of red, green and blue
//! of that colour. Every expected value is worked out by hand.

use bullterrier_scene::{Blend, Error, Layer, Place, Raster, Text};
use proptest::prelude::*;

use crate::common::{Outcome, close, exactly, pixel, pixels, rectangle, scene};

const PLUS_ONE_STOP: &str =
    r#"{"steps": [{"effect": "grade", "params": {"exposure": 1.0}, "opacity": 1.0, "blend_mode": "normal"}]}"#;

fn cell(rgba: [f32; 4]) -> Result<Layer, Error> {
    Ok(Layer::picture(Raster::rgba(1, 1, rgba.to_vec())?))
}

fn one(layers: impl IntoIterator<Item = Layer>) -> Result<[f32; 4], Error> {
    Ok(pixel(&scene(1, 1, layers)?.render(1)?, 0, 0))
}

/// Straight colour (0.5, 0.5, 0.5) at alpha 0.5 is the colour 0.25 over black, so the light is 0.25 at alpha 0.25, which is
/// straight (1, 1, 1): the brightest colour that gives that much light. Black gives no alpha at all, and white is opaque white.
#[test]
fn a_light_layer_is_its_colour_over_black_with_the_brightest_channel_for_alpha() -> Outcome {
    let rows = [
        ([0.5, 0.5, 0.5, 0.5], [1.0, 1.0, 1.0, 0.25]),
        ([0.25, 0.5, 0.75, 1.0], [0.25 / 0.75, 0.5 / 0.75, 1.0, 0.75]),
        ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]),
        ([0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ([1.0, 0.5, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]),
    ];
    for (source, expected) in rows {
        let got = one([cell(source)?.as_light(true)])?;
        assert!(
            close(got, expected, 1e-6),
            "{source:?}: wanted {expected:?}, got {got:?}"
        );
    }
    Ok(())
}

#[test]
fn a_light_layer_screened_over_black_gives_its_colour_and_black_adds_nothing() -> Outcome {
    let black = || cell([0.0, 0.0, 0.0, 1.0]);
    for colour in [
        [0.25, 0.5, 0.75],
        [1.0, 0.0, 0.0],
        [0.5, 0.5, 0.5],
        [0.125, 0.0, 0.0625],
    ] {
        let layer = cell([colour[0], colour[1], colour[2], 1.0])?
            .as_light(true)
            .blend(Blend::Screen);
        let got = one([black()?, layer])?;
        assert!(
            close(got, [colour[0], colour[1], colour[2], 1.0], 1e-6),
            "{colour:?}: {got:?}"
        );
    }
    // black over any colour with screen, and with the normal blend, leaves the colour
    let backdrop = [0.2, 0.4, 0.6, 1.0];
    for blend in [Blend::Screen, Blend::Normal] {
        let dark = cell([0.0, 0.0, 0.0, 1.0])?.as_light(true).blend(blend);
        assert!(close(one([cell(backdrop)?, dark])?, backdrop, 1e-6), "{blend:?}");
    }
    Ok(())
}

#[test]
fn light_is_made_after_a_look_and_a_fill_is_a_light_too() -> Outcome {
    // +1 stop doubles (0.25, 0.4, 0.2) to (0.5, 0.8, 0.4): the alpha is 0.8 and the straight colour that over 0.8 is it
    let picture = Layer::picture(Raster::rgba(1, 1, vec![0.25, 0.4, 0.2, 1.0])?)
        .look(PLUS_ONE_STOP)?
        .as_light(true);
    assert!(close(one([picture])?, [0.5 / 0.8, 1.0, 0.4 / 0.8, 0.8], 1e-5));
    // #804020 is (128, 64, 32) of 255: the alpha is 128/255
    let fill = Layer::fill("#804020")?.as_light(true);
    assert!(close(one([fill])?, [1.0, 0.5, 0.25, 128.0 / 255.0], 1e-6));
    Ok(())
}

#[test]
fn light_off_is_no_call() -> Outcome {
    let below = cell([0.5, 0.25, 0.5, 1.0])?;
    let source = || cell([0.25, 0.5, 0.75, 0.5]);
    let plain = one([below.clone(), source()?])?;
    assert!(exactly(one([below.clone(), source()?.as_light(false)])?, plain));
    assert!(exactly(one([below, source()?.as_light(true).as_light(false)])?, plain));
    Ok(())
}

/// Stages 6 to 9 by hand. The source (0.5, 0.25, 0) opaque, masked by half: premultiplied (0.25, 0.125, 0), so the light has
/// alpha 0.25 and straight colour (1, 0.5, 0). Opacity 0.5: alpha 0.125. Screen over opaque black: B = Cs, so the result is
/// 0.125 (1, 0.5, 0) = (0.125, 0.0625, 0).
#[test]
fn a_mask_scales_the_light_then_opacity_then_the_blend() -> Outcome {
    let half = Layer::fill("#ffffff")?.opacity(0.5)?;
    let source = cell([0.5, 0.25, 0.0, 1.0])?.mask(half)?.as_light(true);
    let got = one([
        cell([0.0, 0.0, 0.0, 1.0])?,
        source.clone().opacity(0.5)?.blend(Blend::Screen),
    ])?;
    assert!(close(got, [0.125, 0.0625, 0.0, 1.0], 1e-6), "{got:?}");
    // the light alone shows the mask's share: alpha 0.25
    assert!(close(one([source])?, [1.0, 0.5, 0.0, 0.25], 1e-6));
    Ok(())
}

/// A layer drawn alone, plain and then made light: where the plain pixel is straight colour `C` at alpha `a`, the light one is
/// the same colour over black, `C a`, at alpha the largest of its channels, with straight colour `C a` over that. A source
/// that is no picture (a drawing, a text) is a light by the same rule, edges of partial alpha included.
fn is_the_light_of_its_plain_self(layer: &Layer, (width, height): (u32, u32)) -> Outcome {
    let plain = scene(width, height, [layer.clone()])?.render(width.max(height))?;
    let light = scene(width, height, [layer.clone().as_light(true)])?.render(width.max(height))?;
    let (mut partial, mut changed) = (0, 0);
    for (plain, light) in pixels(&plain).zip(pixels(&light)) {
        let over_black = [plain[0] * plain[3], plain[1] * plain[3], plain[2] * plain[3]];
        let brightest = over_black[0].max(over_black[1]).max(over_black[2]);
        let expected = if brightest > 0.0 {
            [
                over_black[0] / brightest,
                over_black[1] / brightest,
                over_black[2] / brightest,
                brightest,
            ]
        } else {
            [0.0; 4]
        };
        assert!(
            close(light, expected, 2e-6),
            "plain {plain:?}: wanted {expected:?}, got {light:?}"
        );
        partial += usize::from(plain[3] > 0.05 && plain[3] < 0.95);
        changed += usize::from((light[3] - plain[3]).abs() > 0.01);
    }
    assert!(
        partial > 4,
        "the layer has an edge of partial alpha to check ({partial} pixels)"
    );
    assert!(changed > 4, "light changes the alpha of the layer ({changed} pixels)");
    Ok(())
}

#[test]
fn a_drawing_is_a_light_by_the_same_rule_as_a_picture() -> Outcome {
    // a rectangle whose edges fall between pixels, in a colour whose channels differ
    let drawing = Layer::drawing(&rectangle((24, 24), (3.4, 4.6), (19.3, 17.7), "#d04020"))?;
    is_the_light_of_its_plain_self(&drawing, (24, 24))
}

#[test]
fn a_text_is_a_light_by_the_same_rule_as_a_picture() -> Outcome {
    let text = Text::new("bricolage_grotesque").color("#40a0e0").option("size", 40.0);
    let layer = Layer::text("Hi", &text)?.place(Place::new().dx(6.0).dy(40.0))?;
    is_the_light_of_its_plain_self(&layer, (80, 56))
}

proptest! {
    /// Alone, a light layer's premultiplied colour is the colour of its source over black, and its alpha is the largest of it.
    #[test]
    fn a_light_layer_keeps_the_colour_over_black_and_takes_the_brightest_channel_as_alpha(
        colour in [0.0_f32..=1.0, 0.0_f32..=1.0, 0.0_f32..=1.0], alpha in 0.0_f32..=1.0,
    ) {
        let got = one([cell([colour[0], colour[1], colour[2], alpha])?.as_light(true)])?;
        let over_black = colour.map(|channel| channel * alpha);
        let brightest = over_black[0].max(over_black[1]).max(over_black[2]);
        prop_assert!((got[3] - brightest).abs() < 1e-6, "{got:?}");
        for channel in 0..3 {
            prop_assert!((got[channel] * got[3] - over_black[channel]).abs() < 1e-6, "{got:?}");
        }
    }

    /// Whatever the mode and the backdrop, a light layer gives numbers from 0 to 1.
    #[test]
    fn a_light_layer_in_any_mode_gives_numbers_from_0_to_1(
        cb in [0.0_f32..=1.0, 0.0_f32..=1.0, 0.0_f32..=1.0], ab in 0.0_f32..=1.0,
        cs in [0.0_f32..=1.0, 0.0_f32..=1.0, 0.0_f32..=1.0], a_s in 0.0_f32..=1.0,
    ) {
        for word in ["normal", "screen", "multiply", "color-burn", "soft-light", "luminosity", "hue"] {
            let blend: Blend = word.parse()?;
            let got = one([cell([cb[0], cb[1], cb[2], ab])?, cell([cs[0], cs[1], cs[2], a_s])?.as_light(true).blend(blend)])?;
            prop_assert!(got.iter().all(|value| (0.0..=1.0).contains(value)), "{word}: {got:?}");
        }
    }
}
