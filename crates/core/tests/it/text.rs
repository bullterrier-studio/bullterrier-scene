//! Type as a layer: set by vector at the size it will have, its origin the text's anchor, its box the glyphs' bounds. The
//! glyphs of a font are not worked out by hand, so the oracle is vector's own drawing of the same text, and the rest are
//! properties (what is painted lies in the box, the area follows the square of the scale, the alignment moves the text by its
//! width).

use bullterrier_scene::{Error, Fit, Layer, Mask, Place, Raster, Text};
use bullterrier_vector::{Color, Drawing, Font, Size, Values};

use crate::common::{Outcome, pixels, scene, solid};

const FONT: &str = "bricolage_grotesque";

fn type_of(content: &str, text: &Text) -> Result<Layer, Error> {
    Layer::text(content, text)
}

/// The pixels of `raster` painted (an alpha above a millionth: the edge of a box that ends on a pixel's edge can leave 1e-14
/// of rounding in the next), as `(column, row)`.
fn painted(raster: &Raster) -> Vec<(usize, usize)> {
    pixels(raster)
        .enumerate()
        .filter(|(_, pixel)| pixel[3] > 1e-6)
        .map(|(at, _)| (at % raster.width(), at / raster.width()))
        .collect()
}

/// `(first column, last column, first row, last row)` of the painted pixels.
fn extent(raster: &Raster) -> Option<(usize, usize, usize, usize)> {
    let found = painted(raster);
    Some((
        found.iter().map(|at| at.0).min()?,
        found.iter().map(|at| at.0).max()?,
        found.iter().map(|at| at.1).min()?,
        found.iter().map(|at| at.1).max()?,
    ))
}

/// Vector's own picture of "Hi" at 100 design pixels with its anchor at `anchor` on a frame of `canvas`, filled `#336699`, at
/// `size` on the long side: the pixels a text layer placed at that anchor must match.
fn vectors_own(
    anchor: (f64, f64),
    canvas: (u32, u32),
    size: u32,
) -> Result<(Vec<f32>, usize), Box<dyn std::error::Error>> {
    let font = Font::bundled(FONT)?;
    let drawing = bullterrier_vector::text("Hi", bullterrier_vector::Text::new(&font).size(100.0).anchor(anchor))
        .fill(Color::parse("#336699")?);
    let framed = Drawing::new([drawing]).with_size(f64::from(canvas.0), f64::from(canvas.1));
    let long = f64::from(canvas.0.max(canvas.1));
    let (columns, rows) = (
        (f64::from(canvas.0) * f64::from(size) / long).ceil(),
        (f64::from(canvas.1) * f64::from(size) / long).ceil(),
    );
    let bytes = framed.to_rgba(Size::px(columns, rows), &Values::new())?;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a side of at most a few thousand pixels"
    )]
    Ok((
        bytes.iter().map(|&byte| f32::from(byte) / 255.0).collect(),
        columns as usize,
    ))
}

/// `pixels` (straight RGBA) as premultiplied: where a pixel is nearly clear its straight colour is a quantisation of nothing
/// (a byte of alpha holds colour to a few levels), so layers and vector are compared as the light they carry.
fn premultiplied(pixels: &[f32]) -> Vec<f32> {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[red, green, blue, alpha]| [red * alpha, green * alpha, blue * alpha, alpha])
        .collect()
}

/// The layer is vector's own text drawing, offset by the anchor: placed by `dx`, `dy` it is the same picture as vector's with the
/// anchor at that point, at whole and at fractional anchors and at whole and at fractional output scales (the canvases are
/// chosen so that vector's own picture has no slack either: its design pixel is the same on both axes). Vector's picture is
/// 8-bit, so the two are the same to a level or so: measured, at most 0.05% of the premultiplied values differ by more than one
/// level of 255 and none by more than 1.25 levels, on and off the whole pixels of anchor and scale.
#[test]
fn a_text_layer_is_vectors_own_text_drawing_offset_by_the_anchor() -> Outcome {
    // (anchor, canvas, size)
    type Row = ((f64, f64), (u32, u32), u32);
    let rows: [Row; 9] = [
        ((40.0, 140.0), (400, 200), 400),
        ((10.3, 140.7), (400, 200), 400),
        ((10.5, 140.0), (400, 200), 400),
        ((10.0, 140.5), (400, 200), 400),
        ((10.3, 140.7), (400, 200), 600),
        ((10.3, 140.7), (400, 200), 500),
        ((10.3, 140.7), (400, 200), 300),
        ((10.3, 140.7), (400, 400), 437),
        ((40.0, 140.0), (400, 400), 437),
    ];
    let level = 1.0 / 255.0;
    for (anchor, canvas, size) in rows {
        let layer = type_of("Hi", &Text::new(FONT).option("size", 100.0).color("#336699"))?
            .place(Place::new().dx(anchor.0).dy(anchor.1))?;
        let drawn = premultiplied(scene(canvas.0, canvas.1, [layer])?.render(size)?.pixels());
        let own = premultiplied(&vectors_own(anchor, canvas, size)?.0);
        assert_eq!(drawn.len(), own.len(), "{anchor:?} at {size}");
        let differences: Vec<f32> = drawn
            .iter()
            .zip(&own)
            .map(|(got, expected)| (got - expected).abs())
            .collect();
        let worst = differences.iter().copied().fold(0.0, f32::max);
        let differing = differences
            .iter()
            .filter(|&&difference| difference > level + 1e-6)
            .count();
        assert!(
            worst <= 1.25 * level,
            "{anchor:?} at {size}: the layer differs from vector's drawing by {}",
            worst / level
        );
        assert!(
            differing * 2000 <= differences.len(),
            "{anchor:?} at {size}: {differing} of {} values differ",
            differences.len()
        );
        assert!(
            drawn.chunks(4).filter(|pixel| pixel[3] > 0.5).count() > 100,
            "{anchor:?} at {size}"
        );
    }
    Ok(())
}

/// What is painted lies inside the box: contained in a box, the letters fill it along the axis that binds (the box is the
/// glyphs' bounds, so within a pixel of each edge) and none is outside it.
#[test]
fn what_a_text_paints_fills_its_box_and_does_not_leave_it() -> Outcome {
    let layer = type_of("Hi", &Text::new(FONT).option("size", 100.0))?.fit(
        [50.0, 40.0, 300.0, 120.0],
        Fit::Contain,
        [0.5, 0.5],
    )?;
    let drawn = scene(400, 200, [layer])?.render(400)?;
    let (left, right, top, bottom) = extent(&drawn).ok_or("nothing is painted")?;
    // the text is taller than it is wide here, so the height binds
    assert!(
        left >= 50 && right < 350 && top >= 40 && bottom < 160,
        "{left} {right} {top} {bottom}"
    );
    assert!(top <= 42 && bottom >= 157, "{top} {bottom}");
    Ok(())
}

/// Scale 2 draws the same text at twice the size (sharp, not enlarged pixels): four times the area, to the rounding of the
/// edges.
#[test]
fn a_text_layer_scaled_covers_the_square_of_the_scale() -> Outcome {
    let area = |scale: f64| -> Result<f64, Box<dyn std::error::Error>> {
        let layer = type_of("Hi", &Text::new(FONT).option("size", 60.0))?
            .place(Place::new().dx(20.0).dy(100.0).scale(scale).anchor(0.0, 0.0))?;
        let drawn = scene(400, 200, [layer])?.render(400)?;
        Ok(pixels(&drawn).map(|pixel| f64::from(pixel[3])).sum())
    };
    let (small, large) = (area(1.0)?, area(2.0)?);
    assert!(small > 300.0, "{small}");
    assert!((large / small - 4.0).abs() <= 0.05 * 4.0, "{large} is not 4 x {small}");
    Ok(())
}

/// `align` puts the anchor at the left end, the middle or the right end of the first baseline: the painted columns start at the
/// anchor, are centred on it, or end at it (the side bearings of the first and last letters are a few pixels).
#[test]
fn align_puts_the_anchor_at_the_left_the_middle_or_the_right_of_the_text() -> Outcome {
    let anchor = 200_usize;
    let columns = |align: &str| -> Result<(usize, usize), Box<dyn std::error::Error>> {
        let layer = type_of("MIDDLE", &Text::new(FONT).option("size", 60.0).option("align", align))?
            .place(Place::new().dx(200.0).dy(100.0))?;
        let (left, right, _, _) = extent(&scene(400, 200, [layer])?.render(400)?).ok_or("nothing is painted")?;
        Ok((left, right))
    };
    let (left_of, left_end) = columns("left")?;
    let (right_of, right_end) = columns("right")?;
    let (middle_of, middle_end) = columns("center")?;
    assert!(left_of.abs_diff(anchor) <= 6, "left: {left_of}");
    assert!(right_end.abs_diff(anchor) <= 6, "right: {right_end}");
    assert!(
        (middle_of + middle_end).abs_diff(2 * anchor) <= 12,
        "centre: {middle_of}..{middle_end}"
    );
    // the three are the same width, moved by the width of the text
    let (a, b, c) = (left_end - left_of, right_end - right_of, middle_end - middle_of);
    assert!(a.abs_diff(b) <= 2 && a.abs_diff(c) <= 2, "{a} {b} {c}");
    assert!(right_of.abs_diff(left_of) + 6 >= a && right_of < left_of);
    Ok(())
}

/// Each row: a text that is refused when the layer is made, and the word its refusal carries (the option, or the field).
#[test]
fn a_text_that_vector_or_scene_refuses_is_refused_at_the_call_naming_the_option() -> Outcome {
    let good = || Text::new(FONT);
    let rows: [(&str, Result<Layer, Error>, &str); 14] = [
        ("a size", type_of("Hi", &good().option("size", 40.0)), ""),
        (
            "axes",
            type_of("Hi", &good().option("axes", serde_json::json!({"wght": 400}))),
            "",
        ),
        ("a size of zero", type_of("Hi", &good().option("size", 0.0)), "size"),
        ("a negative size", type_of("Hi", &good().option("size", -4.0)), "size"),
        (
            "an unknown option",
            type_of("Hi", &good().option("bogus", 1.0)),
            "bogus",
        ),
        (
            "a word that is no alignment",
            type_of("Hi", &good().option("align", "middle")),
            "align",
        ),
        (
            "an axis the font has not",
            type_of("Hi", &good().option("axes", serde_json::json!({"ZZZZ": 3}))),
            "ZZZZ",
        ),
        (
            "an anchor",
            type_of("Hi", &good().option("anchor", serde_json::json!([1, 2]))),
            "anchor",
        ),
        (
            "a shape to set the text in",
            type_of("Hi", &good().option("inside", "M0 0 L9 0 L9 9 Z")),
            "inside",
        ),
        (
            "an expression",
            type_of("Hi", &good().option("size", serde_json::json!({"param": "s"}))),
            "size",
        ),
        ("a colour that is none", type_of("Hi", &good().color("red")), "color"),
        (
            "a font that does not exist",
            type_of("Hi", &Text::new("no_such_font")),
            "no_such_font",
        ),
        ("no text", type_of("", &good()), "content"),
        ("only a space", type_of("   ", &good()), "content"),
    ];
    for (what, made, named) in rows {
        match (made, named.is_empty()) {
            (Ok(_), true) => {}
            (Err(Error::Invalid { address, message }), false) => {
                assert!(
                    address.contains(named) || message.contains(named),
                    "{what}: {address}: {message}"
                );
            }
            (other, _) => return Err(format!("{what}: {other:?}").into()),
        }
    }
    Ok(())
}

/// A font that is not a bundled name is listed: the refusal names the fonts there are.
#[test]
fn an_unknown_bundled_font_is_refused_listing_the_fonts_there_are() {
    let refused = Layer::text("Hi", &Text::new("no_such_font"));
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message }) if address == "font" && message.contains(FONT)),
        "{refused:?}"
    );
}

/// A font file that is not there is a read error naming it; one that is not a font is refused naming the file.
#[test]
fn a_font_file_that_cannot_be_read_or_is_no_font_is_refused() -> Outcome {
    let folder = tempfile::tempdir()?;
    let missing = Layer::text("Hi", &Text::from_file(folder.path().join("none.ttf")));
    assert!(
        matches!(&missing, Err(Error::Read { path, .. }) if path.ends_with("none.ttf")),
        "{missing:?}"
    );
    let bad = folder.path().join("bad.ttf");
    std::fs::write(&bad, b"not a font")?;
    let refused = Layer::text("Hi", &Text::from_file(&bad));
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message }) if address == "font" && message.contains("bad.ttf")),
        "{refused:?}"
    );
    Ok(())
}

/// A text is a layer like any: it masks (a picture clipped into the letters), and masked by the text the picture shows only
/// where the letters are, as much as they are.
#[test]
fn a_picture_masked_by_text_is_the_text_as_its_alpha() -> Outcome {
    let letters = || {
        type_of("Hi", &Text::new(FONT).option("size", 100.0))
            .and_then(|layer| layer.place(Place::new().dx(40.0).dy(140.0)))
    };
    let alone = scene(400, 200, [letters()?])?.render(400)?;
    let clipped = scene(
        400,
        200,
        [Layer::picture(solid(400, 200, [0.9, 0.2, 0.1, 1.0])?).mask(letters()?)?],
    )?
    .render(400)?;
    for (got, expected) in pixels(&clipped).zip(pixels(&alone)) {
        assert!((got[3] - expected[3]).abs() <= 1e-6);
        if got[3] > 0.0 {
            assert!((got[0] - 0.9).abs() <= 1e-6 && (got[1] - 0.2).abs() <= 1e-6, "{got:?}");
        }
    }
    // read as luminance, black letters hide nothing (they are black) and white ones keep everything
    let white = type_of("Hi", &Text::new(FONT).option("size", 100.0).color("#ffffff"))?
        .place(Place::new().dx(40.0).dy(140.0))?;
    let by_light = scene(
        400,
        200,
        [Layer::fill("#ff0000")?.mask(Mask::new(white).using(bullterrier_scene::Use::Luminance))?],
    )?
    .render(400)?;
    for (got, expected) in pixels(&by_light).zip(pixels(&alone)) {
        assert!((got[3] - expected[3]).abs() <= 1e-6);
    }
    Ok(())
}

#[test]
fn a_text_layer_draws_the_same_picture_each_time_and_the_layer_is_not_changed_by_placing_it() -> Outcome {
    let base = type_of("Hi", &Text::new(FONT).option("size", 80.0))?.place(Place::new().dy(80.0))?;
    let moved = base.clone().place(Place::new().dx(10.0))?;
    let first = scene(200, 100, [base.clone()])?.render(200)?;
    assert_ne!(painted(&first).len(), 0);
    assert_eq!(first.pixels(), scene(200, 100, [base])?.render(200)?.pixels());
    assert_ne!(first.pixels(), scene(200, 100, [moved])?.render(200)?.pixels());
    Ok(())
}

/// A text fitted by `cover` into a box narrower than its bounds is cut about the middle of its bounds, whichever side of the
/// anchor the bounds are on (right-aligned type lies to the left of its anchor): the box is filled with letters and none is
/// outside it.
#[test]
fn a_text_covers_a_box_from_the_middle_of_its_bounds_wherever_the_anchor_is() -> Outcome {
    for align in ["left", "center", "right"] {
        let layer = type_of("MIDDLE", &Text::new(FONT).option("size", 100.0).option("align", align))?.fit(
            [100.0, 50.0, 100.0, 100.0],
            Fit::Cover,
            [0.5, 0.5],
        )?;
        let drawn = scene(300, 200, [layer])?.render(300)?;
        let inside = painted(&drawn)
            .iter()
            .filter(|&&(x, y)| (100..200).contains(&x) && (50..150).contains(&y))
            .count();
        assert_eq!(inside, painted(&drawn).len(), "{align}: letters outside the box");
        assert!(inside > 1_500, "{align}: {inside} pixels of 10,000 are painted");
    }
    Ok(())
}

/// A text `fit` with `stretch` fills its box: the letters span it, from within a pixel of one edge to within a pixel of the
/// other, and none is outside it (the box is the glyphs' bounds).
#[test]
fn a_text_stretched_into_a_box_spans_the_box() -> Outcome {
    let layer = type_of("Hi", &Text::new(FONT).option("size", 100.0))?.fit(
        [0.0, 0.0, 400.0, 60.0],
        Fit::Stretch,
        [0.5, 0.5],
    )?;
    let drawn = scene(400, 60, [layer])?.render(400)?;
    let (left, right, top, bottom) = extent(&drawn).ok_or("nothing is painted")?;
    assert!(
        left <= 1 && right >= 398 && top <= 1 && bottom >= 58,
        "{left} {right} {top} {bottom}"
    );
    Ok(())
}
