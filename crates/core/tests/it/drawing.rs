//! A vector drawing as a layer: its box is its frame at the canvas origin, rasterised by vector at the output scale.
//! Expected pixels are counted by hand from the rectangle's coordinates.

use bullterrier_scene::{Error, Layer};

use crate::common::{Outcome, all_exactly, close, pixel, pixels, rectangle, scene};

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

/// A 40 by 20 frame with a red rectangle from (10, 5) to (30, 15): 20 by 10 design pixels.
fn red_rectangle() -> String {
    rectangle((40, 20), (10.0, 5.0), (30.0, 15.0), "#ff0000")
}

/// At scale `s` the rectangle covers exactly `20 s` by `10 s` pixels from (`10 s`, `5 s`), opaque red; every other pixel is
/// clear.
#[test]
fn a_rectangle_covers_exactly_the_hand_counted_pixels_at_every_whole_scale() -> Outcome {
    let canvas = scene(40, 20, [Layer::drawing(&red_rectangle())?])?;
    for scale in [1_usize, 2, 4] {
        let drawn = canvas.render(40 * u32::try_from(scale)?)?;
        assert_eq!((drawn.width(), drawn.height()), (40 * scale, 20 * scale));
        for y in 0..drawn.height() {
            for x in 0..drawn.width() {
                let inside = (10 * scale..30 * scale).contains(&x) && (5 * scale..15 * scale).contains(&y);
                let expected = if inside { RED } else { [0.0; 4] };
                assert!(
                    close(pixel(&drawn, x, y), expected, 0.0),
                    "({x}, {y}) at {scale}x: {:?}",
                    pixel(&drawn, x, y)
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_drawing_without_a_frame_is_refused_at_the_call_naming_the_frame() {
    let unframed = r##"{"version":1,"root":[{"path":"M0 0 L10 0 L10 10 L0 10 Z"},{"fill":{"color":"#ff0000"}}]}"##;
    let refused = Layer::drawing(unframed);
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message }) if address == "drawing" && message.contains("frame")),
        "{refused:?}"
    );
}

/// Each row: a document a drawing layer cannot take, and the word its refusal carries.
#[test]
fn a_document_vector_cannot_read_is_refused_at_the_call_by_its_own_words() {
    let framed = red_rectangle();
    let rows = [
        ("not JSON", "{".to_owned(), "drawing"),
        (
            "a version it does not read",
            framed.replace(r#""version":1"#, r#""version":2"#),
            "version",
        ),
        (
            "an unknown key",
            framed.replace(r#""version":1"#, r#""version":1,"bogus":1"#),
            "bogus",
        ),
    ];
    for (what, text, named) in rows {
        let refused = Layer::drawing(&text);
        assert!(
            matches!(&refused, Err(Error::Format { pointer, message }) if pointer.starts_with("/drawing") && format!("{pointer} {message}").contains(named)),
            "{what}: {refused:?}"
        );
    }
}

#[test]
fn a_known_good_drawing_is_taken() {
    assert!(Layer::drawing(&red_rectangle()).is_ok());
}

/// A drawing on a canvas larger than its frame sits at the origin; on a smaller canvas it is cut.
#[test]
fn a_drawing_sits_at_the_canvas_origin_and_is_cut_at_the_canvas() -> Outcome {
    let layer = Layer::drawing(&rectangle((8, 8), (0.0, 0.0), (8.0, 8.0), "#ff0000"))?;
    let larger = scene(16, 16, [layer.clone()])?.render(16)?;
    for y in 0..16 {
        for x in 0..16 {
            let expected = if x < 8 && y < 8 { RED } else { [0.0; 4] };
            assert!(close(pixel(&larger, x, y), expected, 0.0), "({x}, {y})");
        }
    }
    let smaller = scene(4, 4, [layer])?.render(4)?;
    assert!(all_exactly(&smaller, RED));
    Ok(())
}

#[test]
fn the_layer_opacity_scales_a_drawings_alpha() -> Outcome {
    let layer = Layer::drawing(&rectangle((4, 4), (0.0, 0.0), (4.0, 4.0), "#ff0000"))?.opacity(0.5)?;
    let drawn = scene(4, 4, [layer])?.render(4)?;
    assert!(all_exactly(&drawn, [1.0, 0.0, 0.0, 0.5]));
    Ok(())
}

/// The same drawing at two sizes is the same picture in less detail: a rectangle whose edge falls half way through a
/// pixel at 1x is exact at 2x, and the 2x render averaged down is the 1x render (to a level of rounding).
#[test]
fn the_same_drawing_at_two_sizes_agrees_when_averaged_down() -> Outcome {
    let canvas = scene(
        40,
        20,
        [Layer::drawing(&rectangle(
            (40, 20),
            (10.5, 5.5),
            (30.5, 15.5),
            "#3366cc",
        ))?],
    )?;
    let (once, twice) = (canvas.render(40)?, canvas.render(80)?);
    // averaging is done on premultiplied colour, as the renderer mixes it
    let premultiplied = |p: [f32; 4]| [p[0] * p[3], p[1] * p[3], p[2] * p[3], p[3]];
    for y in 0..20 {
        for x in 0..40 {
            let mut mixed = [0.0_f32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                for (sum, value) in mixed
                    .iter_mut()
                    .zip(premultiplied(pixel(&twice, 2 * x + dx, 2 * y + dy)))
                {
                    *sum += value / 4.0;
                }
            }
            let at = premultiplied(pixel(&once, x, y));
            assert!(close(at, mixed, 2.0 / 255.0), "({x}, {y}): {at:?} is not {mixed:?}");
        }
    }
    Ok(())
}

#[test]
fn a_drawing_over_a_fill_follows_the_normal_blend() -> Outcome {
    let drawn = scene(
        4,
        4,
        [
            Layer::fill("#0000ff")?,
            Layer::drawing(&rectangle((4, 4), (0.0, 0.0), (2.0, 4.0), "#ff0000"))?,
        ],
    )?
    .render(4)?;
    for (x, expected) in [(0, RED), (1, RED), (2, [0.0, 0.0, 1.0, 1.0]), (3, [0.0, 0.0, 1.0, 1.0])] {
        assert!(
            pixels(&drawn).count() == 16 && close(pixel(&drawn, x, 1), expected, 0.0),
            "column {x}"
        );
    }
    Ok(())
}

/// A frame of 60 by 60 design pixels on a canvas of 6 at size 1200 is drawn 12,000 by 12,000: refused, naming the layer
/// and the size, as a picture is.
#[test]
fn a_drawing_whose_drawn_size_passes_2_pow_27_pixels_is_refused_naming_the_layer_and_its_size() -> Outcome {
    let layer = Layer::drawing(&rectangle((60, 60), (0.0, 0.0), (60.0, 60.0), "#ff0000"))?.named("mark")?;
    let refused = scene(6, 6, [layer])?.render(1200);
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message })
            if address.starts_with("mark") && message.contains("12000 x 12000") && message.contains("134217728")),
        "{refused:?}"
    );
    Ok(())
}

/// A drawing whose frame is the canvas covers the canvas whole at any size: every pixel, the edge included, is opaque. Vector
/// draws a frame at one scale and centres it in a raster whose short side is rounded up, which left a quarter to a half pixel
/// of slack and a translucent border (alpha 0.749 at 1,920 by 1,080 drawn at 1,000).
#[test]
fn a_drawing_that_fills_its_frame_is_opaque_to_the_edge_at_any_size() -> Outcome {
    for ((width, height), size) in [
        ((1920, 1080), 1000),
        ((1081, 1350), 1000),
        ((1000, 707), 900),
        ((1800, 2400), 1234),
        ((40, 20), 61),
    ] {
        let layer = Layer::drawing(&rectangle(
            (width, height),
            (0.0, 0.0),
            (f64::from(width), f64::from(height)),
            "#ff0000",
        ))?;
        let drawn = scene(width, height, [layer])?.render(size)?;
        let faint = pixels(&drawn).filter(|pixel| pixel[3] < 1.0 - 1e-6).count();
        assert_eq!(
            faint,
            0,
            "{width} x {height} at {size}: {faint} of {} pixels are not opaque",
            drawn.width() * drawn.height()
        );
    }
    Ok(())
}

/// `stretch` stretches a drawing as it does a picture: a square drawing fitted to a box twice as wide covers all of it.
#[test]
fn a_drawing_stretched_into_a_box_covers_the_box() -> Outcome {
    let layer = Layer::drawing(&rectangle((100, 100), (0.0, 0.0), (100.0, 100.0), "#ff0000"))?.fit(
        [0.0, 0.0, 200.0, 100.0],
        bullterrier_scene::Fit::Stretch,
        [0.5, 0.5],
    )?;
    let drawn = scene(200, 100, [layer])?.render(200)?;
    assert!(
        pixels(&drawn).all(|pixel| close(pixel, RED, 1e-6)),
        "the box is not covered"
    );
    // the half that is not red stays clear: a drawing of the left half only is stretched to the left half of the box
    let half = Layer::drawing(&rectangle((100, 100), (0.0, 0.0), (50.0, 100.0), "#ff0000"))?.fit(
        [0.0, 0.0, 200.0, 100.0],
        bullterrier_scene::Fit::Stretch,
        [0.5, 0.5],
    )?;
    let drawn = scene(200, 100, [half])?.render(200)?;
    for (index, pixel) in pixels(&drawn).enumerate() {
        let expected = if index % 200 < 100 { RED } else { [0.0; 4] };
        assert!(close(pixel, expected, 1e-6), "pixel {index}: {pixel:?}");
    }
    Ok(())
}
