//! Room (C5c): a look that paints past its picture (a bloom) is run on the layer grown by the room image declares, and a light
//! layer shows it. The oracle is image alone: the content on a frame grown by hand by that room, run through image's own
//! render, which never grows a picture. Every recipe here is a bloom of image, whose spread is the number the pad is made from.

use bullterrier_image::{Presets, Recipe};
use bullterrier_scene::{Blend, Error, Fit, Layer, Map, Place, Raster};

use crate::common::{Outcome, made, pixel, pixels, rectangle, scene, solid};

/// The canvas, in design pixels, drawn one output pixel to the design pixel.
const CANVAS: u32 = 96;
/// The content of every layer: a square of this many pixels, put at (`AT`, `AT`).
const SIDE: usize = 20;
const AT: usize = 38;
/// `AT` as a length in design pixels.
const AT_PX: f64 = 38.0;
const BLOOM: &str = r#"{"steps": [{"effect": "bloom", "params": {"radius": 12.0, "threshold": 0.2, "amount": 1.0}}]}"#;

/// The room, in whole pixels, image declares for the recipe on this canvas (the same on all four sides for a bloom).
fn declared(recipe: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let presets = Presets::shipped()?;
    let spread = Recipe::from_json(recipe)?.checked(&presets)?.spread(&presets, CANVAS)?;
    let [top, right, bottom, left] = spread.pixels().ok_or("the recipe has no bounded spread")?;
    assert!(top == right && right == bottom && bottom == left, "{spread:?}");
    Ok(top)
}

fn render(layer: Layer) -> Result<Raster, Error> {
    scene(CANVAS, CANVAS, [layer])?.render(CANVAS)
}

/// The colour of the content: orange, as bytes so that a drawing and a picture hold the same numbers.
const ORANGE: [f32; 4] = [1.0, 128.0 / 255.0, 0.0, 1.0];

/// A square drawing of the content with a frame of `SIDE + 2 pad`, the square `pad` in from each side.
fn drawing(pad: usize) -> Result<Layer, Error> {
    let (frame, pad) = (
        u32::try_from(SIDE + 2 * pad).unwrap_or(0),
        f64::from(u32::try_from(pad).unwrap_or(0)),
    );
    let side = pad + 20.0;
    Layer::drawing(&rectangle((frame, frame), (pad, pad), (side, side), "#ff8000"))?
        .place(Place::new().dx(AT_PX - pad).dy(AT_PX - pad))
}

/// The content as a picture with `pad` transparent pixels round it.
fn picture(pad: usize) -> Result<Layer, Error> {
    let inside = |x: usize, y: usize| (pad..pad + SIDE).contains(&x) && (pad..pad + SIDE).contains(&y);
    let raster = made(SIDE + 2 * pad, SIDE + 2 * pad, |x, y| {
        if inside(x, y) { ORANGE } else { [0.0; 4] }
    })?;
    let pad = f64::from(u32::try_from(pad).unwrap_or(0));
    Layer::picture(raster).place(Place::new().dx(AT_PX - pad).dy(AT_PX - pad))
}

/// The content as a group: a scene of `SIDE + 2 pad` holding the square `pad` in from its sides.
fn group(pad: usize) -> Result<Layer, Error> {
    let frame = u32::try_from(SIDE + 2 * pad).unwrap_or(0);
    let pad = f64::from(u32::try_from(pad).unwrap_or(0));
    let inner = scene(
        frame,
        frame,
        [Layer::picture(solid(SIDE, SIDE, ORANGE)?).place(Place::new().dx(pad).dy(pad))?],
    )?;
    Layer::scene(inner).place(Place::new().dx(AT_PX - pad).dy(AT_PX - pad))
}

/// The content as a fill, fitted into its square (the hand-made twin is the picture of the same colour).
fn fill() -> Result<Layer, Error> {
    Layer::fill("#ff8000")?.fit([AT_PX, AT_PX, 20.0, 20.0], Fit::Stretch, [0.5, 0.5])
}

/// A look and a light, the layer a glow is: a bloom whose halo is let past the box.
fn glowing(layer: Result<Layer, Error>, recipe: &str, light: bool) -> Result<Layer, Error> {
    Ok(layer?.look(recipe)?.as_light(light).blend(Blend::Screen))
}

/// What image alone makes of the content on a frame grown by `pad` (transparent black), looked at as a light layer's look sees
/// it (the colour over black): `3` floats a pixel, the picture `SIDE + 2 pad` square. With `boxed`, image is told the frame
/// is a box with room (`Options::with_room`), so that a step that places itself by its picture is placed by the box.
fn by_image(recipe: &str, pad: usize, boxed: bool) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let side = SIDE + 2 * pad;
    let inside = |x: usize, y: usize| (pad..pad + SIDE).contains(&x) && (pad..pad + SIDE).contains(&y);
    let colour: Vec<f32> = (0..side * side)
        .flat_map(|at| {
            if inside(at % side, at / side) {
                [ORANGE[0], ORANGE[1], ORANGE[2]]
            } else {
                [0.0; 3]
            }
        })
        .collect();
    let presets = Presets::shipped()?;
    let checked = Recipe::from_json(recipe)?.checked(&presets)?;
    let options = bullterrier_image::Options::default().with_canvas(CANVAS, CANVAS);
    let pad_px = u32::try_from(pad)?;
    let options = if boxed {
        options.with_room(pad_px, pad_px, pad_px, pad_px)
    } else {
        options
    };
    let image = bullterrier_image::Photo::new(bullterrier_image::Picture::new(side, side, &colour)?)
        .render(&checked, &presets, options)?;
    Ok(image.into_values())
}

/// The largest difference between `shown` and `expected` (what `by_image` made on a frame grown by `pad`, its top left at the
/// canvas pixel (`AT` - `pad`)): the light of every canvas pixel for a light layer, nothing outside the frame; for a layer that
/// is not light the colour in the box, an alpha of 1 in the box and 0 outside it.
fn against_image(shown: &Raster, expected: &[f32], pad: usize, light: bool) -> f32 {
    let (side, origin) = (SIDE + 2 * pad, AT - pad);
    let mut worst = 0.0_f32;
    for y in 0..shown.height() {
        for x in 0..shown.width() {
            let [red, green, blue, alpha] = pixel(shown, x, y);
            let frame = (x >= origin && x < origin + side && y >= origin && y < origin + side).then(|| {
                let at = ((y - origin) * side + x - origin) * 3;
                [expected[at], expected[at + 1], expected[at + 2]]
            });
            let in_box = (pad..pad + SIDE).contains(&x.wrapping_sub(origin))
                && (pad..pad + SIDE).contains(&y.wrapping_sub(origin));
            let (got, wanted, alpha_wanted) = if light {
                (
                    [red * alpha, green * alpha, blue * alpha],
                    frame.unwrap_or([0.0; 3]),
                    None,
                )
            } else if in_box {
                ([red, green, blue], frame.unwrap_or([0.0; 3]), Some(1.0))
            } else {
                ([0.0; 3], [0.0; 3], Some(0.0))
            };
            let colour = (0..3).map(|c| (got[c] - wanted[c]).abs()).fold(0.0, f32::max);
            let own = alpha_wanted.map_or(0.0, |expected: f32| (alpha - expected).abs());
            worst = worst.max(colour).max(own);
        }
    }
    worst
}

/// A tight layer with room made by the look is image's own render of the content on a frame grown by the declared room: a
/// drawing, a picture, a group and a fill, light or not. The grown picture is the same one, so to a rounding of the light
/// (a division and a product).
#[test]
fn a_looked_layer_is_what_image_makes_of_the_layer_on_a_frame_grown_by_the_declared_room() -> Outcome {
    let room = declared(BLOOM)?;
    assert!(room > 4, "the room is {room}: too little to tell a halo from an edge");
    let expected = by_image(BLOOM, room, false)?;
    for (what, tight) in [
        ("drawing", drawing(0)?),
        ("picture", picture(0)?),
        ("group", group(0)?),
        ("fill", fill()?),
    ] {
        let shown = render(glowing(Ok(tight), BLOOM, true)?)?;
        let difference = against_image(&shown, &expected, room, true);
        assert!(difference < 1e-6, "{what}: differ by {difference}");
    }
    Ok(())
}

/// The declared room holds the whole halo: image's own render on a frame grown by more than the room has no light further than
/// the room from the content (to the rounding of a Fourier blur).
#[test]
fn the_declared_room_holds_the_whole_halo() -> Outcome {
    let room = declared(BLOOM)?;
    let wide = by_image(BLOOM, room + 8, false)?;
    let side = SIDE + 2 * (room + 8);
    let beyond = (0..side * side)
        .filter(|at| {
            let (x, y) = (at % side, at / side);
            let far = |c: usize| c + room < room + 8 || c >= room + 8 + SIDE + room;
            far(x) || far(y)
        })
        .flat_map(|at| &wide[at * 3..at * 3 + 3])
        .fold(0.0_f32, |worst, value| worst.max(value.abs()));
    assert!(beyond < 1e-5, "light {beyond} beyond the room {room}");
    Ok(())
}

/// The columns and rows (from 0) at which the picture has light (an alpha above a millionth), as the first and last of each.
fn lit_extent(raster: &Raster) -> Option<(usize, usize, usize, usize)> {
    let width = raster.width();
    pixels(raster)
        .enumerate()
        .filter(|(_, pixel)| pixel[3] > 1e-6)
        .map(|(at, _)| (at % width, at / width))
        .fold(None, |found, (x, y)| match found {
            None => Some((x, x, y, y)),
            Some((left, right, top, bottom)) => Some((left.min(x), right.max(x), top.min(y), bottom.max(y))),
        })
}

/// A light layer's halo lands past its box and goes no further than the room: the lit pixels are the box grown by the room at
/// most, and more than the box. A layer that is not light shows its box and nothing else, whatever the look made past it.
#[test]
fn a_light_layers_halo_shows_past_its_box_and_no_further_than_the_room() -> Outcome {
    let room = declared(BLOOM)?;
    let at = AT;
    let box_extent = (at, at + SIDE - 1, at, at + SIDE - 1);
    for (what, tight) in [
        ("drawing", drawing(0)?),
        ("picture", picture(0)?),
        ("group", group(0)?),
        ("fill", fill()?),
    ] {
        let light = lit_extent(&render(glowing(Ok(tight.clone()), BLOOM, true)?)?).ok_or("no light")?;
        assert!(
            light.0 < box_extent.0 && light.1 > box_extent.1 && light.2 < box_extent.2 && light.3 > box_extent.3,
            "{what}: {light:?}"
        );
        assert!(
            light.0 + room >= box_extent.0
                && light.1 <= box_extent.1 + room
                && light.2 + room >= box_extent.2
                && light.3 <= box_extent.3 + room,
            "{what}: the halo {light:?} passes the room {room} round {box_extent:?}"
        );
        let plain = lit_extent(&render(glowing(Ok(tight), BLOOM, false)?)?).ok_or("no pixels")?;
        assert_eq!(
            plain, box_extent,
            "{what}: a layer that is not light keeps its source's alpha"
        );
    }
    Ok(())
}

/// A layer that is not light is looked at its own size, as before the room: image's own render of its picture, bit for bit, even
/// at an edge a bloom or a blur would reach past (the look sees the picture's own edge, not black), and its alpha is its source's.
#[test]
fn a_layer_that_is_not_light_is_looked_at_its_own_size() -> Outcome {
    let presets = Presets::shipped()?;
    let (width, height) = (60, 40);
    let tight = made(width, height, |x, _| {
        if x >= 52 {
            [1.0, 0.8, 0.2, 1.0]
        } else {
            [0.05, 0.05, 0.05, 1.0]
        }
    })?;
    let colour: Vec<f32> = pixels(&tight).flat_map(|[r, g, b, _]| [r, g, b]).collect();
    for recipe in [
        BLOOM,
        r#"{"steps": [{"effect": "gaussian_blur", "params": {"radius": 8.0}}]}"#,
    ] {
        let checked = Recipe::from_json(recipe)?.checked(&presets)?;
        let alone = bullterrier_image::Photo::new(bullterrier_image::Picture::new(height, width, &colour)?)
            .render(
                &checked,
                &presets,
                bullterrier_image::Options::default().with_canvas(CANVAS, CANVAS),
            )?
            .into_values();
        let shown = render(Layer::picture(tight.clone()).look(recipe)?)?;
        for y in 0..height {
            for x in 0..width {
                let ours = pixel(&shown, x, y);
                let at = (y * width + x) * 3;
                assert_eq!(
                    ours.map(f32::to_bits)[..3],
                    alone[at..at + 3].iter().map(|v| v.to_bits()).collect::<Vec<_>>()[..],
                    "{recipe} at ({x}, {y})"
                );
                assert!(exactly_one(ours[3]), "alpha {} at ({x}, {y})", ours[3]);
            }
        }
    }
    Ok(())
}

fn exactly_one(alpha: f32) -> bool {
    alpha.to_bits() == 1.0_f32.to_bits()
}

/// The report of a bloom "tinting the whole box": a bloom on a drawing whose frame is only a little larger than its content has a
/// halo wider than the frame, and the frame cut it, a lit rectangle with an edge. A transparent pixel is black to the look, and
/// the bloom of black is black: what showed was the halo cut at the frame. With room the halo goes on past the frame, and the
/// picture is image's render of the content on a frame as large as the halo needs (to the rounding of a blur on another size).
#[test]
fn a_halo_wider_than_the_frame_goes_on_past_the_frame_edge() -> Outcome {
    let room = declared(BLOOM)?;
    let shown = render(glowing(drawing(2), BLOOM, true)?)?;
    let (row, edge) = (AT + SIDE / 2, AT - 2);
    assert!(pixel(&shown, edge - 1, row)[3] > 0.0, "the halo stops at the frame");
    let difference = against_image(&shown, &by_image(BLOOM, room + 2, false)?, room + 2, true);
    assert!(difference < 1e-4, "differs from image's render by {difference}");
    Ok(())
}

/// A layer mask is multiplied into the grown raster where it lands, so a mask that shows only the left of the canvas cuts the
/// halo on the right and leaves it on the left; a mask by the layer's own map goes on past the box as the map is at the box's
/// edge, so a map of one half halves the halo as it halves the box.
#[test]
fn masks_apply_to_the_halo_a_light_layer_shows() -> Outcome {
    let open = render(glowing(drawing(0), BLOOM, true)?)?;
    let left_half = Layer::fill("#ffffff")?.fit([0.0, 0.0, 48.0, 96.0], Fit::Stretch, [0.5, 0.5])?;
    let cut = render(glowing(drawing(0), BLOOM, true)?.mask(left_half)?)?;
    for y in 0..CANVAS as usize {
        for x in 0..CANVAS as usize {
            let light = |[red, green, blue, alpha]: [f32; 4]| [red * alpha, green * alpha, blue * alpha, alpha];
            let expected = if x < 48 { light(pixel(&open, x, y)) } else { [0.0; 4] };
            assert!(
                crate::common::close(light(pixel(&cut, x, y)), expected, 1e-6),
                "({x}, {y})"
            );
        }
    }
    let half = Map::new(SIDE, SIDE, vec![0.5; SIDE * SIDE])?;
    let faint = render(glowing(picture(0), BLOOM, true)?.map("matte", half)?.mask("matte")?)?;
    let strong = render(glowing(picture(0), BLOOM, true)?)?;
    let spot = (AT - 3, AT + SIDE / 2);
    let (faint, strong) = (pixel(&faint, spot.0, spot.1), pixel(&strong, spot.0, spot.1));
    // the colour of a light layer is its premultiplied colour: half the alpha is half the light, and the halo is not cut
    assert!(
        strong[3] > 0.0 && (faint[3] * 2.0 - strong[3]).abs() < 1e-5,
        "{faint:?} {strong:?}"
    );
    Ok(())
}

/// A look that spreads none (a vignette: it darkens, it paints nothing outside) gets no room, so image's own render of the
/// layer's picture is what the layer shows, bit for bit; so does one that image cannot bound (a swirl about the centre).
#[test]
fn a_look_that_spreads_none_or_cannot_be_bounded_gets_no_room() -> Outcome {
    let presets = Presets::shipped()?;
    for recipe in [
        r#"{"steps": [{"effect": "vignette"}]}"#,
        r#"{"steps": [{"effect": "swirl", "params": {"angle": 90.0}}]}"#,
    ] {
        let side = 40;
        let tight = made(side, side, |x, y| {
            [
                f32::from(u8::try_from(x).unwrap_or(0)) / 40.0,
                f32::from(u8::try_from(y).unwrap_or(0)) / 40.0,
                0.5,
                1.0,
            ]
        })?;
        let colour: Vec<f32> = pixels(&tight).flat_map(|[r, g, b, _]| [r, g, b]).collect();
        let checked = Recipe::from_json(recipe)?.checked(&presets)?;
        let alone = bullterrier_image::Photo::new(bullterrier_image::Picture::new(side, side, &colour)?)
            .render(
                &checked,
                &presets,
                bullterrier_image::Options::default().with_canvas(CANVAS, CANVAS),
            )?
            .into_values();
        let shown = render(Layer::picture(tight).look(recipe)?.as_light(true))?;
        // the raster is lit as light is: its brightest channel is its alpha, so compare the colour over black
        for y in 0..side {
            for x in 0..side {
                let [r, g, b, a] = pixel(&shown, x, y);
                let at = (y * side + x) * 3;
                let over_black = [r * a, g * a, b * a];
                let expected = [alone[at], alone[at + 1], alone[at + 2]];
                assert!(
                    (0..3).all(|c| (over_black[c] - expected[c]).abs() < 1e-6),
                    "{recipe}: ({x}, {y}) {over_black:?} against {expected:?}"
                );
            }
        }
        assert_eq!(
            lit_extent(&shown),
            Some((0, side - 1, 0, side - 1)),
            "{recipe}: nothing past the box"
        );
    }
    Ok(())
}

/// A look that mixes a halo with an effect that places itself by the picture (a vignette): the vignette is placed by the layer's box,
/// not by the box grown by the room (image's `Options::with_room`), so the layer's own corners are darkened as image darkens a
/// render of the box, and the halo shows past it. The oracle is image's render of the box grown by the room, told it is a box with
/// room; the same render without that differs, so the test fails where the vignette is placed by the grown picture.
#[test]
fn a_vignette_with_a_halo_is_placed_by_the_box() -> Outcome {
    const BOTH: &str = r#"{"steps": [{"effect": "vignette"}, {"effect": "bloom", "params": {"radius": 12.0, "threshold": 0.2, "amount": 1.0}}]}"#;
    let room = declared(BOTH)?;
    let shown = render(glowing(picture(0), BOTH, true)?)?;
    let difference = against_image(&shown, &by_image(BOTH, room, true)?, room, true);
    assert!(
        difference < 1e-6,
        "differs from image's render of the box with room by {difference}"
    );
    let placed_by_grown = against_image(&shown, &by_image(BOTH, room, false)?, room, true);
    assert!(
        placed_by_grown > 0.01,
        "the oracle must tell the box from the grown picture: they differ by {placed_by_grown}"
    );
    Ok(())
}

/// Looks add their rooms: two blurs, one after the other, are image's render of the two-step recipe on a frame grown by the room
/// of the two (the second halo starts where the first ends), and a frame grown by one blur's room is too small for them.
#[test]
fn the_rooms_of_the_looks_of_a_layer_add() -> Outcome {
    const BLUR: &str = r#"{"steps": [{"effect": "gaussian_blur", "params": {"radius": 20.0}}]}"#;
    const TWICE: &str = r#"{"steps": [{"effect": "gaussian_blur", "params": {"radius": 20.0}}, {"effect": "gaussian_blur", "params": {"radius": 20.0}}]}"#;
    let (one, two) = (declared(BLUR)?, declared(TWICE)?);
    assert_eq!(two, 2 * one);
    let shown = render(glowing(picture(0), BLUR, true)?.look(BLUR)?)?;
    let difference = against_image(&shown, &by_image(TWICE, two, false)?, two, true);
    assert!(difference < 1e-5, "differs by {difference}");
    Ok(())
}
