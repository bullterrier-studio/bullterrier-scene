//! Perspective: the fitted box's corners put on a quadrilateral. Expected values come from a homography solved by hand-written
//! elimination in this file (not the crate's closed form), from `place` where it makes the same map, and from the property
//! that a filtered render at half the size is the average of the one at the full size.

use bullterrier_scene::{Deg, Error, Fit, Layer, Place, Raster};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

use crate::common::{Outcome, close, made, numbered, pixel, pixels, rectangle, render, scene, solid};

type Corners = [[f64; 2]; 4];

/// Solves `matrix x = right` by Gauss-Jordan elimination with a pivot of the largest entry.
fn solve(mut matrix: Vec<Vec<f64>>, mut right: Vec<f64>) -> Vec<f64> {
    let size = right.len();
    for column in 0..size {
        let pivot = (column..size)
            .max_by(|&a, &b| matrix[a][column].abs().total_cmp(&matrix[b][column].abs()))
            .unwrap_or(column);
        matrix.swap(column, pivot);
        right.swap(column, pivot);
        for row in 0..size {
            if row != column {
                let factor = matrix[row][column] / matrix[column][column];
                let pivot_row = matrix[column].clone();
                for (entry, pivot_entry) in matrix[row].iter_mut().zip(pivot_row) {
                    *entry -= factor * pivot_entry;
                }
                right[row] -= factor * right[column];
            }
        }
    }
    (0..size).map(|row| right[row] / matrix[row][row]).collect()
}

/// The unit square point `(u, v)` that the quadrilateral `corners` has at `(x, y)`: the 8 by 8 system of the homography that
/// sends (0, 0), (1, 0), (1, 1), (0, 1) to the corners, then its inverse at the point.
fn unit_square_point(corners: Corners, (x, y): (f64, f64)) -> (f64, f64) {
    let square = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
    let (mut matrix, mut right) = (Vec::new(), Vec::new());
    for ((u, v), [px, py]) in square.into_iter().zip(corners) {
        matrix.push(vec![u, v, 1.0, 0.0, 0.0, 0.0, -u * px, -v * px]);
        right.push(px);
        matrix.push(vec![0.0, 0.0, 0.0, u, v, 1.0, -u * py, -v * py]);
        right.push(py);
    }
    let h = solve(matrix, right);
    // (x, y) = ((h0 u + h1 v + h2) / w, (h3 u + h4 v + h5) / w), w = h6 u + h7 v + 1: linear in (u, v) once x and y are known
    let rows = vec![
        vec![h[0] - x * h[6], h[1] - x * h[7]],
        vec![h[3] - y * h[6], h[4] - y * h[7]],
    ];
    let found = solve(rows, vec![x - h[2], y - h[5]]);
    (found[0], found[1])
}

/// A `width` by `height` picture of a ramp in each direction: red is the position across (the pixel's middle over the
/// width), green the position down.
fn ramps(width: usize, height: usize) -> Result<Raster, Error> {
    #[expect(clippy::cast_precision_loss, reason = "a small side")]
    made(width, height, |x, y| {
        [
            (x as f32 + 0.5) / width as f32,
            (y as f32 + 0.5) / height as f32,
            0.0,
            1.0,
        ]
    })
}

/// The quadrilateral of the near side wide (bottom) and the far side narrow (top): a floor receding.
const TILTED: Corners = [[26.0, 6.0], [38.0, 6.0], [58.0, 58.0], [6.0, 58.0]];

#[test]
fn the_corners_of_the_box_land_on_the_corners_and_every_point_between_is_where_a_hand_solved_homography_says() -> Outcome
{
    let quad: Corners = [[11.0, 7.0], [50.0, 15.0], [58.0, 55.0], [5.0, 44.0]];
    let layer = Layer::picture(ramps(32, 32)?).perspective(quad)?;
    let drawn = render(layer, (64, 64), 64)?;
    // the corners: a pixel inside each, on the line to the middle, has the unit square corner's colour to within the part of
    // a pixel it is in from the corner (the ramps change by 1/32 a source pixel, and a few source pixels are a pixel here)
    let middle = ((11.0 + 50.0 + 58.0 + 5.0) / 4.0, (7.0 + 15.0 + 55.0 + 44.0) / 4.0);
    for ([cx, cy], (u, v)) in quad
        .into_iter()
        .zip([(0.0_f32, 0.0_f32), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)])
    {
        let (x, y) = (cx + (middle.0 - cx) * 0.06, cy + (middle.1 - cy) * 0.06);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a pixel of the canvas"
        )]
        let got = pixel(&drawn, x as usize, y as usize);
        assert!(close(got, [u, v, 0.0, 1.0], 0.1), "corner ({cx}, {cy}): {got:?}");
    }
    // 25 points inside: the colour is the point's place in the box
    let mut counted = 0;
    for step_u in 1..=5 {
        for step_v in 1..=5 {
            let (u, v) = (f64::from(step_u) / 6.0, f64::from(step_v) / 6.0);
            // the homography of the quadrilateral, forward, by the same 8 by 8 solve: find the pixel whose centre is nearest
            // to where (u, v) lands, and check that pixel's own place instead
            let target = (
                quad[0][0] * (1.0 - u) * (1.0 - v)
                    + quad[1][0] * u * (1.0 - v)
                    + quad[2][0] * u * v
                    + quad[3][0] * (1.0 - u) * v,
                quad[0][1] * (1.0 - u) * (1.0 - v)
                    + quad[1][1] * u * (1.0 - v)
                    + quad[2][1] * u * v
                    + quad[3][1] * (1.0 - u) * v,
            );
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a pixel of the canvas"
            )]
            let (x, y) = (target.0 as usize, target.1 as usize);
            let (pu, pv) = unit_square_point(
                quad,
                (f64::from(u32::try_from(x)?) + 0.5, f64::from(u32::try_from(y)?) + 0.5),
            );
            if (0.08..0.92).contains(&pu) && (0.08..0.92).contains(&pv) {
                counted += 1;
                #[expect(clippy::cast_possible_truncation, reason = "a level")]
                let expected = [pu as f32, pv as f32, 0.0, 1.0];
                assert!(
                    close(pixel(&drawn, x, y), expected, 0.01),
                    "({x}, {y}): {:?} is not {expected:?}",
                    pixel(&drawn, x, y)
                );
            }
        }
    }
    assert!(counted >= 20, "{counted} points checked");
    Ok(())
}

#[test]
fn a_quadrilateral_that_is_the_box_is_the_identity_and_an_axis_aligned_one_is_what_place_makes_bit_for_bit() -> Outcome
{
    let picture = || Layer::picture(numbered(6, 4).unwrap_or_else(|_| unreachable!()));
    let same = |layer: Layer, other: Layer, what: &str| -> Outcome {
        let (a, b) = (render(layer, (16, 12), 16)?, render(other, (16, 12), 16)?);
        assert!(
            a.pixels()
                .iter()
                .zip(b.pixels())
                .all(|(x, y)| x.to_bits() == y.to_bits()),
            "{what}"
        );
        Ok(())
    };
    same(
        picture(),
        picture().perspective([[0.0, 0.0], [6.0, 0.0], [6.0, 4.0], [0.0, 4.0]])?,
        "the box itself",
    )?;
    same(
        picture().place(Place::new().dx(3.0).dy(2.0))?,
        picture().perspective([[3.0, 2.0], [9.0, 2.0], [9.0, 6.0], [3.0, 6.0]])?,
        "a move",
    )?;
    same(
        picture().place(Place::new().scale(2.0).anchor(0.0, 0.0))?,
        picture().perspective([[0.0, 0.0], [12.0, 0.0], [12.0, 8.0], [0.0, 8.0]])?,
        "a scale of 2",
    )?;
    same(
        picture().place(Place::new().scale(0.5).anchor(0.0, 0.0).dx(4.0).dy(1.0))?,
        picture().perspective([[4.0, 1.0], [7.0, 1.0], [7.0, 3.0], [4.0, 3.0]])?,
        "a scale of a half and a move",
    )?;
    // a reduction by 8/3, which is not a whole number of pixels: the exact area average of `place`, not a lattice of taps
    same(
        picture().place(Place::new().scale(0.375).anchor(0.0, 0.0).dx(2.0))?,
        picture().perspective([[2.0, 0.0], [4.25, 0.0], [4.25, 1.5], [2.0, 1.5]])?,
        "a scale of three eighths and a move",
    )?;
    same(
        picture().place(Place::new().flip_x(true))?,
        picture().perspective([[6.0, 0.0], [0.0, 0.0], [0.0, 4.0], [6.0, 4.0]])?,
        "a flip across x, which is a mirrored quadrilateral",
    )?;
    same(
        picture().place(Place::new().rotate(Deg(90.0)).dx(5.0).dy(4.0))?,
        picture().perspective([[10.0, 3.0], [10.0, 9.0], [6.0, 9.0], [6.0, 3.0]])?,
        "a quarter turn",
    )?;
    Ok(())
}

#[test]
fn a_mirrored_quadrilateral_is_accepted_and_is_the_picture_mirrored_on_the_clockwise_one() -> Outcome {
    let picture = ramps(24, 24)?;
    let mirrored_picture = made(24, 24, |x, y| pixel(&picture, 23 - x, y))?;
    let clockwise = [[10.0, 6.0], [50.0, 12.0], [56.0, 52.0], [4.0, 46.0]];
    // top left and top right swapped, bottom left and bottom right swapped: the box seen from behind
    let counter = [clockwise[1], clockwise[0], clockwise[3], clockwise[2]];
    let by_mirror = render(Layer::picture(picture).perspective(counter)?, (64, 64), 64)?;
    let by_picture = render(Layer::picture(mirrored_picture).perspective(clockwise)?, (64, 64), 64)?;
    for (got, expected) in pixels(&by_mirror).zip(pixels(&by_picture)) {
        assert!(close(got, expected, 1e-5), "{got:?} is not {expected:?}");
    }
    Ok(())
}

/// Each row: what is wrong with the corners and the field the refusal names.
#[test]
fn corners_that_are_not_a_strictly_convex_quadrilateral_or_are_out_of_range_are_refused_naming_them() -> Outcome {
    let nan = f64::NAN;
    let rows: Vec<(&str, Corners, bool)> = vec![
        (
            "a rectangle",
            [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            true,
        ),
        ("a trapezoid", [[3.0, 0.0], [7.0, 0.0], [10.0, 10.0], [0.0, 10.0]], true),
        (
            "a mirrored rectangle",
            [[10.0, 0.0], [0.0, 0.0], [0.0, 10.0], [10.0, 10.0]],
            true,
        ),
        (
            "a quadrilateral on the far side of the range",
            [[-1e6, -1e6], [1e6, -1e6], [1e6, 1e6], [-1e6, 1e6]],
            true,
        ),
        ("a dart", [[0.0, 0.0], [10.0, 0.0], [2.0, 2.0], [0.0, 10.0]], false),
        (
            "a dart the other way",
            [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [8.0, 2.0]],
            false,
        ),
        ("a bowtie", [[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]], false),
        (
            "three corners on a line",
            [[0.0, 0.0], [5.0, 0.0], [10.0, 0.0], [0.0, 10.0]],
            false,
        ),
        (
            "a corner twice",
            [[0.0, 0.0], [10.0, 0.0], [10.0, 0.0], [0.0, 10.0]],
            false,
        ),
        ("all on a line", [[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0]], false),
        ("a NaN", [[0.0, 0.0], [10.0, 0.0], [10.0, nan], [0.0, 10.0]], false),
        (
            "an infinity",
            [[0.0, 0.0], [f64::INFINITY, 0.0], [10.0, 10.0], [0.0, 10.0]],
            false,
        ),
        (
            "a corner at 1e9",
            [[0.0, 0.0], [1e9, 0.0], [10.0, 10.0], [0.0, 10.0]],
            false,
        ),
    ];
    for (what, corners, good) in rows {
        let layer = Layer::fill("#ffffff")?.perspective(corners);
        let refused = match &layer {
            Err(Error::Invalid { address, .. }) => Some(address.as_str()),
            _ => None,
        };
        assert_eq!(refused.is_none(), good, "{what}: {layer:?}");
        assert!(good || refused == Some("corners"), "{what}: {layer:?}");
    }
    Ok(())
}

/// A dart with each corner in turn the reflex one (moved to the other side of the line through its neighbours), clockwise and
/// counter-clockwise (the same corners in the other order), and a corner out of range in y alone, are refused naming `corners`.
#[test]
fn a_dart_with_any_corner_reflex_in_either_direction_and_a_y_out_of_range_are_refused() {
    let square: Corners = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
    let refused = |corners: Corners| -> Option<String> {
        match Layer::fill("#ffffff").and_then(|layer| layer.perspective(corners)) {
            Err(Error::Invalid { address, .. }) => Some(address),
            _ => None,
        }
    };
    for reflex in 0..4 {
        let mut dart = square;
        // 20% of the way back from the middle: past the line through the two neighbours
        dart[reflex] = [
            5.0 - 0.2 * (square[reflex][0] - 5.0),
            5.0 - 0.2 * (square[reflex][1] - 5.0),
        ];
        assert_eq!(
            refused(dart).as_deref(),
            Some("corners"),
            "corner {reflex} reflex, clockwise"
        );
        let mirrored = [dart[0], dart[3], dart[2], dart[1]];
        assert_eq!(
            refused(mirrored).as_deref(),
            Some("corners"),
            "corner {reflex} reflex, counter-clockwise"
        );
    }
    assert_eq!(
        refused([[0.0, 0.0], [10.0, 0.0], [10.0, 1_000_001.0], [0.0, 10.0]]).as_deref(),
        Some("corners")
    );
    assert_eq!(
        refused([[0.0, -1_000_001.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]).as_deref(),
        Some("corners")
    );
    assert_eq!(refused([[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]), None);
}

/// `fit` then `perspective`: the fitted box, not the picture's own box, lands on the quadrilateral, for a picture wider than tall
/// and taller than wide, each contained, stretched and covered into a box, onto a parallelogram (the exact path) and onto a
/// general quadrilateral (the projective one). The colour at each of 20 or more pixels is the picture's own ramps at the point
/// of the picture the hand-solved homography says is there, the window of a cover included.
#[test]
fn the_fitted_box_lands_on_the_quadrilateral_whatever_the_fit_and_the_shape_of_the_picture() -> Outcome {
    let parallelogram: Corners = [[10.0, 10.0], [90.0, 20.0], [100.0, 70.0], [20.0, 60.0]];
    let general: Corners = [[20.0, 8.0], [100.0, 20.0], [110.0, 100.0], [8.0, 90.0]];
    for (width, height) in [(64_usize, 32_usize), (32, 64)] {
        // (what, the fit, the box, the window of the picture that is shown: x, y, width, height, and the fitted box's size)
        let (across_size, down_size) = (f64::from(u32::try_from(width)?), f64::from(u32::try_from(height)?));
        let fits = [
            ("contain", Fit::Contain, [0.0, 0.0, 40.0, 40.0]),
            ("stretch", Fit::Stretch, [0.0, 0.0, 48.0, 24.0]),
            ("cover", Fit::Cover, [0.0, 0.0, 20.0, 50.0]),
        ];
        for (what, how, area) in fits {
            // the part of the picture that is shown, by the rule of the fit worked out here
            let (across, down) = (area[2] / across_size, area[3] / down_size);
            let window = match how {
                Fit::Cover => {
                    let factor = across.max(down);
                    let (shown_w, shown_h) = (area[2] / factor, area[3] / factor);
                    [
                        (across_size - shown_w) / 2.0,
                        (down_size - shown_h) / 2.0,
                        shown_w,
                        shown_h,
                    ]
                }
                _ => [0.0, 0.0, across_size, down_size],
            };
            for (shape, quad) in [("a parallelogram", parallelogram), ("a quadrilateral", general)] {
                let layer = Layer::picture(ramps(width, height)?)
                    .fit(area, how, [0.5, 0.5])?
                    .perspective(quad)?;
                let drawn = render(layer, (128, 128), 128)?;
                let mut checked = 0;
                for step_x in 1..=8 {
                    for step_y in 1..=8 {
                        let (u, v) = (
                            0.1 + 0.8 * f64::from(step_x - 1) / 7.0,
                            0.1 + 0.8 * f64::from(step_y - 1) / 7.0,
                        );
                        // the pixel the unit square point (u, v) lands on, by bilinear blend of the corners for the guess, then
                        // its true unit square place by the hand-solved homography
                        let guess = (
                            quad[0][0] * (1.0 - u) * (1.0 - v)
                                + quad[1][0] * u * (1.0 - v)
                                + quad[2][0] * u * v
                                + quad[3][0] * (1.0 - u) * v,
                            quad[0][1] * (1.0 - u) * (1.0 - v)
                                + quad[1][1] * u * (1.0 - v)
                                + quad[2][1] * u * v
                                + quad[3][1] * (1.0 - u) * v,
                        );
                        #[expect(
                            clippy::cast_possible_truncation,
                            clippy::cast_sign_loss,
                            reason = "a pixel of the canvas"
                        )]
                        let (x, y) = (guess.0 as usize, guess.1 as usize);
                        let (pu, pv) = unit_square_point(
                            quad,
                            (f64::from(u32::try_from(x)?) + 0.5, f64::from(u32::try_from(y)?) + 0.5),
                        );
                        if (0.05..0.95).contains(&pu) && (0.05..0.95).contains(&pv) {
                            checked += 1;
                            #[expect(clippy::cast_possible_truncation, reason = "a level")]
                            let expected = [
                                ((window[0] + pu * window[2]) / across_size) as f32,
                                ((window[1] + pv * window[3]) / down_size) as f32,
                                0.0,
                                1.0,
                            ];
                            let got = pixel(&drawn, x, y);
                            assert!(
                                close(got, expected, 0.01),
                                "{width}x{height} {what} on {shape}, ({x}, {y}): {got:?} is not {expected:?}"
                            );
                        }
                    }
                }
                assert!(checked >= 20, "{width}x{height} {what} on {shape}: {checked} points");
            }
        }
    }
    Ok(())
}

/// The last of `place` and `perspective` wins, in either order; `fit` comes first.
#[test]
fn the_last_of_place_and_perspective_wins_and_fit_comes_first() -> Outcome {
    let picture = || Layer::picture(numbered(6, 4).unwrap_or_else(|_| unreachable!()));
    let quad = [[3.0, 2.0], [9.0, 2.0], [9.0, 6.0], [3.0, 6.0]];
    let bits = |layer: Layer| -> Result<Vec<u32>, Error> {
        Ok(render(layer, (16, 12), 16)?
            .pixels()
            .iter()
            .map(|value| value.to_bits())
            .collect())
    };
    let moved = bits(picture().place(Place::new().dx(3.0).dy(2.0))?)?;
    assert_eq!(
        bits(
            picture()
                .place(Place::new().dx(9.0))
                .and_then(|layer| layer.perspective(quad))?
        )?,
        moved,
        "place then perspective"
    );
    let unmoved = bits(picture())?;
    assert_eq!(
        bits(
            picture()
                .perspective(quad)?
                .place(Place::new())
                .unwrap_or_else(|_| unreachable!())
        )?,
        unmoved,
        "perspective then place"
    );
    // fit first: a 4 by 2 picture contained in an 8 by 8 box is the box (0, 2, 8, 4); a quadrilateral that is that rectangle changes nothing
    let contained = picture_of(4, 2)?.fit([0.0, 0.0, 8.0, 8.0], Fit::Contain, [0.5, 0.5])?;
    let by_fit = render(contained.clone(), (8, 8), 8)?;
    let by_quad = render(
        contained.perspective([[0.0, 2.0], [8.0, 2.0], [8.0, 6.0], [0.0, 6.0]])?,
        (8, 8),
        8,
    )?;
    assert_eq!(by_fit.pixels(), by_quad.pixels());
    Ok(())
}

/// `place` after `perspective` replaces the perspective and starts from the default placement plus what it names: the fields
/// of a `place` before the perspective are not brought back. `perspective` after `place` replaces it.
#[test]
fn place_after_perspective_starts_from_the_defaults_and_perspective_after_place_replaces_it() -> Outcome {
    let picture = || Layer::picture(numbered(6, 4).unwrap_or_else(|_| unreachable!()));
    let quad = [[3.0, 2.0], [9.0, 2.0], [9.0, 6.0], [3.0, 6.0]];
    let bits = |layer: Layer| -> Result<Vec<u32>, Error> {
        Ok(render(layer, (16, 12), 16)?
            .pixels()
            .iter()
            .map(|value| value.to_bits())
            .collect())
    };
    let turned_then_moved = bits(picture().place(Place::new().rotate(Deg(90.0)).dx(3.0))?)?;
    let only_moved = bits(picture().place(Place::new().dx(3.0))?)?;
    assert!(
        turned_then_moved
            .iter()
            .zip(&only_moved)
            .any(|(turned, moved)| turned != moved)
    );
    let revived = bits(
        picture()
            .place(Place::new().rotate(Deg(90.0)))?
            .perspective(quad)?
            .place(Place::new().dx(3.0))?,
    )?;
    assert!(revived == only_moved, "the turn from before the perspective came back");
    let replaced = bits(
        picture()
            .place(Place::new().rotate(Deg(90.0)).dx(7.0))?
            .perspective(quad)?,
    )?;
    assert_eq!(
        replaced,
        bits(picture().perspective(quad)?)?,
        "a place before the perspective still counts"
    );
    Ok(())
}

fn picture_of(width: usize, height: usize) -> Result<Layer, Error> {
    Ok(Layer::picture(numbered(width, height)?))
}

#[test]
fn perspective_returns_a_new_layer_and_leaves_the_old_one_as_it_was() -> Outcome {
    let layer = picture_of(6, 4)?;
    let before = render(layer.clone(), (8, 8), 8)?;
    let _tilted = layer.clone().perspective(TILTED)?;
    assert_eq!(render(layer, (8, 8), 8)?.pixels(), before.pixels());
    Ok(())
}

/// A quadrilateral partly or wholly outside the canvas is cut at it, and a wholly outside one draws nothing.
#[test]
fn a_quadrilateral_partly_or_wholly_off_the_canvas_is_cut_at_it() -> Outcome {
    let picture = ramps(16, 16)?;
    let partly = render(
        Layer::picture(picture.clone()).perspective([[-20.0, -10.0], [30.0, 0.0], [40.0, 40.0], [-30.0, 50.0]])?,
        (32, 32),
        32,
    )?;
    assert!(pixels(&partly).all(|got| got.iter().all(|value| (0.0..=1.0).contains(value))));
    assert!(close(pixel(&partly, 16, 20), [0.5, 0.5, 0.0, 1.0], 0.2));
    for corners in [
        [[100.0, 100.0], [120.0, 100.0], [120.0, 130.0], [100.0, 140.0]],
        [[-300.0, -300.0], [-200.0, -300.0], [-210.0, -200.0], [-290.0, -190.0]],
    ] {
        let off = render(Layer::picture(picture.clone()).perspective(corners)?, (32, 32), 32)?;
        assert!(off.pixels().iter().all(|&value| value == 0.0));
    }
    Ok(())
}

/// The near side is drawn at the scale it has: a drawing whose frame has a hard edge, on a quadrilateral four times wider at the
/// bottom, has that edge sharp at the bottom (from clear to full red within two pixels), as it would if drawn at the near scale
/// and not enlarged from the scale of the far side.
#[test]
fn the_near_side_of_a_perspective_is_drawn_at_its_own_scale_and_is_sharp() -> Outcome {
    // a 16 by 16 frame, red from x = 8 to the right
    let layer = Layer::drawing(&rectangle((16, 16), (8.0, 0.0), (16.0, 16.0), "#ff0000"))?.perspective([
        [28.0, 4.0],
        [36.0, 4.0],
        [64.0, 60.0],
        [0.0, 60.0],
    ])?;
    let drawn = render(layer, (64, 64), 64)?;
    // the red starts at the middle of each row; count the pixels of the row at the bottom that are neither clear nor full
    let row = 58;
    let between = (0..64)
        .filter(|&x| (0.05..0.95).contains(&pixel(&drawn, x, row)[3]))
        .count();
    // the quadrilateral's own edges, left and right, are the two soft pixels each: the middle edge adds at most two
    assert!(between <= 6, "{between} soft pixels in row {row}");
    let middle: Vec<f32> = (28..38).map(|x| pixel(&drawn, x, row)[3]).collect();
    let soft_in_the_middle = middle.iter().filter(|&&alpha| (0.05..0.95).contains(&alpha)).count();
    assert!(soft_in_the_middle <= 2, "{middle:?}");
    Ok(())
}

/// A layer drawn under a perspective that would need more than 2^27 pixels is drawn at fewer, not refused: a drawing allocates its
/// whole frame at the scale it is asked, 16,384 by 8,192 pixels at the cut (a frame of 4 by 2 on a quadrilateral half a million
/// times as large), where the request without the cut would be 2 million by 1 million. The frame's red rectangle covers the middle.
#[test]
fn a_perspective_that_asks_for_more_than_2_pow_27_pixels_is_drawn_smaller_not_refused() -> Outcome {
    let layer = Layer::drawing(&rectangle((4, 2), (1.0, 0.5), (3.0, 1.5), "#ff0000"))?.perspective([
        [-1e6, -1e6],
        [1e6, -1e6],
        [1e6, 1e6],
        [-1e6, 1e6],
    ])?;
    let drawn = render(layer, (16, 16), 16)?;
    // the canvas is the middle of the quadrilateral, which the frame's rectangle covers
    assert!(pixels(&drawn).all(|got| close(got, [1.0, 0.0, 0.0, 1.0], 1e-6)));
    // while a place that asks for as much, with a look run on it, is refused
    let looked = Layer::fill("#ffffff")?
        .place(Place::new().scale(1000.0))?
        .look(r#"{"steps": [{"effect": "grade", "params": {"exposure": 0.5}}]}"#)?;
    let refused = scene(100, 100, [looked])?.render(100);
    assert!(refused.is_err());
    Ok(())
}

/// The far side of a floor does not alias. A picture of 1-pixel stripes across (every other column white) on a quadrilateral
/// whose top is a tenth of its bottom: the average of a stripe pair is grey, so the deviation from grey of a row falls toward
/// the far side, row by row, within the noise of the taps (0.08, where the footprint is about two pixels and the taps are few);
/// at the far end the rows are close to grey, and at the near end, where a stripe is a pixel, they are not.
#[test]
fn stripes_on_a_receding_plane_fade_toward_grey_row_by_row() -> Outcome {
    let stripes = made(128, 128, |x, _| {
        let level = if x % 2 == 0 { 1.0 } else { 0.0 };
        [level, level, level, 1.0]
    })?;
    // near side (bottom) 120 wide, far side (top) 12 wide
    let layer = Layer::picture(stripes).perspective([[58.0, 4.0], [70.0, 4.0], [124.0, 124.0], [4.0, 124.0]])?;
    let drawn = render(layer, (128, 128), 128)?;
    // the mean deviation from grey of the pixels well inside the quadrilateral, per block of 8 rows
    let deviation = |rows: std::ops::Range<usize>| -> f32 {
        let mut sum = 0.0;
        let mut count = 0;
        for y in rows {
            // half of the row's width at this depth, about the middle (64): the quadrilateral is 12 + 108 (y - 4) / 120 wide
            let half = usize::try_from(i64::from(
                (6 + 54 * (i32::try_from(y).unwrap_or(0) - 4) / 120 - 3).max(1),
            ))
            .unwrap_or(1);
            for x in 64 - half..64 + half {
                let got = pixel(&drawn, x, y);
                if got[3] > 0.999 {
                    sum += (got[0] - 0.5).abs();
                    count += 1;
                }
            }
        }
        sum / f32::from(u16::try_from(count.max(1)).unwrap_or(u16::MAX))
    };
    let blocks: Vec<f32> = (0..14).map(|block| deviation(8 + block * 8..16 + block * 8)).collect();
    // from the far block (first) to the near block (last) the deviation rises: each block at most the noise below the one before
    for pair in blocks.windows(2) {
        assert!(
            pair[1] >= pair[0] - 0.06,
            "the deviation falls toward the near side: {blocks:?}"
        );
    }
    assert!(blocks[0] < 0.1, "the far side is not grey: {blocks:?}");
    assert!(blocks[13] > 0.1, "the near side lost its stripes: {blocks:?}");
    Ok(())
}

/// A render at half the size is the box average of the render at the full size, for a strongly tilted plane (the far edge an
/// eighth of the near one) of 1-pixel stripes: the case an unfiltered sample fails (it beats). Compared on the pixels that are
/// inside the quadrilateral at both sizes, by their mean over the rows; the tolerance is the noise of the taps.
#[test]
fn a_strongly_tilted_plane_at_half_the_size_is_the_box_average_of_the_whole_size() -> Outcome {
    let stripes = made(256, 256, |x, y| {
        let level = if (x + y / 4) % 2 == 0 { 1.0 } else { 0.0 };
        [level, level, level, 1.0]
    })?;
    let corners = |scale: f64| -> Corners {
        [
            [56.0 * scale, 8.0 * scale],
            [72.0 * scale, 8.0 * scale],
            [124.0 * scale, 120.0 * scale],
            [4.0 * scale, 120.0 * scale],
        ]
    };
    let layer = Layer::picture(stripes);
    let full = render(layer.clone().perspective(corners(1.0))?, (128, 128), 128)?;
    let half = render(layer.perspective(corners(1.0))?, (128, 128), 64)?;
    let (mut worst, mut compared) = (0.0_f32, 0);
    for y in 0..64 {
        for x in 0..64 {
            let near = [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dy)| pixel(&full, 2 * x + dx, 2 * y + dy));
            if near.iter().any(|got| got[3] < 0.999) || pixel(&half, x, y)[3] < 0.999 {
                continue;
            }
            compared += 1;
            let mean = near.iter().map(|got| got[0]).sum::<f32>() / 4.0;
            worst = worst.max((pixel(&half, x, y)[0] - mean).abs());
        }
    }
    assert!(compared > 500, "{compared} pixels compared");
    assert!(
        worst < 0.35,
        "the half-size render differs from the box average of the whole by {worst}"
    );
    Ok(())
}

/// The far edge of a perspective layer is soft by coverage as an edge of `place` is: along each row of a slanted edge there is a
/// pixel that is neither clear nor opaque.
#[test]
fn the_slanted_edges_of_a_perspective_layer_are_soft_not_jagged() -> Outcome {
    let layer = Layer::picture(solid(16, 16, [1.0, 0.5, 0.0, 1.0])?).perspective(TILTED)?;
    let drawn = render(layer, (64, 64), 64)?;
    let rows_with_a_soft_pixel = (10..54)
        .filter(|&y| (0..64).any(|x| (0.02..0.98).contains(&pixel(&drawn, x, y)[3])))
        .count();
    assert_eq!(rows_with_a_soft_pixel, 44);
    Ok(())
}

/// A valid quadrilateral that is nearly a line renders cleanly: pixels in range, never NaN.
#[test]
fn a_very_flat_quadrilateral_renders_or_is_refused_but_never_gives_nan() -> Outcome {
    let (mut accepted, mut refused) = (0, 0);
    for flat in [1e-3, 1e-6, 1e-9, 1e-12, 1e-15] {
        let corners = [
            [0.0, 10.0],
            [30.0, 10.0 + flat],
            [30.0, 10.0 + 2.0 * flat],
            [0.0, 10.0 + flat],
        ];
        match Layer::picture(ramps(8, 8)?).perspective(corners) {
            Ok(layer) => {
                accepted += 1;
                let drawn = render(layer, (32, 32), 32)?;
                assert!(
                    pixels(&drawn).all(|got| got.iter().all(|value| (0.0..=1.0).contains(value))),
                    "flat {flat}"
                );
            }
            Err(Error::Invalid { address, message }) => {
                refused += 1;
                assert_eq!(address, "corners", "flat {flat}");
                assert!(message.contains("convex"), "flat {flat}: {message}");
            }
            Err(other) => return Err(other.into()),
        }
    }
    // the flattest that is still a quadrilateral renders, and the flat ones the arithmetic cannot tell from a line are refused
    assert!(accepted >= 2 && refused >= 1, "{accepted} accepted, {refused} refused");
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 60, rng_seed: RngSeed::Fixed(20_261_114), ..ProptestConfig::default() })]

    /// A rectangle given in decimals (a corner sum that is a few units in the last place from 0, not 0) is an axis-aligned
    /// quadrilateral all the same: it takes the exact path of `place`, and gives what the equivalent `place` gives. The maps are
    /// formed in a different order of operations, so they may differ in the last place and not in the pixels: the tolerance is
    /// 1e-5 (a step of the 32-bit floats of the blend), where a lattice of taps in place of the exact area average differs by
    /// tenths.
    #[test]
    fn a_rectangle_in_decimals_is_what_place_makes(
        left in 0_u32..300, top in 0_u32..300, wide in 40_u32..400, tall in 40_u32..400,
    ) {
        let (x0, y0) = (f64::from(left) / 10.0, f64::from(top) / 10.0);
        let (x1, y1) = (x0 + f64::from(wide) / 10.0, y0 + f64::from(tall) / 10.0);
        let picture = || Layer::picture(numbered(8, 12).unwrap_or_else(|_| unreachable!()));
        let by_quad = render(picture().perspective([[x0, y0], [x1, y0], [x1, y1], [x0, y1]])?, (80, 80), 80)?;
        let scale = (x1 - x0) / 8.0;
        let aspect = (y1 - y0) / 12.0 / scale;
        let by_place = render(picture().place(Place::new().scale(scale).aspect(aspect).anchor(0.0, 0.0).dx(x0).dy(y0))?, (80, 80), 80)?;
        // compared as the blend sees them: an alpha of 1e-16 past an edge carries a colour that shows nothing
        let seen = |pixel: [f32; 4]| [pixel[0] * pixel[3], pixel[1] * pixel[3], pixel[2] * pixel[3], pixel[3]];
        for (got, expected) in pixels(&by_quad).zip(pixels(&by_place)) {
            prop_assert!(close(seen(got), seen(expected), 1e-5), "{got:?} is not {expected:?}");
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 3000, rng_seed: RngSeed::Fixed(20_261_113), ..ProptestConfig::default() })]

    /// Hostile corners (huge, tiny, flat, any order), with a turn and a scale as well on half the cases: whatever is accepted
    /// renders a picture in 0..=1 with no colour where there is no alpha, and the same twice.
    #[test]
    fn any_accepted_quadrilateral_gives_a_picture_in_range_and_the_same_one_twice(
        coordinates in proptest::array::uniform8(prop_oneof![-1e6_f64..1e6, -100.0_f64..100.0, -2.0_f64..40.0, Just(0.0), Just(8.0)]),
        place_too: bool, scale in 0.001_f64..1000.0, turn in -360.0_f64..360.0,
    ) {
        let [x0, y0, x1, y1, x2, y2, x3, y3] = coordinates;
        if let Ok(layer) = Layer::picture(numbered(6, 4)?).perspective([[x0, y0], [x1, y1], [x2, y2], [x3, y3]]) {
            let layer = if place_too { layer.place(Place::new().scale(scale).rotate(Deg(turn)))? } else { layer };
            let first = render(layer.clone(), (16, 12), 16)?;
            let second = render(layer, (16, 12), 16)?;
            prop_assert_eq!(first.pixels(), second.pixels());
            for got in pixels(&first) {
                prop_assert!(got.iter().all(|value| (0.0..=1.0).contains(value)), "{got:?}");
                prop_assert!(got[3] > 0.0 || got[..3] == [0.0, 0.0, 0.0], "{got:?}");
            }
        }
    }
}

/// A look on a layer whose near side is magnified far past the canvas is drawn smaller, as a layer with no look is, instead of
/// asking image for a picture past its limit (2^26 pixels): the floor of a world with a glow on it.
#[test]
fn a_looked_layer_with_a_strongly_magnified_near_side_renders() -> Outcome {
    let at_defaults = r#"{"steps": [{"effect": "grade"}]}"#;
    // an 8 x 8 box drawn about 1000 x 1500 times larger at its near edge: 8e7 pixels, between 2^26 and 2^27
    let floor = Layer::fill("#ff2bd6")?
        .fit([0.0, 0.0, 8.0, 8.0], Fit::Stretch, [0.5, 0.5])?
        .look(at_defaults)?
        .perspective([[32.0, 0.0], [34.0, 0.0], [4000.0, 64.0], [-4000.0, 64.0]])?;
    let drawn = render(floor, (64, 64), 64)?;
    assert!(pixel(&drawn, 32, 60)[3] > 0.99, "{:?}", pixel(&drawn, 32, 60));
    Ok(())
}
