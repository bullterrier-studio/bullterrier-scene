//! A picture as a layer: one design pixel to a source pixel at the canvas origin, resampled once to the output size by
//! bilinear interpolation of premultiplied colour, cut at the canvas. Expected values are worked out by hand.

use bullterrier_scene::{Error, Layer, Raster};
use proptest::prelude::*;

use crate::common::{Outcome, all_exactly, close, pixel, pixels, scene, solid, varied};

/// Render `picture` alone on a canvas of its own size, at `size`.
fn alone(picture: Raster, size: u32) -> Result<Raster, Error> {
    let (width, height) = (
        u32::try_from(picture.width()).unwrap_or(1),
        u32::try_from(picture.height()).unwrap_or(1),
    );
    scene(width, height, [Layer::picture(picture)])?.render(size)
}

#[test]
fn a_raster_is_whole_pixels_of_four_values_from_0_to_1() {
    let rows: [(&str, usize, usize, Vec<f32>, bool); 8] = [
        ("one pixel", 1, 1, vec![0.0, 0.5, 1.0, 1.0], true),
        ("two pixels", 2, 1, vec![0.0; 8], true),
        ("no width", 0, 1, vec![], false),
        ("no height", 1, 0, vec![], false),
        ("too few values", 2, 2, vec![0.0; 15], false),
        ("too many values", 1, 1, vec![0.0; 5], false),
        ("a value above 1", 1, 1, vec![0.0, 0.0, 1.5, 1.0], false),
        ("a NaN", 1, 1, vec![0.0, f32::NAN, 0.0, 1.0], false),
    ];
    for (what, width, height, values, good) in rows {
        let made = Raster::rgba(width, height, values);
        assert_eq!(made.is_ok(), good, "{what}: {made:?}");
        if let Err(error) = made {
            assert!(error.to_string().contains("pixels"), "{what}: {error}");
        }
    }
}

#[test]
fn a_constant_picture_stays_constant_at_any_size() -> Outcome {
    for colour in [[0.2, 0.4, 0.6, 1.0], [0.9, 0.1, 0.3, 0.5]] {
        for size in [1, 3, 7, 20, 101] {
            let picture = alone(solid(7, 5, colour)?, size)?;
            assert!(
                pixels(&picture).all(|got| close(got, colour, 1e-6)),
                "{colour:?} at {size}"
            );
        }
    }
    Ok(())
}

/// One design pixel is one pixel of the picture, and at one output pixel to the design pixel nothing is resampled: an
/// opaque picture comes out bit for bit, a translucent one within a rounding step of its premultiplied round trip.
#[test]
fn at_one_output_pixel_to_the_design_pixel_the_picture_is_unchanged() -> Outcome {
    let source = varied(6, 4)?;
    assert_eq!(alone(source.clone(), 6)?.pixels(), source.pixels());
    let mut translucent = source.into_pixels();
    translucent
        .iter_mut()
        .skip(3)
        .step_by(4)
        .for_each(|alpha| *alpha = 0.37);
    let picture = Raster::rgba(6, 4, translucent)?;
    for (got, expected) in pixels(&alone(picture.clone(), 6)?).zip(pixels(&picture)) {
        assert!(close(got, expected, 1e-6), "{got:?} is not {expected:?}");
    }
    Ok(())
}

/// A 2 by 2 picture at 2x. Output pixel `i` looks at source position `(i + 0.5) / 2 - 0.5` = -0.25, 0.25, 0.75, 1.25; a
/// position outside the picture takes its edge. So the weights of the source columns (and rows) 0 and 1 are
/// (1, 0), (0.75, 0.25), (0.25, 0.75), (0, 1) for output columns 0 to 3.
#[test]
fn a_two_by_two_picture_at_twice_the_size_is_the_hand_worked_bilinear_blend() -> Outcome {
    let values = [[0.0_f32, 0.4], [0.8, 0.2]]; // the red channel of the rows of the picture
    let mut pixels_in = Vec::new();
    for row in values {
        for red in row {
            pixels_in.extend([red, 0.0, 0.0, 1.0]);
        }
    }
    let drawn = alone(Raster::rgba(2, 2, pixels_in)?, 4)?;
    let weights = [[1.0_f32, 0.0], [0.75, 0.25], [0.25, 0.75], [0.0, 1.0]];
    for (y, row) in weights.iter().enumerate() {
        for (x, column) in weights.iter().enumerate() {
            let expected: f32 = (0..2)
                .flat_map(|a| (0..2).map(move |b| (a, b)))
                .map(|(a, b)| row[a] * column[b] * values[a][b])
                .sum();
            let got = pixel(&drawn, x, y);
            assert!(
                close(got, [expected, 0.0, 0.0, 1.0], 1e-6),
                "({x}, {y}): {got:?} is not {expected}"
            );
        }
    }
    Ok(())
}

/// A box that is not whole in output pixels is drawn at the next whole size, the whole picture stretched onto it. A 3
/// pixel row ramp 0, 0.5, 1 on a canvas of 3 at size 5: output pixel `i` looks at `(i + 0.5) * 3 / 5 - 0.5` = -0.2, 0.4,
/// 1.0, 1.6, 2.2, which is the red 0 (edge), 0.2, 0.5, 0.8, 1 (edge).
#[test]
fn a_picture_that_does_not_fit_whole_pixels_is_stretched_onto_the_next_whole_size() -> Outcome {
    let ramp = Raster::rgba(3, 1, vec![0.0, 0.0, 0.0, 1.0, 0.5, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0])?;
    let drawn = scene(3, 1, [Layer::picture(ramp)])?.render(5)?;
    assert_eq!((drawn.width(), drawn.height()), (5, 2));
    for (x, red) in [0.0, 0.2, 0.5, 0.8, 1.0].into_iter().enumerate() {
        for y in 0..2 {
            assert!(close(pixel(&drawn, x, y), [red, 0.0, 0.0, 1.0], 1e-6), "({x}, {y})");
        }
    }
    Ok(())
}

/// Colour is interpolated premultiplied: a transparent pixel adds no colour. An opaque red beside a transparent blue,
/// at 2x: the pixel at source position 0.25 is 0.75 red and 0.25 clear, so alpha 0.75 and a straight colour of pure red.
#[test]
fn a_transparent_neighbour_adds_no_colour_to_the_blend() -> Outcome {
    let edge = Raster::rgba(2, 1, vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0])?;
    let drawn = scene(2, 1, [Layer::picture(edge)])?.render(4)?;
    assert!(
        close(pixel(&drawn, 1, 0), [1.0, 0.0, 0.0, 0.75], 1e-6),
        "{:?}",
        pixel(&drawn, 1, 0)
    );
    assert!(
        close(pixel(&drawn, 2, 0), [1.0, 0.0, 0.0, 0.25], 1e-6),
        "{:?}",
        pixel(&drawn, 2, 0)
    );
    Ok(())
}

/// A picture smaller than the canvas covers its own pixels from the origin and nothing else; a larger one is cut at the
/// canvas.
#[test]
fn a_picture_sits_at_the_canvas_origin_one_design_pixel_to_a_pixel_and_is_cut_at_the_canvas() -> Outcome {
    let small = scene(6, 4, [Layer::picture(solid(3, 2, [1.0, 0.0, 0.0, 1.0])?)])?.render(6)?;
    for y in 0..4 {
        for x in 0..6 {
            let inside = x < 3 && y < 2;
            let expected = if inside { [1.0, 0.0, 0.0, 1.0] } else { [0.0; 4] };
            assert!(close(pixel(&small, x, y), expected, 0.0), "({x}, {y})");
        }
    }
    let large = varied(6, 4)?;
    let cut = scene(3, 2, [Layer::picture(large.clone())])?.render(3)?;
    for y in 0..2 {
        for x in 0..3 {
            assert!(close(pixel(&cut, x, y), pixel(&large, x, y), 0.0), "({x}, {y})");
        }
    }
    Ok(())
}

#[test]
fn a_picture_file_draws_what_the_same_picture_in_memory_draws() -> Outcome {
    let source = varied(5, 3)?;
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("photo.png");
    let image = bullterrier_image::Image::rgba(3, 5, source.pixels())?;
    bullterrier_image::write(&path, &image)?;
    let from_file = scene(5, 3, [Layer::picture_file(&path)])?.render(10)?;
    let from_memory = scene(5, 3, [Layer::picture(source)])?.render(10)?;
    assert_eq!((from_file.width(), from_file.height()), (10, 6));
    for (got, expected) in pixels(&from_file).zip(pixels(&from_memory)) {
        assert!(close(got, expected, 1e-6), "{got:?} is not {expected:?}");
    }
    Ok(())
}

#[test]
fn a_picture_file_that_cannot_be_read_is_a_read_error_naming_the_layer_and_the_file() -> Outcome {
    let folder = tempfile::tempdir()?;
    let missing = folder.path().join("missing.png");
    let text = folder.path().join("notes.txt");
    std::fs::write(&text, "not a picture")?;
    for (what, path) in [
        ("a missing file", missing),
        ("a file that is no picture", text),
        ("a folder", folder.path().to_path_buf()),
    ] {
        let canvas = scene(4, 4, [Layer::picture_file(&path).named("photo")?])?;
        let refused = canvas.render(4);
        assert!(
            matches!(&refused, Err(Error::Read { path: named, message }) if named.contains(path.to_str().unwrap_or("?")) && message.contains("photo")),
            "{what}: {refused:?}"
        );
    }
    Ok(())
}

/// The four weights of a bilinear sample are not exactly 1 in 32-bit floats (for a 2 by 2 picture at size 10 some sum to a
/// step over it): an opaque picture still comes out with an alpha of 1 to within a rounding step, never above.
#[test]
fn an_opaque_picture_stays_opaque_at_a_size_whose_weights_do_not_sum_to_exactly_one() -> Outcome {
    let drawn = alone(varied(2, 2)?, 10)?;
    assert!(
        pixels(&drawn).all(|got| (1.0 - 1e-6..=1.0).contains(&got[3])),
        "an alpha is outside 1 - 1e-6 to 1"
    );
    Ok(())
}

proptest! {
    /// An opaque picture is opaque at any size, and every value is within 0 to 1.
    #[test]
    fn an_opaque_picture_is_opaque_and_in_range_at_any_size(width in 2_usize..8, height in 2_usize..8, size in 1_u32..60) {
        let drawn = alone(varied(width, height)?, size)?;
        for got in pixels(&drawn) {
            prop_assert!((1.0 - 1e-6..=1.0).contains(&got[3]), "alpha {}", got[3]);
            prop_assert!(got[..3].iter().all(|value| (0.0..=1.0).contains(value)), "{got:?}");
        }
    }

    /// A ramp across a picture of `width` pixels, on a canvas that is just that row, at any size: output pixel `i` of `size`
    /// looks at `(i + 0.5) * width / size - 0.5` among the pixel centres (an edge outside), where a linear ramp is its own
    /// position over `width - 1`.
    #[test]
    fn a_ramp_is_stretched_onto_exactly_the_output_at_any_size(width in 2_usize..40, size in 1_u32..120) {
        let last = u8::try_from(width - 1)?;
        let levels: Vec<f32> = (0..=last).flat_map(|x| [f32::from(x) / f32::from(last), 0.0, 0.0, 1.0]).collect();
        let drawn = scene(u32::try_from(width)?, 1, [Layer::picture(Raster::rgba(width, 1, levels)?)])?.render(size)?;
        prop_assert_eq!(drawn.width(), usize::try_from(size)?);
        let (width_f, size_f) = (f64::from(u32::try_from(width)?), f64::from(size));
        for x in 0..drawn.width() {
            let at = ((f64::from(u32::try_from(x)?) + 0.5) * width_f / size_f - 0.5).clamp(0.0, width_f - 1.0);
            let expected = at / (width_f - 1.0);
            let got = f64::from(pixel(&drawn, x, 0)[0]);
            prop_assert!((got - expected).abs() <= 1e-6, "column {x}: {got} is not {expected}");
        }
    }

    /// Half the size is the 2 by 2 box average of the full size: bilinear sampling at a 2x reduction lands on the middle of
    /// four pixels. Opaque pictures, because an average of straight colours is not that of premultiplied ones.
    #[test]
    fn half_the_size_is_the_box_average_of_the_whole_size(width in 1_usize..8, height in 1_usize..8, seed in 1_usize..200) {
        let (width, height) = (width * 2, height * 2);
        let levels: Vec<f32> = (0..width * height)
            .flat_map(|index| {
                let level = |step: usize| f32::from(u8::try_from(index * step % seed.max(2) % 256).unwrap_or(0)) / 255.0;
                [level(37), level(91), level(151), 1.0]
            })
            .collect();
        let whole = Raster::rgba(width, height, levels)?;
        let side = u32::try_from(width.max(height))?;
        let (columns, rows) = (u32::try_from(width)?, u32::try_from(height)?);
        let canvas = |picture: Raster| scene(columns, rows, [Layer::picture(picture)]);
        let full = canvas(whole.clone())?.render(side)?;
        prop_assert_eq!(full.pixels(), whole.pixels());
        let half = canvas(whole)?.render(side / 2)?;
        for y in 0..height / 2 {
            for x in 0..width / 2 {
                let mut sum = [0.0_f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let near = pixel(&full, 2 * x + dx, 2 * y + dy);
                    sum.iter_mut().zip(near).for_each(|(sum, value)| *sum += value / 4.0);
                }
                prop_assert!(close(pixel(&half, x, y), sum, 1e-5), "({x}, {y}): {:?} is not {sum:?}", pixel(&half, x, y));
            }
        }
    }

    /// The same picture at two sizes is the same picture in more or less detail: a smooth ramp, rendered at twice the size
    /// and averaged down, stays within a pixel's step of the render at the size itself.
    #[test]
    fn a_smooth_picture_at_twice_the_size_averages_back_to_itself(size in 4_u32..24) {
        let width = 16_usize;
        let ramp: Vec<f32> = (0..width * width).flat_map(|index| {
            let (x, y) = (index % width, index / width);
            [f32::from(u8::try_from(x * 15).unwrap_or(0)) / 255.0, f32::from(u8::try_from(y * 15).unwrap_or(0)) / 255.0, 0.0, 1.0]
        }).collect();
        let canvas = |picture: Raster| scene(16, 16, [Layer::picture(picture)]);
        let once = canvas(Raster::rgba(width, width, ramp.clone())?)?.render(size)?;
        let twice = canvas(Raster::rgba(width, width, ramp)?)?.render(size * 2)?;
        prop_assert_eq!(twice.width(), once.width() * 2);
        for y in 0..once.height() {
            for x in 0..once.width() {
                let mut sum = [0.0_f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let near = pixel(&twice, 2 * x + dx, 2 * y + dy);
                    sum.iter_mut().zip(near).for_each(|(sum, value)| *sum += value / 4.0);
                }
                // a ramp steps 15/255 a pixel: the two renders may differ by a step
                prop_assert!(close(pixel(&once, x, y), sum, 15.0 / 255.0), "({x}, {y}): {:?} is not {sum:?}", pixel(&once, x, y));
            }
        }
    }
}

#[test]
fn a_picture_alone_on_a_matching_canvas_at_its_size_is_exactly_opaque_where_it_is() -> Outcome {
    let drawn = alone(solid(3, 3, [0.25, 0.5, 0.75, 1.0])?, 3)?;
    assert!(all_exactly(&drawn, [0.25, 0.5, 0.75, 1.0]));
    Ok(())
}

/// A layer is drawn at its own size times the scale, whatever the canvas: 60 by 60 design pixels at 200 output pixels each is
/// 12,000 by 12,000, more than the 2^27 pixels a picture may have. It is refused naming the layer and the sizes, before the
/// picture is resampled.
#[test]
fn a_layer_whose_drawn_size_passes_2_pow_27_pixels_is_refused_naming_the_layer_and_its_size() -> Outcome {
    let layer = Layer::picture(solid(60, 60, [0.5, 0.5, 0.5, 1.0])?).named("photo")?;
    let refused = scene(6, 6, [layer])?.render(1200);
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message })
            if address.starts_with("photo") && message.contains("12000 x 12000") && message.contains("134217728")),
        "{refused:?}"
    );
    Ok(())
}

/// A picture is never printed whole: a failed assertion on a render must not build a diff of gigabytes.
#[test]
fn a_raster_shows_its_size_and_not_its_pixels_when_printed() -> Outcome {
    let shown = format!("{:?}", solid(300, 200, [0.5; 4])?);
    assert!(
        shown.contains("300") && shown.contains("200") && shown.len() < 100,
        "{shown}"
    );
    Ok(())
}

/// A file with three channels has no alpha: it comes out opaque, in the exact colour it holds (values that are whole
/// multiples of 1/255, which an 8-bit file keeps).
#[test]
fn a_picture_file_with_three_channels_is_opaque_and_keeps_its_exact_colour() -> Outcome {
    let source = varied(5, 3)?;
    let rgb: Vec<f32> = source
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[red, green, blue, _]| [red, green, blue])
        .collect();
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("photo.png");
    bullterrier_image::write(&path, &bullterrier_image::Image::new(3, 5, &rgb)?)?;
    let drawn = scene(5, 3, [Layer::picture_file(&path)])?.render(5)?;
    assert_eq!(drawn.pixels(), source.pixels());
    Ok(())
}
