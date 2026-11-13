//! The maps of a picture layer: given by name, checked against the picture when the scene renders, handed to the looks
//! that use them. What a mask does with a map is in `masks`; what image does with one is in `data_boundary`.

use bullterrier_scene::{Error, Layer, Map, Raster};

use crate::common::{Outcome, pixels, scene, solid, varied};

/// A map of `width` by `height` pixels holding `value` everywhere.
fn flat(width: usize, height: usize, value: f32) -> Result<Map, Error> {
    Map::new(width, height, vec![value; width * height])
}

#[test]
fn a_map_is_whole_pixels_of_one_value_from_0_to_1() {
    let rows: [(&str, usize, usize, Vec<f32>, bool); 7] = [
        ("one pixel", 1, 1, vec![0.5], true),
        ("a row", 3, 1, vec![0.0, 0.5, 1.0], true),
        ("no width", 0, 1, vec![], false),
        ("too few values", 2, 2, vec![0.0; 3], false),
        ("too many values", 1, 1, vec![0.0; 2], false),
        ("a value above 1", 1, 1, vec![1.5], false),
        ("a NaN", 1, 1, vec![f32::NAN], false),
    ];
    for (what, width, height, values, good) in rows {
        let made = Map::new(width, height, values);
        assert_eq!(made.is_ok(), good, "{what}: {made:?}");
        if let Err(error) = made {
            assert!(error.to_string().contains("values"), "{what}: {error}");
        }
    }
}

/// A map is given to a picture layer by a name of letters, digits and `_`, starting with a letter; any other layer has no
/// picture to give it to.
#[test]
fn a_map_needs_a_picture_layer_and_a_name_that_is_a_name() -> Outcome {
    let picture = || Layer::picture(solid(2, 2, [0.5, 0.5, 0.5, 1.0]).unwrap_or_else(|error| unreachable!("{error}")));
    let drawing = Layer::drawing(&crate::common::rectangle((4, 4), (0.0, 0.0), (4.0, 4.0), "#ff0000"))?;
    let rows = [
        ("a picture and a name", picture(), "subject", true),
        ("a name with digits and an underscore", picture(), "depth_2", true),
        ("a name that starts with a digit", picture(), "2deep", false),
        ("a name with a space", picture(), "a b", false),
        ("an empty name", picture(), "", false),
        ("a fill", Layer::fill("#ff0000")?, "subject", false),
        ("a drawing", drawing, "subject", false),
    ];
    for (what, layer, name, good) in rows {
        let given = layer.map(name, flat(2, 2, 1.0)?);
        assert_eq!(given.is_ok(), good, "{what}: {given:?}");
        if let Err(Error::Invalid { address, .. }) = given {
            assert_eq!(address, "map", "{what}");
        }
    }
    Ok(())
}

/// A map of another size than its picture is refused when the scene renders, naming the layer, the map and both sizes; the
/// same map at the picture's size is taken.
#[test]
fn a_map_of_another_size_than_its_picture_is_refused_at_render_naming_both_sizes() -> Outcome {
    for ((width, height), good) in [((4, 3), true), ((3, 4), false), ((4, 4), false), ((2, 1), false)] {
        let layer = Layer::picture(solid(4, 3, [0.2, 0.4, 0.6, 1.0])?)
            .map("matte", flat(width, height, 1.0)?)?
            .named("photo")?;
        let drawn = scene(4, 3, [layer])?.render(4);
        assert_eq!(drawn.is_ok(), good, "{width} x {height}: {drawn:?}");
        if let Err(Error::Invalid { address, message }) = drawn {
            assert_eq!(address, "photo.map.matte");
            assert!(
                message.contains(&format!("{width} x {height}")) && message.contains("4 x 3"),
                "{message}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_map_that_is_a_hole_is_refused_at_render_naming_the_layer_and_the_map() -> Outcome {
    let layer = Layer::picture(solid(2, 2, [0.5, 0.5, 0.5, 1.0])?)
        .map_hole("subject")?
        .named("photo")?;
    let refused = scene(2, 2, [layer])?.render(2);
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message }) if address == "photo.map.subject" && message.contains("hole")),
        "{refused:?}"
    );
    Ok(())
}

/// A map nothing reads changes nothing: the picture comes out bit for bit as without it.
#[test]
fn a_map_that_nothing_uses_does_not_change_the_picture() -> Outcome {
    let source = varied(6, 4)?;
    let plain = scene(6, 4, [Layer::picture(source.clone())])?.render(9)?;
    let mapped = scene(6, 4, [Layer::picture(source).map("matte", flat(6, 4, 0.25)?)?])?.render(9)?;
    assert_eq!(plain.pixels(), mapped.pixels());
    Ok(())
}

#[test]
fn a_map_file_that_cannot_be_read_is_a_read_error_at_render_naming_the_layer() -> Outcome {
    let folder = tempfile::tempdir()?;
    let layer = Layer::picture(solid(2, 2, [0.5, 0.5, 0.5, 1.0])?)
        .map_file("subject", folder.path().join("missing.png"))?
        .named("photo")?;
    let refused = scene(2, 2, [layer])?.render(2);
    assert!(
        matches!(&refused, Err(Error::Read { path, message }) if path.ends_with("missing.png") && message.contains("photo")),
        "{refused:?}"
    );
    Ok(())
}

/// The recipe of a `given` region `here` that `grade` is masked by: +1 stop only where the region says.
const GRADE_WHERE_GIVEN: &str = r#"{"steps": [{"effect": "grade", "params": {"exposure": 1.0}, "mask": "here"}], "regions": {"here": {"kind": "given"}}}"#;

/// A look whose region is given the picture's map `here` works where the map is 1 and not where it is 0, and half way where
/// it is a half: the picture's own value `c` becomes `(1 - m) c + m min(2c, 1)` (the +1 stop doubles and clips, `grade`'s
/// documented rule, laid in by the mask). The map is a name of the picture, not of the recipe, so a map a recipe does not
/// know (`matte`) is held and never given to image.
#[test]
fn a_look_works_where_the_given_map_is_one_and_leaves_the_rest() -> Outcome {
    let levels = [0.1_f32, 0.25, 0.4, 0.6];
    let weights = [0.0_f32, 0.25, 0.5, 1.0];
    let picture: Vec<f32> = levels.iter().flat_map(|&level| [level, level, level, 1.0]).collect();
    let layer = Layer::picture(Raster::rgba(4, 1, picture)?)
        .map("here", Map::new(4, 1, weights.to_vec())?)?
        .map("matte", flat(4, 1, 1.0)?)?
        .look(GRADE_WHERE_GIVEN)?;
    let drawn = scene(4, 1, [layer])?.render(4)?;
    for (x, (&level, &weight)) in levels.iter().zip(&weights).enumerate() {
        let expected = (1.0 - weight) * level + weight * (2.0 * level).min(1.0);
        let got = pixels(&drawn).nth(x).unwrap_or([f32::NAN; 4]);
        assert!(
            (got[0] - expected).abs() <= 1e-6 && (got[3] - 1.0).abs() == 0.0,
            "column {x}: {got:?} is not {expected}"
        );
    }
    Ok(())
}

/// The map a look is given is the map of the pixels the look sees: a picture drawn at twice its size gives the look a map
/// of twice the size, resampled as the picture is, so a map that is 1 on the left half and 0 on the right is still so.
#[test]
fn the_map_a_look_is_given_follows_the_picture_to_the_size_it_is_drawn_at() -> Outcome {
    let weights: Vec<f32> = (0..8).map(|x| if x < 4 { 1.0 } else { 0.0 }).collect();
    let picture = Raster::rgba(8, 1, [0.25_f32, 0.25, 0.25, 1.0].repeat(8))?;
    let layer = Layer::picture(picture)
        .map("here", Map::new(8, 1, weights)?)?
        .look(GRADE_WHERE_GIVEN)?;
    let drawn = scene(8, 1, [layer])?.render(16)?;
    // a canvas of 8 by 1 at 16 is 16 columns of 2 rows: the first row is all of it
    let row: Vec<f32> = pixels(&drawn).take(drawn.width()).map(|pixel| pixel[0]).collect();
    assert_eq!(row.len(), 16);
    // the picture is 0.25 everywhere, so a pixel is 0.25 + 0.25 m (the +1 stop doubles it, laid in by the weight m); the map
    // doubled is bilinear: output column i reads the map at (i + 0.5) / 2 - 0.5, so m is 1 up to column 6, 0.75 at column 7,
    // 0.25 at column 8 and 0 from column 9 on
    for (x, red) in row.iter().enumerate() {
        let weight = match x {
            0..7 => 1.0,
            7 => 0.75,
            8 => 0.25,
            _ => 0.0,
        };
        assert!((red - (0.25 + 0.25 * weight)).abs() <= 1e-6, "column {x}: {red}");
    }
    Ok(())
}
