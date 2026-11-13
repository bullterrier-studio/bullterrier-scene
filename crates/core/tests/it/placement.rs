//! Placement: `fit` and `place`. Every expected value is worked out by hand from the stated rule (a permutation of pixels, a
//! matrix applied to a corner, the area of a rectangle), or is the same crate's render of the same thing in another way
//! (a layer at twice the scale against the scene at twice the size). Where a result is a permutation or a copy it is
//! compared bit for bit, with a render of the layer unplaced standing for "the layer's pixels" (both go through the same
//! premultiplied round trip of the blend, so a translucent pixel keeps its bits).

use bullterrier_scene::{Deg, Error, Fit, Layer, Place, Raster};
use proptest::prelude::*;

use crate::common::{Outcome, close, exactly, made, numbered, pixel, pixels, rectangle, render, scene, solid, varied};

const CLEAR: [f32; 4] = [0.0; 4];

/// Where a quarter turn about the middle of a `w` by `h` picture sends pixel (`i`, `j`): `(w, h, i, j)` to a pixel.
type Turn = fn(i64, i64, i64, i64) -> (i64, i64);

/// Where a quarter turn about the top left corner sends pixel (`i`, `j`).
type CornerTurn = fn(i64, i64) -> (i64, i64);

/// A `fit` row: what it is, the box, the align, and the field a refusal names (`None` for a good row).
type FitRow<'a> = (&'a str, [f64; 4], [f64; 2], Option<&'a str>);

/// Whether `placed` is, bit for bit, `unplaced` moved so that pixel (`x`, `y`) of the unplaced picture is at
/// `destination(x, y)`, and clear everywhere that no pixel moves to. `Some(message)` says where it is not.
fn permuted(placed: &Raster, unplaced: &Raster, destination: impl Fn(i64, i64) -> (i64, i64)) -> Option<String> {
    let (width, height) = (
        i64::try_from(placed.width()).unwrap_or(0),
        i64::try_from(placed.height()).unwrap_or(0),
    );
    let mut expected = vec![CLEAR; usize::try_from(width * height).unwrap_or(0)];
    for y in 0..i64::try_from(unplaced.height()).unwrap_or(0) {
        for x in 0..i64::try_from(unplaced.width()).unwrap_or(0) {
            let (to_x, to_y) = destination(x, y);
            if (0..width).contains(&to_x) && (0..height).contains(&to_y) {
                let (source_x, source_y) = (usize::try_from(x).unwrap_or(0), usize::try_from(y).unwrap_or(0));
                expected[usize::try_from(to_y * width + to_x).unwrap_or(0)] = pixel(unplaced, source_x, source_y);
            }
        }
    }
    pixels(placed)
        .zip(expected)
        .enumerate()
        .find(|(_, (got, expected))| !exactly(*got, *expected))
        .map(|(at, (got, expected))| format!("pixel {at}: {got:?} is not {expected:?}"))
}

#[test]
fn a_layer_placed_with_nothing_set_or_fitted_into_its_own_box_is_the_layer() -> Outcome {
    let own_box = [0.0, 0.0, 6.0, 4.0];
    let cases: Vec<(&str, Layer)> = vec![
        ("place()", Layer::picture(numbered(6, 4)?).place(Place::new())?),
        (
            "every default set",
            Layer::picture(numbered(6, 4)?).place(
                Place::new()
                    .dx(0.0)
                    .dy(0.0)
                    .scale(1.0)
                    .aspect(1.0)
                    .rotate(Deg(0.0))
                    .skew_x(Deg(0.0))
                    .skew_y(Deg(0.0))
                    .flip_x(false)
                    .flip_y(false)
                    .anchor(0.5, 0.5),
            )?,
        ),
        (
            "rotate 360",
            Layer::picture(numbered(6, 4)?).place(Place::new().rotate(Deg(360.0)))?,
        ),
        (
            "rotate -720",
            Layer::picture(numbered(6, 4)?).place(Place::new().rotate(Deg(-720.0)))?,
        ),
        (
            "cover into its own box",
            Layer::picture(numbered(6, 4)?).fit(own_box, Fit::Cover, [0.5, 0.5])?,
        ),
        (
            "contain into its own box",
            Layer::picture(numbered(6, 4)?).fit(own_box, Fit::Contain, [0.0, 1.0])?,
        ),
        (
            "stretch into its own box",
            Layer::picture(numbered(6, 4)?).fit(own_box, Fit::Stretch, [0.5, 0.5])?,
        ),
    ];
    let unplaced = render(Layer::picture(numbered(6, 4)?), (8, 6), 8)?;
    for (what, layer) in cases {
        let placed = render(layer, (8, 6), 8)?;
        assert_eq!(placed.pixels().len(), unplaced.pixels().len(), "{what}");
        assert!(
            placed
                .pixels()
                .iter()
                .zip(unplaced.pixels())
                .all(|(got, expected)| got.to_bits() == expected.to_bits()),
            "{what} changed the picture"
        );
    }
    Ok(())
}

#[test]
fn a_move_by_whole_output_pixels_copies_the_layer_exactly_and_leaves_the_rest_clear() -> Outcome {
    // at size 12 a design pixel is one output pixel; at size 24 it is two, and 1.5 design pixels are 3 output pixels
    let cases = [
        (12, 3.0, 2.0, 3, 2),
        (24, 1.5, 1.0, 3, 2),
        (12, -2.0, -1.0, -2, -1),
        (12, 20.0, 0.0, 20, 0),
    ];
    for (size, dx, dy, to_x, to_y) in cases {
        let layer = Layer::picture(numbered(6, 4)?);
        let unplaced = render(layer.clone(), (12, 10), size)?;
        let placed = render(layer.place(Place::new().dx(dx).dy(dy))?, (12, 10), size)?;
        assert_eq!(
            permuted(&placed, &unplaced, |x, y| (x + to_x, y + to_y)),
            None,
            "size {size}, move ({dx}, {dy})"
        );
    }
    Ok(())
}

/// Half a pixel to the right: an output pixel straddles two source pixels, so it is their average (premultiplied, and these
/// are opaque), the first pixel is half covered by the first source pixel and one pixel past the end is half covered by the
/// last.
#[test]
fn a_move_by_half_a_pixel_is_the_average_of_the_two_pixels_it_lands_between() -> Outcome {
    let levels = [0.0, 0.25, 0.5, 1.0, 0.75];
    let row = made(5, 1, |x, _| [levels[x], 0.0, 0.0, 1.0])?;
    let placed = render(Layer::picture(row).place(Place::new().dx(0.5))?, (8, 1), 8)?;
    let expected = [
        [0.0, 0.0, 0.0, 0.5],   // half of the first pixel, the rest outside the layer
        [0.125, 0.0, 0.0, 1.0], // (0 + 0.25) / 2
        [0.375, 0.0, 0.0, 1.0],
        [0.75, 0.0, 0.0, 1.0],
        [0.875, 0.0, 0.0, 1.0],
        [0.75, 0.0, 0.0, 0.5], // half of the last pixel
        CLEAR,
        CLEAR,
    ];
    for (x, expected) in expected.into_iter().enumerate() {
        assert!(
            close(pixel(&placed, x, 0), expected, 1e-6),
            "{x}: {:?}",
            pixel(&placed, x, 0)
        );
    }
    Ok(())
}

/// Turning by a quarter, a half or three quarters of a turn is a permutation of the pixels (here for a 4 by 2 picture about
/// its middle, `(2, 1)`, a corner of four pixels; and for any size, about its corner), derived by hand: a pixel centre `p`
/// from the pivot `c` goes to `c + (-v, u)` for a quarter turn clockwise on a canvas with y down.
#[test]
fn a_turn_by_a_multiple_of_90_degrees_is_an_exact_permutation_of_the_pixels() -> Outcome {
    let (width, height) = (4_i64, 2_i64);
    let layer = Layer::picture(numbered(4, 2)?);
    let unplaced = render(layer.clone(), (16, 16), 16)?;
    // about the middle of the picture: the picture's pixel (i, j) goes to ..., then the move (dx, dy) of whole pixels
    let about_the_middle: [(f64, Turn); 4] = [
        (90.0, |w, h, i, j| (w / 2 + h / 2 - 1 - j, h / 2 - w / 2 + i)),
        (180.0, |w, h, i, j| (w - 1 - i, h - 1 - j)),
        (270.0, |w, h, i, j| (w / 2 - h / 2 + j, h / 2 + w / 2 - 1 - i)),
        (-90.0, |w, h, i, j| (w / 2 - h / 2 + j, h / 2 + w / 2 - 1 - i)),
    ];
    for (angle, to) in about_the_middle {
        let placed = render(
            layer.clone().place(Place::new().rotate(Deg(angle)).dx(6.0).dy(5.0))?,
            (16, 16),
            16,
        )?;
        assert_eq!(
            permuted(&placed, &unplaced, |i, j| {
                let (x, y) = to(width, height, i, j);
                (x + 6, y + 5)
            }),
            None,
            "rotate {angle}"
        );
    }
    // about the top left corner (anchor (0, 0)): (i, j) is centred at (i + .5, j + .5) from the pivot
    let about_a_corner: [(f64, CornerTurn); 3] = [
        (90.0, |i, j| (-j - 1, i)),
        (180.0, |i, j| (-i - 1, -j - 1)),
        (270.0, |i, j| (j, -i - 1)),
    ];
    for (angle, to) in about_a_corner {
        let placed = render(
            layer
                .clone()
                .place(Place::new().rotate(Deg(angle)).anchor(0.0, 0.0).dx(8.0).dy(9.0))?,
            (16, 16),
            16,
        )?;
        assert_eq!(
            permuted(&placed, &unplaced, |i, j| {
                let (x, y) = to(i, j);
                (x + 8, y + 9)
            }),
            None,
            "rotate {angle} about a corner"
        );
    }
    Ok(())
}

#[test]
fn an_angle_wraps_so_a_whole_number_of_turns_changes_nothing() -> Outcome {
    let layer = Layer::picture(numbered(4, 2)?);
    let turned = |angle: f64| {
        render(
            layer.clone().place(Place::new().rotate(Deg(angle)).dx(6.0).dy(5.0))?,
            (16, 16),
            16,
        )
    };
    for (angle, same_as) in [
        (450.0, 90.0),
        (-270.0, 90.0),
        (1080.0, 0.0),
        (540.0, 180.0),
        (-450.0, 270.0),
    ] {
        assert_eq!(
            turned(angle)?.pixels(),
            turned(same_as)?.pixels(),
            "{angle} is {same_as}"
        );
    }
    Ok(())
}

#[test]
fn a_flip_mirrors_the_pixels_about_the_anchor() -> Outcome {
    let layer = Layer::picture(numbered(4, 6)?);
    let unplaced = render(layer.clone(), (12, 12), 12)?;
    // about the middle (2, 3): column i goes to 3 - i; the row j to 5 - j
    let x = render(layer.clone().place(Place::new().flip_x(true))?, (12, 12), 12)?;
    assert_eq!(permuted(&x, &unplaced, |i, j| (3 - i, j)), None, "flip_x");
    let y = render(layer.clone().place(Place::new().flip_y(true))?, (12, 12), 12)?;
    assert_eq!(permuted(&y, &unplaced, |i, j| (i, 5 - j)), None, "flip_y");
    let both = render(
        layer.clone().place(Place::new().flip_x(true).flip_y(true))?,
        (12, 12),
        12,
    )?;
    assert_eq!(permuted(&both, &unplaced, |i, j| (3 - i, 5 - j)), None, "both");
    // about the bottom edge (anchor (0.5, 1)): row j goes to 2 * 6 - 1 - j, below the picture: a reflection on a floor
    let floor = render(layer.place(Place::new().flip_y(true).anchor(0.5, 1.0))?, (12, 12), 12)?;
    assert_eq!(permuted(&floor, &unplaced, |i, j| (i, 11 - j)), None, "reflection");
    Ok(())
}

/// Flip, then scale, skew and turn: a flip across x and then a quarter turn is not the quarter turn and then the flip.
/// For a 4 by 2 picture about its middle: flip then turn sends the pixel (i, j) to (2 - j, 2 - i); the other way round it
/// would be (j + 1, i - 1).
#[test]
fn the_flip_is_applied_before_the_turn() -> Outcome {
    let layer = Layer::picture(numbered(4, 2)?);
    let unplaced = render(layer.clone(), (16, 16), 16)?;
    let placed = render(
        layer.place(Place::new().flip_x(true).rotate(Deg(90.0)).dx(6.0).dy(6.0))?,
        (16, 16),
        16,
    )?;
    assert_eq!(permuted(&placed, &unplaced, |i, j| (2 - j + 6, 2 - i + 6)), None);
    Ok(())
}

/// A turn and its turn back, on a smooth picture, within the error of two bilinear samples of a curve: the picture is
/// `0.5 + 0.4 sin(x / 5) cos(y / 7)` whose second derivative is below 0.4/25, so one bilinear step errs by under
/// 0.4/25/8 = 0.002; two steps, 0.004; the tolerance is 0.01.
#[test]
fn a_turn_and_the_turn_back_return_the_picture_within_two_bilinear_steps() -> Outcome {
    let smooth = made(24, 24, |x, y| {
        #[expect(clippy::cast_precision_loss, reason = "a small column")]
        let (x, y) = (x as f32, y as f32);
        let level = 0.5 + 0.4 * (x / 5.0).sin() * (y / 7.0).cos();
        [level, 1.0 - level, 0.5, 1.0]
    })?;
    let canvas = (48, 48);
    let turned = render(
        Layer::picture(smooth.clone()).place(Place::new().rotate(Deg(30.0)).dx(12.0).dy(12.0))?,
        canvas,
        48,
    )?;
    // the picture of the first render, turned back about the same pivot (the middle of the canvas)
    let back = render(
        Layer::picture(turned).place(Place::new().rotate(Deg(-30.0)))?,
        canvas,
        48,
    )?;
    let original = render(
        Layer::picture(smooth).place(Place::new().dx(12.0).dy(12.0))?,
        canvas,
        48,
    )?;
    let mut compared = 0;
    for y in 0..48_usize {
        for x in 0..48_usize {
            // the disc of radius 9 about the middle lies inside the picture before and after both turns
            if (f64::from(u32::try_from(x)?) - 23.5).hypot(f64::from(u32::try_from(y)?) - 23.5) <= 9.0 {
                compared += 1;
                assert!(
                    close(pixel(&back, x, y), pixel(&original, x, y), 0.01),
                    "({x}, {y}): {:?} is not {:?}",
                    pixel(&back, x, y),
                    pixel(&original, x, y)
                );
            }
        }
    }
    assert!(compared > 200, "{compared}");
    Ok(())
}

/// `scale` makes the layer at twice the size: a drawing or a picture at `scale=2` about its corner is bit for bit the scene
/// rendered at twice the size (the drawing is drawn at that density, not enlarged), and at `scale=0.5` it is the corner of
/// the scene rendered at half the size.
#[test]
fn a_layer_at_twice_the_scale_is_the_layer_at_twice_the_size() -> Outcome {
    let drawing = Layer::drawing(&rectangle((8, 8), (1.5, 1.0), (6.0, 7.5), "#3366cc"))?;
    let picture = Layer::picture(numbered(8, 8)?);
    for (what, layer) in [("drawing", drawing), ("picture", picture)] {
        let doubled = render(
            layer.clone().place(Place::new().scale(2.0).anchor(0.0, 0.0))?,
            (16, 16),
            16,
        )?;
        let at_twice_the_size = render(layer.clone(), (8, 8), 16)?;
        assert_eq!(doubled.pixels(), at_twice_the_size.pixels(), "{what} at 2");
        // the layer is 8 design pixels, 4 output pixels at half the size: the corner of a canvas of 8 by 8 pixels
        let halved = render(
            layer.clone().place(Place::new().scale(0.5).anchor(0.0, 0.0))?,
            (16, 16),
            16,
        )?;
        let at_half_the_size = render(layer, (16, 16), 8)?;
        for y in 0..16 {
            for x in 0..16 {
                let expected = if x < 8 && y < 8 {
                    pixel(&at_half_the_size, x, y)
                } else {
                    CLEAR
                };
                assert!(exactly(pixel(&halved, x, y), expected), "{what} at 0.5, ({x}, {y})");
            }
        }
    }
    Ok(())
}

/// A look runs at the size the layer has: a vignette on a picture at `scale=2` is the vignette at twice the size, not the
/// vignette of the small picture enlarged.
#[test]
fn a_look_runs_at_the_placed_size() -> Outcome {
    let vignette = r#"{"steps": [{"effect": "vignette", "opacity": 1.0, "blend_mode": "normal"}]}"#;
    let source = varied(8, 8)?;
    let doubled = render(
        Layer::picture(source.clone())
            .look(vignette)?
            .place(Place::new().scale(2.0).anchor(0.0, 0.0))?,
        (16, 16),
        16,
    )?;
    let at_twice_the_size = render(Layer::picture(source).look(vignette)?, (8, 8), 16)?;
    assert_eq!(doubled.pixels(), at_twice_the_size.pixels());
    Ok(())
}

/// The area of a layer scales with the square of `scale` (and with `aspect` once): an opaque rectangle of `a` pixels
/// covers `a x s x s x aspect` pixels in all, summed over alpha. Axis-aligned edges are exact; a turned one is within 2%.
#[test]
fn the_area_of_a_layer_scales_with_the_square_of_its_scale() -> Outcome {
    let rectangle = solid(8, 6, [0.2, 0.4, 0.6, 1.0])?;
    let sum = |layer: Layer| -> Result<f64, Error> {
        let picture = render(layer, (64, 64), 64)?;
        Ok(pixels(&picture).map(|pixel| f64::from(pixel[3])).sum())
    };
    for (scale, aspect, turn, tolerance) in [
        (1.0, 1.0, 0.0, 1e-4),
        (0.5, 1.0, 0.0, 1e-4),
        (1.5, 1.0, 0.0, 1e-4),
        (3.0, 1.0, 0.0, 1e-4),
        (2.0, 0.12, 0.0, 1e-4),
        (2.5, 0.5, 90.0, 1e-4),
        (2.0, 1.0, 30.0, 0.02),
        (1.3, 0.7, 213.0, 0.02),
    ] {
        let place = Place::new()
            .scale(scale)
            .aspect(aspect)
            .rotate(Deg(turn))
            .dx(32.0)
            .dy(32.0);
        let got = sum(Layer::picture(rectangle.clone()).place(place)?)?;
        let expected = 48.0 * scale * scale * aspect;
        assert!(
            (got - expected).abs() <= tolerance * expected,
            "scale {scale}, aspect {aspect}, turn {turn}: {got} is not {expected}"
        );
    }
    Ok(())
}

/// A constant picture stays constant under any placement that keeps it on the canvas; alpha is 1 inside and 0 outside.
#[test]
fn a_constant_picture_stays_constant_when_it_is_placed() -> Outcome {
    let colour = [0.2, 0.4, 0.6, 1.0];
    let layer = Layer::picture(solid(10, 10, colour)?).place(
        Place::new()
            .scale(1.7)
            .rotate(Deg(37.0))
            .skew_x(Deg(12.0))
            .dx(30.0)
            .dy(30.0),
    )?;
    let drawn = render(layer, (60, 60), 60)?;
    let mut inside = 0;
    for got in pixels(&drawn) {
        if got[3] > 0.0 {
            assert!(
                close(
                    [got[0], got[1], got[2], 0.0],
                    [colour[0], colour[1], colour[2], 0.0],
                    1e-6
                ),
                "{got:?}"
            );
        }
        inside += usize::from(got[3] > 0.999);
    }
    assert!(inside > 150, "{inside}");
    Ok(())
}

/// A line of 1-pixel stripes squashed by `aspect` is the average of the stripes under each pixel: grey 0.5 in a whole
/// number of pixels (20 rows at 0.1 is 2 rows; 20 rows at 0.5 is 10).
#[test]
fn a_reduction_by_aspect_averages_the_pixels_it_covers() -> Outcome {
    let stripes = made(4, 20, |_, y| {
        let level = if y % 2 == 0 { 1.0 } else { 0.0 };
        [level, level, level, 1.0]
    })?;
    for (aspect, rows) in [(0.1, 2), (0.5, 10), (0.25, 5)] {
        let drawn = render(
            Layer::picture(stripes.clone()).place(Place::new().aspect(aspect).anchor(0.0, 0.0))?,
            (4, 20),
            20,
        )?;
        for y in 0..20 {
            let expected = if y < rows { [0.5, 0.5, 0.5, 1.0] } else { CLEAR };
            for x in 0..4 {
                assert!(
                    close(pixel(&drawn, x, y), expected, 1e-6),
                    "aspect {aspect}, ({x}, {y}): {:?}",
                    pixel(&drawn, x, y)
                );
            }
        }
    }
    Ok(())
}

/// Where the map shrinks the picture, whichever way, the pixel is the average of what its footprint covers. 1-pixel vertical
/// stripes (every other column white) are kept as they are at no skew (the footprint is under a pixel), and are grey under
/// a skew of 80, 86 or 89 degrees, where the sheared stripes are far finer than a pixel: the footprint is `s1` source pixels
/// long, a box average of `s1` pixels of a square wave of period 2 is within `1 / (2 s1)` of grey, and the taps along the
/// footprint (about one a pixel) add a little: the tolerance is that and 0.05.
#[test]
fn stripes_are_kept_when_the_footprint_is_a_pixel_and_averaged_to_grey_under_any_skew() -> Outcome {
    let stripes = made(256, 256, |x, _| {
        let level = if x % 2 == 0 { 1.0 } else { 0.0 };
        [level, level, level, 1.0]
    })?;
    // the interior pixels (alpha 1) around the middle, as (column, red)
    let interior = |place: Place| -> Result<Vec<(usize, f32)>, Error> {
        let drawn = render(Layer::picture(stripes.clone()).place(place)?, (256, 256), 256)?;
        let around = (127..130).flat_map(|y| (100..156).map(move |x| (x, y)));
        Ok(around
            .map(|(x, y)| (x, pixel(&drawn, x, y)))
            .filter(|(_, got)| got[3] > 0.999)
            .map(|(x, got)| (x, got[0]))
            .collect())
    };
    let kept = interior(Place::new())?;
    assert_eq!(kept.len(), 3 * 56);
    assert!(
        kept.iter()
            .all(|&(x, red)| (red - if x % 2 == 0 { 1.0 } else { 0.0 }).abs() < 1e-6),
        "not kept at no skew"
    );
    // halved: exactly grey where the map keeps the axes (the exact area average), within the tolerance where it does not
    for (skew, scale) in [(0.0_f64, 0.5_f64), (80.0, 0.5), (86.0, 0.5), (80.0, 0.25)] {
        let halved = interior(Place::new().scale(scale).skew_x(Deg(skew)))?;
        assert!(!halved.is_empty(), "skew {skew}: nothing in the middle");
        let t = skew.to_radians().tan();
        let tolerance = if skew == 0.0 {
            1e-5
        } else {
            0.5 / ((t + 1.0 / t) / scale) + 0.05
        };
        let largest = halved
            .iter()
            .map(|&(_, red)| f64::from((red - 0.5).abs()))
            .fold(0.0, f64::max);
        assert!(
            largest < tolerance,
            "skew {skew} at {scale}: a pixel is {largest} from grey, more than {tolerance}"
        );
    }
    for skew in [80.0_f64, 86.0, 89.0] {
        // the longest singular value of a skew by `t` is about t + 1 / t
        let t = skew.to_radians().tan();
        let tolerance = 0.5 / (t + 1.0 / t) + 0.05;
        let sheared = interior(Place::new().skew_x(Deg(skew)))?;
        assert!(sheared.len() >= 8, "skew {skew}: {} interior pixels", sheared.len());
        let largest = sheared
            .iter()
            .map(|&(_, red)| f64::from((red - 0.5).abs()))
            .fold(0.0, f64::max);
        assert!(
            largest < tolerance,
            "skew {skew}: a pixel is {largest} from grey, more than {tolerance}"
        );
    }
    Ok(())
}

/// The rows and columns of `raster` where alpha is at least `at_least`, as `(first column, last column, first row, last
/// row)`; `None` for none.
fn extent(raster: &Raster, at_least: f32) -> Option<(usize, usize, usize, usize)> {
    let mut found: Option<(usize, usize, usize, usize)> = None;
    for y in 0..raster.height() {
        for x in 0..raster.width() {
            if pixel(raster, x, y)[3] >= at_least {
                found = Some(match found {
                    None => (x, x, y, y),
                    Some((x0, x1, y0, y1)) => (x0.min(x), x1.max(x), y0.min(y), y1.max(y)),
                });
            }
        }
    }
    found
}

/// A turn by an angle that is not a quarter puts a point where the rotation matrix says (clockwise, y down: (x, y) goes to
/// (x cos - y sin, x sin + y cos) about the anchor). A 20 by 10 rectangle turned 30 degrees about its top left corner and
/// moved by (20, 10): the box point (15, 3) goes to (11.49, 10.10) + (20, 10), the point (2, 8) to (-2.27, 7.93) + (20, 10),
/// and a point 2 past the right edge, (22, 5), to (16.55, 15.33) + (20, 10), outside; the sign of the angle reversed would put
/// the first at (34.49, 5.10), where nothing is.
#[test]
fn a_turn_by_another_angle_puts_points_where_the_rotation_matrix_says() -> Outcome {
    let drawn = render(
        Layer::picture(solid(20, 10, [1.0; 4])?)
            .place(Place::new().rotate(Deg(30.0)).anchor(0.0, 0.0).dx(20.0).dy(10.0))?,
        (60, 40),
        60,
    )?;
    let alpha = |x: usize, y: usize| pixel(&drawn, x, y)[3];
    assert!(alpha(31, 20) > 0.999, "inside (15, 3): {}", alpha(31, 20));
    assert!(alpha(17, 17) > 0.999, "inside (2, 8): {}", alpha(17, 17));
    assert!(alpha(36, 25) < 0.001, "outside (22, 5): {}", alpha(36, 25));
    assert!(
        alpha(34, 5) < 0.001,
        "where a turn the other way would put (15, 3): {}",
        alpha(34, 5)
    );
    Ok(())
}

/// Scale and aspect, then the turn: a 8 by 4 rectangle squashed to half its height is 8 by 2, and a quarter turn makes it 2
/// wide and 8 tall about its middle; had the aspect been applied to x (or after the turn) it would be 4 wide and 4 tall, or
/// 4 wide and 8 tall.
#[test]
fn the_aspect_squashes_y_before_the_turn() -> Outcome {
    let drawn = render(
        Layer::picture(solid(8, 4, [1.0; 4])?).place(Place::new().aspect(0.5).rotate(Deg(90.0)).dx(8.0).dy(8.0))?,
        (24, 24),
        24,
    )?;
    // the middle of the box is (4, 2) + (8, 8) = (12, 10): 2 wide from 11 to 12, 8 tall from 6 to 13
    assert_eq!(extent(&drawn, 1.0), Some((11, 12, 6, 13)));
    assert_eq!(extent(&drawn, f32::MIN_POSITIVE), Some((11, 12, 6, 13)));
    Ok(())
}

/// Scale about the anchor: a 4 by 4 picture moved to (6, 6) and doubled keeps the point of its box that the anchor names
/// where it was, so its first column is 6 - 4 x the anchor's x, and it is 8 wide.
#[test]
fn a_layer_scales_about_its_anchor() -> Outcome {
    let source = solid(4, 4, [1.0; 4])?;
    // (anchor), then the first column, last column, first row and last row that are covered
    let cases = [
        ((0.5, 0.5), (4, 11, 4, 11)),
        ((0.0, 0.0), (6, 13, 6, 13)),
        ((1.0, 1.0), (2, 9, 2, 9)),
        ((0.0, 1.0), (6, 13, 2, 9)),
    ];
    for ((x, y), expected) in cases {
        let layer = Layer::picture(source.clone()).place(Place::new().scale(2.0).anchor(x, y).dx(6.0).dy(6.0))?;
        let drawn = render(layer, (24, 24), 24)?;
        assert_eq!(extent(&drawn, f32::MIN_POSITIVE), Some(expected), "anchor ({x}, {y})");
        assert_eq!(extent(&drawn, 1.0), Some(expected), "anchor ({x}, {y})");
    }
    Ok(())
}

/// Skew then turn (the stated order) and the other way round put a 20 by 10 rectangle at different places: about the
/// top left corner, skewed along x by 45 degrees (a point moves along x by its y) and then turned a quarter clockwise,
/// the corner (x, y) is at (-y, x + y), and moved by (40, 10) the rectangle is the parallelogram (40, 10), (40, 30), (30,
/// 40), (30, 20). Turning first would give (30, 10), (40, 10), (60, 30), (50, 30).
#[test]
fn the_skew_is_applied_before_the_turn() -> Outcome {
    let drawn = render(
        Layer::picture(solid(20, 10, [1.0; 4])?).place(
            Place::new()
                .skew_x(Deg(45.0))
                .rotate(Deg(90.0))
                .anchor(0.0, 0.0)
                .dx(40.0)
                .dy(10.0),
        )?,
        (80, 80),
        80,
    )?;
    let alpha = |x: usize, y: usize| pixel(&drawn, x, y)[3];
    // inside the stated parallelogram (centre 35.5, 25.5) and outside the other
    assert!(alpha(35, 25) > 0.999, "{}", alpha(35, 25));
    // inside the other one (45.5, 20.5) and outside the stated
    assert!(alpha(45, 20) < 0.001, "{}", alpha(45, 20));
    // outside both, to the left and below
    assert!(alpha(25, 25) < 0.001 && alpha(35, 45) < 0.001);
    Ok(())
}

/// `skew_x` then `skew_y`: a 10 by 10 square about its corner, skewed by 45 degrees along x and then by 45 along y, has its
/// far corner (10, 10) at (10 + 10, 10 + 20) = (20, 30); skewing y first would put it at (30, 20).
#[test]
fn the_skew_along_x_is_applied_before_the_skew_along_y() -> Outcome {
    let drawn = render(
        Layer::picture(solid(10, 10, [1.0; 4])?).place(
            Place::new()
                .skew_x(Deg(45.0))
                .skew_y(Deg(45.0))
                .anchor(0.0, 0.0)
                .dx(2.0)
                .dy(2.0),
        )?,
        (40, 40),
        40,
    )?;
    let alpha = |x: usize, y: usize| pixel(&drawn, x, y)[3];
    // (16.5, 24.5) minus the move (2, 2) is (14.5, 22.5): inside the stated parallelogram (0, 0), (10, 10), (20, 30), (10, 20)
    assert!(alpha(16, 24) > 0.999, "{}", alpha(16, 24));
    // inside the other order's (0, 0), (20, 10), (30, 20), (10, 10) at (15.5, 10.5) + (2, 2), outside the stated
    assert!(alpha(17, 12) < 0.001, "{}", alpha(17, 12));
    Ok(())
}

/// A placement applies to the fitted box: `fit` cover of a 16 by 16 picture into the square (4, 4, 8, 8) halves it by area,
/// and a quarter turn about that square's middle (8, 8) is a permutation of the 8 by 8 result: its pixel (i, j) goes to
/// (15 - j, i).
#[test]
fn place_acts_on_the_fitted_box() -> Outcome {
    let source = made(16, 16, |x, y| {
        [
            f32::from(u8::try_from(x).unwrap_or(0)) / 16.0,
            f32::from(u8::try_from(y).unwrap_or(0)) / 16.0,
            0.5,
            1.0,
        ]
    })?;
    let fitted = Layer::picture(source).fit([4.0, 4.0, 8.0, 8.0], Fit::Cover, [0.5, 0.5])?;
    let unturned = render(fitted.clone(), (16, 16), 16)?;
    let turned = render(fitted.place(Place::new().rotate(Deg(90.0)))?, (16, 16), 16)?;
    assert_eq!(extent(&unturned, f32::MIN_POSITIVE), Some((4, 11, 4, 11)));
    assert_eq!(permuted(&turned, &unturned, |i, j| (15 - j, i)), None);
    Ok(())
}

/// A look on a picture that `fit` cuts sees the cut picture, so a vignette is centred on the box: the look of the cut is the
/// look of a picture that was cut by hand.
#[test]
fn a_look_on_a_cover_cut_picture_sees_the_cut_picture() -> Outcome {
    let vignette = r#"{"steps": [{"effect": "vignette", "opacity": 1.0, "blend_mode": "normal"}]}"#;
    let wide = varied(16, 8)?;
    for (align, first_column) in [(0.0, 0_usize), (1.0, 8)] {
        let cut = made(8, 8, |x, y| pixel(&wide, first_column + x, y))?;
        let by_hand = render(Layer::picture(cut).look(vignette)?, (8, 8), 8)?;
        let by_fit = render(
            Layer::picture(wide.clone())
                .look(vignette)?
                .fit([0.0, 0.0, 8.0, 8.0], Fit::Cover, [align, 0.5])?,
            (8, 8),
            8,
        )?;
        assert_eq!(by_fit.pixels(), by_hand.pixels(), "align {align}");
    }
    Ok(())
}

/// A band of 8 horizontal stripes 100 rows tall on a photo 360 wide: the delivery crop of a taller photo into a 9:16 frame.
#[test]
fn the_delivery_crop_of_a_tall_photo_into_720_by_1280_keeps_the_part_align_says() -> Outcome {
    let bands = made(360, 800, |_, y| {
        let level = f32::from(u8::try_from(y / 100).unwrap_or(0) + 1) / 9.0;
        [level, level, level, 1.0]
    })?;
    // cover: the scale is 2 (720 / 360), the photo 720 by 1600 and 320 rows overflow; align 0 keeps the top, 1 the bottom
    for (align, cut) in [(0.0, 0.0), (0.5, 160.0), (1.0, 320.0)] {
        let layer = Layer::picture(bands.clone()).fit([0.0, 0.0, 720.0, 1280.0], Fit::Cover, [0.5, align])?;
        let drawn = render(layer, (720, 1280), 1280)?;
        assert_eq!((drawn.width(), drawn.height()), (720, 1280));
        for y in 0..1280_usize {
            // output row y is row (y + cut) of the 1600 rows, whose bands are 200 rows tall
            #[expect(clippy::cast_precision_loss, reason = "a row")]
            let at = y as f64 + 0.5 + cut;
            let nearest_edge = (at % 200.0).min(200.0 - at % 200.0);
            if nearest_edge < 1.6 {
                continue;
            }
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a band number")]
            let band = (at / 200.0) as u32;
            let expected = f32::from(u8::try_from(band + 1).unwrap_or(0)) / 9.0;
            for x in [0_usize, 359, 719] {
                assert!(
                    close(pixel(&drawn, x, y), [expected, expected, expected, 1.0], 1e-5),
                    "align {align}, row {y}: {:?}",
                    pixel(&drawn, x, y)
                );
            }
        }
    }
    Ok(())
}

/// A wide photo into the same frame: 8 vertical stripes 100 columns wide on a photo 800 by 360. The scale is 1280 / 360, the
/// photo 2844.4 by 1280.
#[test]
fn a_wide_photo_cut_into_a_tall_frame_keeps_the_part_align_says() -> Outcome {
    let bands = made(800, 360, |x, _| {
        let level = f32::from(u8::try_from(x / 100).unwrap_or(0) + 1) / 9.0;
        [level, level, level, 1.0]
    })?;
    let step = 1280.0 / 360.0 * 100.0;
    let overflow = 800.0 * 1280.0 / 360.0 - 720.0;
    for align in [0.0, 0.5, 1.0] {
        let layer = Layer::picture(bands.clone()).fit([0.0, 0.0, 720.0, 1280.0], Fit::Cover, [align, 0.5])?;
        let drawn = render(layer, (720, 1280), 1280)?;
        for x in 0..720_usize {
            #[expect(clippy::cast_precision_loss, reason = "a column")]
            let at = x as f64 + 0.5 + overflow * align;
            let nearest_edge = (at % step).min(step - at % step);
            if nearest_edge < 2.5 {
                continue;
            }
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a band number")]
            let band = (at / step) as u32;
            let expected = f32::from(u8::try_from(band + 1).unwrap_or(0)) / 9.0;
            for y in [0_usize, 640, 1279] {
                assert!(
                    close(pixel(&drawn, x, y), [expected, expected, expected, 1.0], 1e-5),
                    "align {align}, column {x}: {:?}",
                    pixel(&drawn, x, y)
                );
            }
        }
    }
    Ok(())
}

/// A copy, by hand: where the scale is 1 along the binding axis the fitted picture is the photo's own pixels, a window of
/// them (cover) or the whole at an offset (contain), so each pixel is compared to the photo's, bit for bit.
#[test]
fn cover_and_contain_at_scale_one_cut_or_place_the_photos_own_pixels() -> Outcome {
    // cover: a 6 by 10 photo into the box (1, 1, 6, 4): the scale is 1, and 6 of the 10 rows overflow
    let tall = Layer::picture(numbered(6, 10)?);
    let unplaced = render(tall.clone(), (8, 12), 12)?;
    for (align, first_row) in [(0.0, 0_usize), (0.5, 3), (1.0, 6)] {
        let fitted = render(
            tall.clone().fit([1.0, 1.0, 6.0, 4.0], Fit::Cover, [0.3, align])?,
            (8, 12),
            12,
        )?;
        // photo row `first_row + j` is at row `1 + j`, for j in 0..4, and nothing else is drawn
        for j in 0..4_usize {
            for i in 0..6_usize {
                assert!(
                    exactly(pixel(&fitted, 1 + i, 1 + j), pixel(&unplaced, i, first_row + j)),
                    "align {align}, ({i}, {j})"
                );
            }
        }
        assert_eq!(extent(&fitted, f32::MIN_POSITIVE), Some((1, 6, 1, 4)), "align {align}");
    }
    // contain: a 4 by 6 photo into the box (0, 0, 10, 6): the scale is 1 and 6 columns of slack; align says where
    let narrow = Layer::picture(numbered(4, 6)?);
    let unplaced = render(narrow.clone(), (10, 6), 10)?;
    for (align, offset) in [(0.0, 0_i64), (0.5, 3), (1.0, 6)] {
        let fitted = render(
            narrow.clone().fit([0.0, 0.0, 10.0, 6.0], Fit::Contain, [align, 0.0])?,
            (10, 6),
            10,
        )?;
        assert_eq!(
            permuted(&fitted, &unplaced, |i, j| (i + offset, j)),
            None,
            "align {align}"
        );
    }
    // contain into a larger box with slack in y: an 8 by 4 photo into (2, 2, 8, 8) sits at y = 2 + 4 * align
    let wide = Layer::picture(numbered(8, 4)?);
    let unplaced = render(wide.clone(), (12, 12), 12)?;
    for (align, offset) in [(0.0, 2_i64), (0.5, 4), (1.0, 6)] {
        let fitted = render(
            wide.clone().fit([2.0, 2.0, 8.0, 8.0], Fit::Contain, [0.0, align])?,
            (12, 12),
            12,
        )?;
        assert_eq!(
            permuted(&fitted, &unplaced, |i, j| (i + 2, j + offset)),
            None,
            "align {align}"
        );
    }
    Ok(())
}

/// Stretch gives a different scale along each axis, and fills the box exactly: a 8 by 4 photo into a 4 by 4 box is halved
/// along x (the mean of two neighbours, an area average) and kept along y.
#[test]
fn stretch_fills_the_box_with_a_scale_for_each_axis() -> Outcome {
    let photo = numbered(8, 4)?;
    let unplaced = render(Layer::picture(photo.clone()), (8, 4), 8)?;
    let stretched = render(
        Layer::picture(photo).fit([1.0, 1.0, 4.0, 4.0], Fit::Stretch, [0.5, 0.5])?,
        (8, 6),
        8,
    )?;
    for y in 0..6_usize {
        for x in 0..8_usize {
            let inside = (1..5).contains(&x) && (1..5).contains(&y);
            if !inside {
                assert!(exactly(pixel(&stretched, x, y), CLEAR), "({x}, {y}) outside the box");
                continue;
            }
            let (left, right) = (
                pixel(&unplaced, 2 * (x - 1), y - 1),
                pixel(&unplaced, 2 * (x - 1) + 1, y - 1),
            );
            // premultiplied mean of two pixels
            let mut expected = [0.0_f32; 4];
            for pixel in [left, right] {
                for channel in 0..3 {
                    expected[channel] += pixel[channel] * pixel[3] / 2.0;
                }
                expected[3] += pixel[3] / 2.0;
            }
            for channel in 0..3 {
                expected[channel] /= expected[3];
            }
            assert!(
                close(pixel(&stretched, x, y), expected, 1e-5),
                "({x}, {y}): {:?} is not {expected:?}",
                pixel(&stretched, x, y)
            );
        }
    }
    Ok(())
}

/// A box with an edge between two output pixels covers the pixel it ends in in part: a fill of box x from 2.5 to 6.5 is
/// half of column 2, whole 3 to 5 and half of column 6. A fill has no picture of its own, so its size is the canvas's, and
/// `fit` makes it the band.
#[test]
fn a_box_that_ends_between_two_pixels_covers_the_pixel_in_part() -> Outcome {
    let band = Layer::fill("#ff0000")?.fit([2.5, 1.0, 4.0, 2.0], Fit::Stretch, [0.5, 0.5])?;
    let drawn = render(band, (10, 4), 10)?;
    let red = |alpha: f32| [1.0, 0.0, 0.0, alpha];
    for y in 0..4_usize {
        for x in 0..10_usize {
            let alpha = match x {
                2 | 6 => 0.5,
                3..=5 => 1.0,
                _ => 0.0,
            };
            let expected = if !(1..3).contains(&y) || alpha == 0.0 {
                CLEAR
            } else {
                red(alpha)
            };
            assert!(
                close(pixel(&drawn, x, y), expected, 1e-6),
                "({x}, {y}): {:?}",
                pixel(&drawn, x, y)
            );
        }
    }
    Ok(())
}

/// A flat fill is a constant picture: placed by a move, a scale or a flip it gives what a 2 by 2 picture of the same colour
/// gives (the fill is one pixel and takes no taps, the picture takes them), and what the same placement with a turn too small
/// to see gives (the general kernel, which the others skip). Edge pixels are covered in part, so alpha differs from 1 there.
#[test]
fn a_flat_fill_placed_gives_what_a_constant_picture_and_the_general_kernel_give() -> Outcome {
    let colour = [0x33, 0x66, 0x99, 0x80].map(|byte: u8| f32::from(byte) / 255.0);
    let places = [
        ("a move by a fraction", Place::new().dx(2.25).dy(-1.5)),
        (
            "an enlargement about a corner",
            Place::new().scale(2.5).anchor(0.0, 0.0).dx(1.3),
        ),
        (
            "a flip and a reduction",
            Place::new().flip_x(true).scale(0.4).dx(7.0).dy(6.0),
        ),
        ("a squash", Place::new().aspect(0.3).dy(5.5)),
    ];
    for (what, place) in places {
        let fill = Layer::fill("#33669980")?.fit([1.0, 2.0, 9.0, 6.0], Fit::Stretch, [0.5, 0.5])?;
        let picture = Layer::picture(solid(2, 2, colour)?).fit([1.0, 2.0, 9.0, 6.0], Fit::Stretch, [0.5, 0.5])?;
        let flat = render(fill.clone().place(place)?, (16, 12), 16)?;
        let taps = render(picture.place(place)?, (16, 12), 16)?;
        let turned = render(fill.place(place.rotate(Deg(1e-9)))?, (16, 12), 16)?;
        for (x, y) in (0..12).flat_map(|y| (0..16).map(move |x| (x, y))) {
            let (flat, taps, turned) = (pixel(&flat, x, y), pixel(&taps, x, y), pixel(&turned, x, y));
            assert!(
                close(flat, taps, 1e-6),
                "{what}, ({x}, {y}): {flat:?} against the picture's {taps:?}"
            );
            // a turn too small to see leaks an alpha of 1e-12 past an edge, with a colour: compare what the blend sees
            let seen = |pixel: [f32; 4]| [pixel[0] * pixel[3], pixel[1] * pixel[3], pixel[2] * pixel[3], pixel[3]];
            assert!(
                close(seen(flat), seen(turned), 1e-5),
                "{what}, ({x}, {y}): {flat:?} against the general kernel's {turned:?}"
            );
        }
    }
    Ok(())
}

/// A fit factor of 0.1 times a zoom of 3 is 0.30000000000000004 in a float: a 100 pixel picture is still 30 pixels, not 31.
/// A look runs at the drawn size, so a no-op look on the layer must leave it what the layer without the look is, to within
/// the look's own rounding; 31 pixels resampled to 30 would blur the picture.
#[test]
fn a_fit_factor_times_a_zoom_that_is_a_whole_number_of_pixels_draws_no_pixel_more() -> Outcome {
    let noop = r#"{"steps": [{"effect": "grade"}]}"#;
    let photo = varied(100, 100)?;
    let layer = |looked: bool| -> Result<Layer, Error> {
        let picture = Layer::picture(photo.clone());
        let picture = if looked { picture.look(noop)? } else { picture };
        picture
            .fit([0.0, 0.0, 10.0, 10.0], Fit::Stretch, [0.5, 0.5])?
            .place(Place::new().scale(3.0).anchor(0.0, 0.0))
    };
    let (plain, looked) = (
        render(layer(false)?, (30, 30), 30)?,
        render(layer(true)?, (30, 30), 30)?,
    );
    for (x, y) in (0..30).flat_map(|y| (0..30).map(move |x| (x, y))) {
        assert!(close(pixel(&looked, x, y), pixel(&plain, x, y), 1e-5), "({x}, {y})");
    }
    Ok(())
}

/// The drawn-size limit is the size of the part of the box that is shown: a 1000 by 1000 picture cover-fitted into a box 10
/// wide and 1000 tall shows 10 by 1000 of it, 400 by 40,000 pixels (16 million) at a scale of 40; its whole box would be
/// 1.6 billion.
#[test]
fn the_drawn_size_limit_is_that_of_the_part_a_cover_fit_shows() -> Outcome {
    let layer = Layer::picture(solid(1000, 1000, [0.5; 4])?)
        .fit([0.0, 0.0, 10.0, 1000.0], Fit::Cover, [0.5, 0.5])?
        .place(Place::new().scale(40.0))?;
    let drawn = render(layer, (10, 1000), 1000)?;
    assert_eq!((drawn.width(), drawn.height()), (10, 1000));
    // the shown part is the middle of the picture, 10 design pixels wide at 40 times: the canvas is a part of that
    assert!(close(pixel(&drawn, 5, 500), [0.5; 4], 1e-6));
    Ok(())
}

/// A picture cut by cover and reduced at once: a 300 by 200 picture of three vertical bands 100 wide (and a 200 by 300 one of
/// three horizontal bands) cover-fitted into a box 20 by 40 (40 by 20), a reduction by 5, shows the band align names, whole.
#[test]
fn a_cover_cut_that_is_also_a_reduction_keeps_the_part_align_names() -> Outcome {
    let level = |band: usize| f32::from(u8::try_from(band + 1).unwrap_or(0)) / 4.0;
    let wide = made(300, 200, |x, _| [level(x / 100), 0.0, 0.0, 1.0])?;
    let tall = made(200, 300, |_, y| [level(y / 100), 0.0, 0.0, 1.0])?;
    for (align, band) in [(0.0, 0), (0.5, 1), (1.0, 2)] {
        let across = Layer::picture(wide.clone()).fit([0.0, 0.0, 20.0, 40.0], Fit::Cover, [align, 0.5])?;
        let down = Layer::picture(tall.clone()).fit([0.0, 0.0, 40.0, 20.0], Fit::Cover, [0.5, align])?;
        for (what, layer, canvas) in [("across", across, (20, 40)), ("down", down, (40, 20))] {
            let drawn = render(layer, canvas, 40)?;
            assert!(
                pixels(&drawn).all(|got| close(got, [level(band), 0.0, 0.0, 1.0], 1e-5)),
                "{what}, align {align}: band {band} is not all that is shown"
            );
        }
    }
    Ok(())
}

proptest! {
    /// A layer that covers the canvas covers it whole at any size, a size that leaves the short side between two pixels
    /// included (the output is stretched onto whole pixels).
    #[test]
    fn a_full_bleed_layer_covers_the_whole_canvas_at_any_size(width in 1_u32..13, height in 1_u32..13, size in 1_u32..40) {
        let picture = Layer::picture(solid(usize::try_from(width)?, usize::try_from(height)?, [0.3, 0.6, 0.9, 1.0])?);
        let fitted = Layer::picture(varied(5, 7)?).fit([0.0, 0.0, f64::from(width), f64::from(height)], Fit::Stretch, [0.5, 0.5])?;
        for layer in [picture, fitted, Layer::fill("#336699")?] {
            let drawn = render(layer, (width, height), size)?;
            for got in pixels(&drawn) {
                prop_assert!(got[3] >= 1.0 - 1e-6, "alpha {}", got[3]);
            }
        }
    }

    /// Placed layers are drawn the same twice, and what comes out is a picture: every value is from 0 to 1 and where alpha is
    /// 0 the colour is 0. A layer far larger or farther away than the canvas is cut at it.
    #[test]
    fn any_placement_gives_a_picture_in_range_and_the_same_one_twice(
        scale in 0.001_f64..1000.0, aspect in 0.001_f64..1000.0, turn in -720.0_f64..720.0,
        skew_x in -89.0_f64..89.0, skew_y in -89.0_f64..89.0, dx in -50.0_f64..50.0, dy in -50.0_f64..50.0,
        anchor_x in 0.0_f64..=1.0, anchor_y in 0.0_f64..=1.0, flip_x: bool, flip_y: bool,
    ) {
        let place = Place::new().scale(scale).aspect(aspect).rotate(Deg(turn)).skew_x(Deg(skew_x)).skew_y(Deg(skew_y))
            .dx(dx).dy(dy).anchor(anchor_x, anchor_y).flip_x(flip_x).flip_y(flip_y);
        let layer = Layer::picture(numbered(6, 4)?).place(place)?;
        let first = render(layer.clone(), (12, 10), 12)?;
        let second = render(layer, (12, 10), 12)?;
        prop_assert_eq!(first.pixels(), second.pixels());
        for got in pixels(&first) {
            prop_assert!(got.iter().all(|value| (0.0..=1.0).contains(value)), "{got:?}");
            prop_assert!(got[3] > 0.0 || got[..3] == [0.0, 0.0, 0.0], "{got:?}");
        }
    }
}

/// Rendering a placed layer at half the size is the box average of the render at the full size, for a smooth picture, away
/// from the edge (where an edge pixel's coverage differs by a fraction): a bilinear sample of a smooth picture at two
/// resolutions differs by about its second difference, here under 0.02.
#[test]
fn a_placed_layer_at_half_the_size_is_the_box_average_of_the_whole_size() -> Outcome {
    let smooth = made(20, 20, |x, y| {
        #[expect(clippy::cast_precision_loss, reason = "a small column")]
        let (x, y) = (x as f32, y as f32);
        [0.5 + 0.4 * (x / 6.0).sin(), 0.5 + 0.4 * (y / 8.0).cos(), 0.3, 1.0]
    })?;
    let place = Place::new()
        .rotate(Deg(23.0))
        .skew_x(Deg(10.0))
        .scale(1.4)
        .dx(30.0)
        .dy(26.0);
    let layer = Layer::picture(smooth).place(place)?;
    let full = render(layer.clone(), (64, 64), 64)?;
    let half = render(layer, (64, 64), 32)?;
    let mut compared = 0;
    for y in 0..32 {
        for x in 0..32 {
            let near = [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dy)| pixel(&full, 2 * x + dx, 2 * y + dy));
            if near.iter().any(|pixel| pixel[3] < 0.999) || pixel(&half, x, y)[3] < 0.999 {
                continue;
            }
            compared += 1;
            let mut mean = [0.0_f32; 4];
            for pixel in near {
                mean.iter_mut().zip(pixel).for_each(|(sum, value)| *sum += value / 4.0);
            }
            assert!(
                close(pixel(&half, x, y), mean, 0.02),
                "({x}, {y}): {:?} is not {mean:?}",
                pixel(&half, x, y)
            );
        }
    }
    assert!(compared > 100, "{compared}");
    Ok(())
}

/// A layer many times the canvas, turned, is cut at the canvas: nothing wraps or panics, a pixel inside the turned square is
/// opaque and one outside is clear. A 40 by 40 picture about its middle (20, 20) turned 45 degrees is the diamond
/// |x - 20| + |y - 20| <= 28.28; the canvas is the 10 by 10 corner.
#[test]
fn a_turned_layer_larger_than_the_canvas_is_cut_at_the_canvas() -> Outcome {
    let drawn = render(
        Layer::picture(solid(40, 40, [0.5, 0.5, 0.5, 1.0])?).place(Place::new().rotate(Deg(45.0)))?,
        (10, 10),
        10,
    )?;
    assert_eq!((drawn.width(), drawn.height()), (10, 10));
    // (9.5, 9.5) is 21 from the diamond's middle in |x| + |y|: inside; (0.5, 0.5) is 39: outside
    assert!(close(pixel(&drawn, 9, 9), [0.5, 0.5, 0.5, 1.0], 1e-6));
    assert!(exactly(pixel(&drawn, 0, 0), CLEAR));
    Ok(())
}

/// The layer given is not changed: `fit` and `place` return new layers.
#[test]
fn fit_and_place_return_new_layers_and_leave_the_old_one_as_it_was() -> Outcome {
    let layer = Layer::picture(numbered(4, 2)?);
    let before = render(layer.clone(), (8, 8), 8)?;
    let _moved = layer.clone().place(Place::new().dx(2.0))?;
    let _fitted = layer.clone().fit([0.0, 0.0, 3.0, 3.0], Fit::Contain, [0.5, 0.5])?;
    assert_eq!(render(layer, (8, 8), 8)?.pixels(), before.pixels());
    Ok(())
}

/// `place` sets the fields it names and keeps the others: three calls are one call with all three, and a later call overrides.
#[test]
fn place_keeps_the_fields_it_does_not_name() -> Outcome {
    let picture = Layer::picture(numbered(4, 2)?);
    let together = render(
        picture.clone().place(Place::new().dx(3.0).rotate(Deg(90.0)).dy(2.0))?,
        (16, 16),
        16,
    )?;
    let apart = render(
        picture
            .clone()
            .place(Place::new().dx(3.0))?
            .place(Place::new().rotate(Deg(90.0)))?
            .place(Place::new().dy(2.0))?,
        (16, 16),
        16,
    )?;
    assert_eq!(together.pixels(), apart.pixels());
    let overridden = render(
        picture
            .place(Place::new().dx(9.0))?
            .place(Place::new().dx(3.0).rotate(Deg(90.0)).dy(2.0))?,
        (16, 16),
        16,
    )?;
    assert_eq!(together.pixels(), overridden.pixels());
    Ok(())
}

/// Whether `result` was refused, naming `field` (and, for a bad row, the value).
fn refusal(result: Result<Layer, Error>) -> Option<String> {
    match result {
        Err(Error::Invalid { address, message }) => Some(format!("{address}: {message}")),
        Err(other) => Some(format!("not an Invalid: {other}")),
        Ok(_) => None,
    }
}

#[test]
fn a_fit_with_a_size_not_above_zero_or_an_align_outside_zero_to_one_is_refused() -> Outcome {
    let layer = || Layer::fill("#ffffff");
    // (what, box, align, the field the refusal names, or None for a good row)
    let rows: Vec<FitRow> = vec![
        ("a box", [0.0, 0.0, 10.0, 10.0], [0.5, 0.5], None),
        ("a box away from the origin", [-5.0, 1e9, 0.5, 0.001], [0.0, 1.0], None),
        ("a width of 0", [0.0, 0.0, 0.0, 10.0], [0.5, 0.5], Some("box")),
        ("a height of 0", [0.0, 0.0, 10.0, 0.0], [0.5, 0.5], Some("box")),
        ("a negative width", [0.0, 0.0, -1.0, 10.0], [0.5, 0.5], Some("box")),
        ("a NaN x", [f64::NAN, 0.0, 10.0, 10.0], [0.5, 0.5], Some("box")),
        (
            "an infinite y",
            [0.0, f64::INFINITY, 10.0, 10.0],
            [0.5, 0.5],
            Some("box"),
        ),
        (
            "an infinite width",
            [0.0, 0.0, f64::INFINITY, 10.0],
            [0.5, 0.5],
            Some("box"),
        ),
        ("an align below 0", [0.0, 0.0, 10.0, 10.0], [-0.01, 0.5], Some("align")),
        ("an align above 1", [0.0, 0.0, 10.0, 10.0], [0.5, 1.01], Some("align")),
        ("a NaN align", [0.0, 0.0, 10.0, 10.0], [f64::NAN, 0.5], Some("align")),
    ];
    for (what, area, align, field) in rows {
        for how in [Fit::Cover, Fit::Contain, Fit::Stretch] {
            let refused = refusal(layer()?.fit(area, how, align));
            match field {
                None => assert_eq!(refused, None, "{what} with {how:?}"),
                Some(field) => assert!(
                    refused.as_deref().is_some_and(|message| message.starts_with(field)),
                    "{what} with {how:?}: {refused:?}"
                ),
            }
        }
    }
    Ok(())
}

#[test]
fn a_fit_word_is_cover_contain_or_stretch() {
    for (word, how) in [
        ("cover", Fit::Cover),
        ("contain", Fit::Contain),
        ("stretch", Fit::Stretch),
    ] {
        assert_eq!(word.parse::<Fit>().ok(), Some(how));
    }
    for word in ["Cover", "fill", "", "cover ", "none"] {
        let refused = word.parse::<Fit>();
        assert!(
            matches!(&refused, Err(Error::Invalid { address, message }) if address == "how" && message.contains("cover")),
            "{word:?}: {refused:?}"
        );
    }
}

#[test]
fn a_place_number_is_refused_outside_its_range_naming_the_field() -> Outcome {
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    // (field, a place with the value, whether it is good): at each limit, one past it, and not finite
    let rows: Vec<(&str, Place, bool)> = vec![
        ("scale", Place::new().scale(0.001), true),
        ("scale", Place::new().scale(1000.0), true),
        ("scale", Place::new().scale(0.000_999), false),
        ("scale", Place::new().scale(1000.001), false),
        ("scale", Place::new().scale(0.0), false),
        ("scale", Place::new().scale(-1.0), false),
        ("scale", Place::new().scale(nan), false),
        ("scale", Place::new().scale(inf), false),
        ("aspect", Place::new().aspect(0.001), true),
        ("aspect", Place::new().aspect(1000.0), true),
        ("aspect", Place::new().aspect(0.0), false),
        ("aspect", Place::new().aspect(1001.0), false),
        ("aspect", Place::new().aspect(nan), false),
        ("skew_x", Place::new().skew_x(Deg(89.0)), true),
        ("skew_x", Place::new().skew_x(Deg(-89.0)), true),
        ("skew_x", Place::new().skew_x(Deg(89.01)), false),
        ("skew_x", Place::new().skew_x(Deg(-90.0)), false),
        ("skew_x", Place::new().skew_x(Deg(nan)), false),
        ("skew_y", Place::new().skew_y(Deg(88.9)), true),
        ("skew_y", Place::new().skew_y(Deg(90.0)), false),
        ("skew_y", Place::new().skew_y(Deg(-inf)), false),
        ("dx", Place::new().dx(1e6), true),
        ("dx", Place::new().dx(-1e6), true),
        ("dx", Place::new().dx(1.000_001e6), false),
        ("dx", Place::new().dx(nan), false),
        ("dy", Place::new().dy(-1e6), true),
        ("dy", Place::new().dy(-1.1e6), false),
        ("dy", Place::new().dy(inf), false),
        ("rotate", Place::new().rotate(Deg(1e300)), true),
        ("rotate", Place::new().rotate(Deg(-1e300)), true),
        ("rotate", Place::new().rotate(Deg(nan)), false),
        ("rotate", Place::new().rotate(Deg(inf)), false),
        ("anchor", Place::new().anchor(0.0, 1.0), true),
        ("anchor", Place::new().anchor(-0.1, 0.5), false),
        ("anchor", Place::new().anchor(0.5, 1.1), false),
        ("anchor", Place::new().anchor(nan, 0.5), false),
        ("flip", Place::new().flip_x(true).flip_y(true), true),
    ];
    for (field, place, good) in rows {
        let refused = refusal(Layer::fill("#ffffff")?.place(place));
        if good {
            assert_eq!(refused, None, "{field}: {place:?}");
        } else {
            assert!(
                refused.as_deref().is_some_and(|message| message.starts_with(field)),
                "{field}: {place:?}: {refused:?}"
            );
        }
    }
    // a refused call changes nothing: the layer it was made from is still good
    let layer = Layer::fill("#ffffff")?;
    assert!(layer.clone().place(Place::new().scale(-1.0)).is_err());
    assert!(render(layer, (4, 4), 4).is_ok());
    Ok(())
}

/// A layer too large for a count of pixels to be held says so: it is "more than" the limit, not the saturated count of a
/// machine word.
#[test]
fn a_layer_of_absurd_size_is_refused_as_more_than_the_limit_not_as_a_saturated_count() -> Outcome {
    for (area, wanted) in [
        ([0.0, 0.0, 1e300, 1.0], "more than 134217728 x 1 pixels"),
        ([0.0, 0.0, 1.0, 1e300], "x more than 134217728 pixels"),
    ] {
        let layer = Layer::fill("#ffffff")?
            .named("band")?
            .fit(area, Fit::Stretch, [0.5, 0.5])?;
        let refused = scene(10, 10, [layer])?.render(10);
        assert!(
            matches!(&refused, Err(Error::Invalid { address, message })
                if address.starts_with("band") && message.contains(wanted) && !message.contains("18446744073709551615")),
            "{refused:?}"
        );
    }
    Ok(())
}

/// The interior of an opaque picture stays opaque to the last bit under a turn by any angle or a scale above 1: the four
/// bilinear weights of a pixel sum to a step under 1 in 32-bit floats, and the alpha is divided by their sum.
#[test]
fn the_interior_of_an_opaque_layer_stays_exactly_opaque_when_it_is_turned_or_enlarged() -> Outcome {
    let source = varied(40, 40)?;
    for place in [
        Place::new().rotate(Deg(17.0)),
        Place::new().rotate(Deg(33.0)).scale(1.3),
        Place::new().skew_x(Deg(20.0)).scale(1.7).rotate(Deg(-40.0)),
    ] {
        let drawn = render(
            Layer::picture(source.clone()).place(place.dx(20.0).dy(20.0))?,
            (80, 80),
            80,
        )?;
        let mut interior = 0;
        for (x, y) in (0..80_usize).flat_map(|y| (0..80_usize).map(move |x| (x, y))) {
            if (f64::from(u32::try_from(x)?) - 39.5).hypot(f64::from(u32::try_from(y)?) - 39.5) <= 12.0 {
                interior += 1;
                assert!(
                    pixel(&drawn, x, y)[3].to_bits() == 1.0_f32.to_bits(),
                    "({x}, {y}): alpha {}",
                    pixel(&drawn, x, y)[3]
                );
            }
        }
        assert!(interior > 400);
    }
    Ok(())
}

/// A refusal says the range it checks, so the message and the check cannot disagree: the numbers in it are the limits.
#[test]
fn a_refusal_names_the_range_of_the_number() -> Outcome {
    let rows = [
        (Place::new().scale(0.0), ["scale", "0.001", "1000"]),
        (Place::new().aspect(2000.0), ["aspect", "0.001", "1000"]),
        (Place::new().skew_y(Deg(95.0)), ["skew_y", "-89", "89"]),
        (Place::new().dx(2e6), ["dx", "-1000000", "1000000"]),
        (Place::new().anchor(1.5, 0.5), ["anchor", "0", "1"]),
    ];
    for (place, words) in rows {
        let refused = refusal(Layer::fill("#ffffff")?.place(place)).unwrap_or_default();
        assert!(words.iter().all(|word| refused.contains(word)), "{refused}");
    }
    let refused =
        refusal(Layer::fill("#ffffff")?.fit([0.0, 0.0, 1.0, 1.0], Fit::Cover, [0.5, 1.5])).unwrap_or_default();
    assert!(
        ["align", "0", "1"].iter().all(|word| refused.contains(word)),
        "{refused}"
    );
    Ok(())
}

/// A scale that makes the drawn layer larger than 2^27 pixels is refused at render naming the layer, a fill included.
#[test]
fn a_scale_that_draws_more_than_2_pow_27_pixels_is_refused_naming_the_layer() -> Outcome {
    let layer = Layer::fill("#ffffff")?
        .named("band")?
        .place(Place::new().scale(1000.0))?;
    let refused = scene(100, 100, [layer])?.render(100);
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message }) if address.starts_with("band") && message.contains("134217728")),
        "{refused:?}"
    );
    Ok(())
}
