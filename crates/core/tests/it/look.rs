//! An image look on a picture layer: checked when it is given, run on the picture at the output size. Expected values come
//! from the documented rule of the effect (exposure +1 stop doubles a value), never from the code's own arithmetic. The
//! equality with image's own render is in `data_boundary`.

use bullterrier_image::{Options, Photo, Presets, RECIPE_FORMATS, Recipe};
use bullterrier_scene::{Deg, Error, Fit, Layer, Place, Raster, Scene, Text};

use crate::common::{Outcome, close, made, pixel, pixels, rectangle, scene, solid, varied};

/// `Recipe([fx.grade(exposure=1.0)])`, as the pinned package writes it.
const PLUS_ONE_STOP: &str =
    r#"{"steps": [{"effect": "grade", "params": {"exposure": 1.0}, "opacity": 1.0, "blend_mode": "normal"}]}"#;

/// A recipe of one `grade` step with every parameter at its default: it changes nothing.
const GRADE_AT_DEFAULTS: &str = r#"{"steps": [{"effect": "grade"}]}"#;

fn looked(picture: Raster, recipe: &str) -> Result<Layer, Error> {
    Layer::picture(picture).look(recipe)
}

/// Exposure +1 stop doubles every value, and clips what passes 1 (grade's documented rule): 0.25 is 0.5, 0.4 is 0.8, 0.6 is
/// 1. Alpha is the picture's.
#[test]
fn a_look_of_plus_one_stop_doubles_the_values_of_the_picture_and_keeps_its_alpha() -> Outcome {
    let row = Raster::rgba(
        3,
        1,
        vec![0.25, 0.25, 0.25, 1.0, 0.4, 0.4, 0.4, 0.5, 0.6, 0.6, 0.6, 1.0],
    )?;
    let drawn = scene(3, 1, [looked(row, PLUS_ONE_STOP)?])?.render(3)?;
    for (x, expected) in [[0.5, 0.5, 0.5, 1.0], [0.8, 0.8, 0.8, 0.5], [1.0, 1.0, 1.0, 1.0]]
        .into_iter()
        .enumerate()
    {
        assert!(
            close(pixel(&drawn, x, 0), expected, 1e-6),
            "{x}: {:?}",
            pixel(&drawn, x, 0)
        );
    }
    Ok(())
}

#[test]
fn a_look_that_changes_nothing_leaves_the_picture_as_it_was() -> Outcome {
    let source = varied(8, 5)?;
    let drawn = scene(8, 5, [looked(source.clone(), GRADE_AT_DEFAULTS)?])?.render(8)?;
    for (got, expected) in pixels(&drawn).zip(pixels(&source)) {
        assert!(close(got, expected, 1e-6), "{got:?} is not {expected:?}");
    }
    Ok(())
}

/// Looks run in the order given: +1 stop then a flat contrast of 0 (everything at the pivot 0.5) is 0.5; the other way round
/// the contrast flattens first and the stop doubles 0.5 to 1.
#[test]
fn looks_run_in_the_order_they_are_given() -> Outcome {
    let flat = r#"{"steps": [{"effect": "grade", "params": {"contrast": 0.0}}]}"#;
    let picture = solid(2, 2, [0.25, 0.25, 0.25, 1.0])?;
    let stop_then_flat = scene(2, 2, [looked(picture.clone(), PLUS_ONE_STOP)?.look(flat)?])?.render(2)?;
    let flat_then_stop = scene(2, 2, [looked(picture, flat)?.look(PLUS_ONE_STOP)?])?.render(2)?;
    assert!(pixels(&stop_then_flat).all(|got| close(got, [0.5, 0.5, 0.5, 1.0], 1e-6)));
    assert!(pixels(&flat_then_stop).all(|got| close(got, [1.0, 1.0, 1.0, 1.0], 1e-6)));
    Ok(())
}

#[test]
fn a_layer_is_not_changed_by_giving_a_copy_of_it_a_look() -> Outcome {
    let plain = Layer::picture(solid(2, 2, [0.25, 0.25, 0.25, 1.0])?);
    let graded = plain.clone().look(PLUS_ONE_STOP)?;
    let before = scene(2, 2, [plain.clone()])?.render(2)?;
    assert!(pixels(&before).all(|got| close(got, [0.25, 0.25, 0.25, 1.0], 1e-6)));
    assert!(pixels(&scene(2, 2, [graded])?.render(2)?).all(|got| close(got, [0.5, 0.5, 0.5, 1.0], 1e-6)));
    assert!(pixels(&scene(2, 2, [plain])?.render(2)?).all(|got| close(got, [0.25, 0.25, 0.25, 1.0], 1e-6)));
    Ok(())
}

/// Each row: a look, and the words its refusal must carry. The refusal is at the call, by image's own message.
#[test]
fn a_look_image_refuses_is_refused_at_the_call_naming_the_look_and_the_reason() {
    let rows = [
        (
            "an unknown effect",
            r#"{"steps": [{"effect": "glitterbomb"}]}"#,
            "glitterbomb",
        ),
        (
            "a value out of range",
            r#"{"steps": [{"effect": "grade", "params": {"exposure": 99}}]}"#,
            "exposure",
        ),
        ("an unknown key", r#"{"steps": [], "colour": 1}"#, "colour"),
        (
            "a size of its own",
            r#"{"steps": [{"effect": "grade"}], "size": 100}"#,
            "size",
        ),
        (
            "a frame of its own",
            r#"{"steps": [{"effect": "grade"}], "frame": {"aspect": 1.0}}"#,
            "frame",
        ),
        (
            "a region that is not declared",
            r#"{"steps": [{"effect": "grade", "mask": "nowhere"}]}"#,
            "nowhere",
        ),
    ];
    for (what, recipe, named) in rows {
        let refused =
            Layer::picture(solid(1, 1, [0.0; 4]).unwrap_or_else(|error| unreachable!("{error}"))).look(recipe);
        assert!(
            matches!(&refused, Err(Error::Invalid { address, message }) if address == "look" && message.contains(named)),
            "{what}: {refused:?}"
        );
    }
}

#[test]
fn known_good_looks_are_taken() -> Outcome {
    let picture = solid(1, 1, [0.0, 0.0, 0.0, 1.0])?;
    for recipe in [
        PLUS_ONE_STOP,
        GRADE_AT_DEFAULTS,
        r#"{"steps": [{"effect": "gradient_map", "preset": "sepia"}]}"#,
        r#"{"steps": [{"effect": "grade"}], "frame": {}}"#,
    ] {
        assert!(Layer::picture(picture.clone()).look(recipe).is_ok(), "{recipe}");
    }
    Ok(())
}

#[test]
fn text_that_is_not_a_json_object_is_a_format_error_under_look() -> Outcome {
    let picture = solid(1, 1, [0.0; 4])?;
    for text in ["", "{", "not json", "[1]", "3"] {
        let refused = Layer::picture(picture.clone()).look(text);
        assert!(
            matches!(&refused, Err(Error::Format { pointer, .. }) if pointer == "/look"),
            "{text:?}: {refused:?}"
        );
    }
    Ok(())
}

/// A `format` is a version of a recipe, of which image's reader reads those it publishes (`RECIPE_FORMATS`). A recipe written with one is taken if the
/// linked crate reads it, and refused naming the version found and the versions read if not.
#[test]
fn a_format_the_linked_image_crate_reads_is_taken_and_one_it_does_not_is_refused_naming_both() -> Outcome {
    let picture = solid(2, 2, [0.25, 0.25, 0.25, 1.0])?;
    for format in RECIPE_FORMATS {
        let recipe =
            format!(r#"{{"format": {format}, "steps": [{{"effect": "grade", "params": {{"exposure": 1.0}}}}]}}"#);
        let drawn = scene(2, 2, [Layer::picture(picture.clone()).look(&recipe)?])?.render(2)?;
        assert!(
            pixels(&drawn).all(|got| close(got, [0.5, 0.5, 0.5, 1.0], 1e-6)),
            "format {format}"
        );
    }
    for bad in ["4", "0", "3.0", "\"3\"", "true", "null"] {
        let recipe = format!(r#"{{"format": {bad}, "steps": [{{"effect": "grade"}}]}}"#);
        let refused = Layer::picture(picture.clone()).look(&recipe);
        assert!(
            matches!(&refused, Err(Error::Format { pointer, message })
                if pointer == "/look/format" && message.contains(bad) && message.contains("1, 2, 3")),
            "format {bad}: {refused:?}"
        );
    }
    Ok(())
}

/// A look that needs a map scene cannot give (a `subject`): the layer draws, and image's refusal comes from `render`
/// naming the layer.
#[test]
fn a_look_that_needs_a_map_fails_at_render_naming_the_layer_the_step_and_the_map() -> Outcome {
    let masked = r#"{"steps": [{"effect": "grade", "params": {"exposure": 1.0}, "mask": "subject"}]}"#;
    let canvas = scene(
        4,
        4,
        [looked(solid(4, 4, [0.2, 0.2, 0.2, 1.0])?, masked)?.named("photo")?],
    )?;
    let refused = canvas.render(4);
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message }) if address.starts_with("photo") && message.contains("subject")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_recipe_file_is_read_when_given_and_gives_the_look_it_holds() -> Outcome {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("stop.toml");
    std::fs::write(&path, "[[step]]\neffect = \"grade\"\nexposure = 1\n")?;
    let drawn = scene(
        2,
        2,
        [Layer::picture(solid(2, 2, [0.25, 0.25, 0.25, 1.0])?).look_file(&path)?],
    )?
    .render(2)?;
    assert!(pixels(&drawn).all(|got| close(got, [0.5, 0.5, 0.5, 1.0], 1e-6)));
    Ok(())
}

#[test]
fn a_recipe_file_that_cannot_be_read_or_is_no_recipe_is_refused_at_the_call() -> Outcome {
    let folder = tempfile::tempdir()?;
    let bad = folder.path().join("bad.toml");
    std::fs::write(&bad, "[[step]]\neffect = \"glitterbomb\"\n")?;
    let layer = Layer::picture(solid(1, 1, [0.0; 4])?);
    let missing = layer.clone().look_file(folder.path().join("missing.toml"));
    assert!(
        matches!(&missing, Err(Error::Read { path, .. }) if path.ends_with("missing.toml")),
        "{missing:?}"
    );
    let wrong = layer.look_file(&bad);
    assert!(
        matches!(&wrong, Err(Error::Invalid { address, message }) if address == "look" && message.contains("glitterbomb")),
        "{wrong:?}"
    );
    Ok(())
}

/// Image promises its pictures are "nominally 0..1": a scanlines look with a spacing and an offset far past the picture
/// gives values of about -1e-15 on a checker. Scene's output is a picture in 0 to 1 all the same, one that a `Raster` takes
/// back.
#[test]
fn a_look_that_overshoots_image_s_nominal_range_by_rounding_still_gives_a_picture_in_range() -> Outcome {
    let scanlines = r#"{"steps": [{"effect": "scanlines", "params": {"spacing": 2000, "thickness": 1, "darkness": 1, "offset": 2000, "softness": 1}, "opacity": 1.0, "blend_mode": "normal"}]}"#;
    let checker: Vec<f32> = (0..24)
        .flat_map(|row| {
            (0..32).flat_map(move |column| {
                if (row + column) % 2 == 0 {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    [0.0, 0.0, 0.0, 1.0]
                }
            })
        })
        .collect();
    let drawn = scene(
        32,
        24,
        [Layer::picture(Raster::rgba(32, 24, checker)?).look(scanlines)?],
    )?
    .render(32)?;
    assert!(
        drawn.pixels().iter().all(|value| (0.0..=1.0).contains(value)),
        "a value is outside 0 to 1"
    );
    assert!(Raster::rgba(32, 24, drawn.into_pixels()).is_ok());
    Ok(())
}

// --- a look on every kind of layer ----------------------------------------------------------------------------------

/// A blur wide enough to act on a picture of 40 pixels: a `px` is a design pixel of the canvas, whose long side is 480 of
/// them, so 48 of them are 4 pixels of a canvas drawn at 40.
const BLUR_4_PIXELS_AT_40: &str = r#"{"steps": [{"effect": "gaussian_blur", "params": {"radius": 48.0}}]}"#;

/// Dark lines every 2 pixels at 40: a look that depends on where a pixel is, which a flat colour cannot show unless the
/// look runs on a raster of the size the layer has.
const SCANLINES_2_PIXELS_AT_40: &str =
    r#"{"steps": [{"effect": "scanlines", "params": {"spacing": 24, "thickness": 0.5, "darkness": 0.8}}]}"#;

const SEPIA: &str = r#"{"steps": [{"effect": "gradient_map", "preset": "sepia"}]}"#;

/// The same blur at half that: 2 pixels at 40.
const BLUR_2_PIXELS_AT_40: &str = r#"{"steps": [{"effect": "gaussian_blur", "params": {"radius": 24.0}}]}"#;

/// What image's own render of `recipe` makes of the colour of `drawn` (straight, its alpha set aside), with the alpha of
/// `drawn` put back: what a look on a layer that drew `drawn` must give.
fn imaged(drawn: &Raster, recipe: &str) -> Result<Raster, Box<dyn std::error::Error>> {
    imaged_on(drawn, recipe, None)
}

/// [`imaged`] on a canvas of `canvas` output pixels (width, height) that `drawn` lies on, or on itself for `None`.
fn imaged_on(drawn: &Raster, recipe: &str, canvas: Option<(u32, u32)>) -> Result<Raster, Box<dyn std::error::Error>> {
    let colour: Vec<f32> = pixels(drawn)
        .flat_map(|[red, green, blue, _]| [red, green, blue])
        .collect();
    let presets = Presets::shipped()?;
    let recipe = Recipe::from_json(recipe)?.checked(&presets)?;
    let photo = Photo::new(bullterrier_image::Picture::new(drawn.height(), drawn.width(), &colour)?);
    let options = canvas.map_or_else(Options::default, |(width, height)| {
        Options::default().with_canvas(width, height)
    });
    let image = photo.render(&recipe, &presets, options)?;
    let looked = image.into_values();
    let values = looked
        .as_chunks::<3>()
        .0
        .iter()
        .zip(pixels(drawn))
        .flat_map(|(&[red, green, blue], [_, _, _, alpha])| [red, green, blue, alpha])
        .collect();
    Ok(Raster::rgba(drawn.width(), drawn.height(), values)?)
}

/// The largest difference of one premultiplied channel (alpha included) of two pictures of one size: the colour of a
/// transparent pixel is nothing, and a look may leave any in it.
fn most(left: &Raster, right: &Raster) -> f32 {
    assert_eq!((left.width(), left.height()), (right.width(), right.height()));
    let premultiplied = |[red, green, blue, alpha]: [f32; 4]| [red * alpha, green * alpha, blue * alpha, alpha];
    pixels(left)
        .zip(pixels(right))
        .flat_map(|(l, r)| {
            premultiplied(l)
                .into_iter()
                .zip(premultiplied(r))
                .map(|(l, r)| (l - r).abs())
        })
        .fold(0.0, f32::max)
}

/// A badge of two layers on 40 by 30: a ramp picture with a translucent fill band over it.
fn inner_scene() -> Result<Scene, Error> {
    let ramp = made(40, 30, |x, y| {
        [
            f32::from(u8::try_from(x).unwrap_or(0)) / 39.0,
            f32::from(u8::try_from(y).unwrap_or(0)) / 29.0,
            0.5,
            1.0,
        ]
    })?;
    scene(
        40,
        30,
        [
            Layer::picture(ramp),
            Layer::fill("#ff880080")?.place(Place::new().dx(6.0))?,
        ],
    )
}

/// Layers of every other kind, each as large as a canvas of 40 by 30 (a text excepted: its box is its glyphs'), with a
/// translucent part.
fn kinds() -> Result<Vec<(&'static str, Layer)>, Error> {
    let text = Layer::text(
        "Hi",
        &Text::new("bricolage_grotesque").option("size", 20.0).color("#336699"),
    )?
    .place(Place::new().dx(4.0).dy(24.0))?;
    Ok(vec![
        ("a fill", Layer::fill("#33669980")?),
        (
            "a drawing",
            Layer::drawing(&rectangle((40, 30), (5.0, 4.0), (33.0, 22.0), "#cc3355"))?,
        ),
        ("a text", text),
        ("a group", Layer::scene(inner_scene()?)),
    ])
}

/// A look runs on what the layer drew, whatever drew it: the result is image's own render of that picture's colour, with
/// the picture's alpha. A pointwise look (a stop) and a look that reads the neighbours (a blur) are asked of each.
#[test]
fn a_look_on_a_fill_a_drawing_a_text_or_a_group_is_images_render_of_what_it_drew_with_its_alpha() -> Outcome {
    for (what, plain) in kinds()? {
        let drawn = scene(40, 30, [plain.clone()])?.render(40)?;
        // a text's box is its glyphs', not the canvas: only a look that sees one pixel at a time is the same on both
        // (a blur of a flat fill is the fill: the looks that change it are a gradient map and one that depends on the place)
        let recipes: &[&str] = match what {
            "a text" => &[PLUS_ONE_STOP],
            "a fill" => &[PLUS_ONE_STOP, SEPIA, SCANLINES_2_PIXELS_AT_40],
            _ => &[PLUS_ONE_STOP, BLUR_4_PIXELS_AT_40],
        };
        for recipe in recipes {
            let got = scene(40, 30, [plain.clone().look(recipe)?])?.render(40)?;
            let expected = imaged(&drawn, recipe)?;
            assert!(
                most(&got, &expected) < 1e-5,
                "{what}, {recipe}: off by {}",
                most(&got, &expected)
            );
            // the alpha is the layer's own, and a look that changes colour does change it
            assert!(
                got.pixels()
                    .chunks(4)
                    .zip(drawn.pixels().chunks(4))
                    .all(|(l, r)| (l[3] - r[3]).abs() < 1e-6),
                "{what}"
            );
            assert!(most(&got, &drawn) > 1e-3, "{what}, {recipe}: the look changed nothing");
        }
    }
    Ok(())
}

/// A look that changes nothing leaves a fill, a drawing, a text and a group as they were.
#[test]
fn a_look_that_changes_nothing_leaves_every_kind_of_layer_as_it_was() -> Outcome {
    for (what, plain) in kinds()? {
        let before = scene(40, 30, [plain.clone()])?.render(40)?;
        let after = scene(40, 30, [plain.look(GRADE_AT_DEFAULTS)?])?.render(40)?;
        assert!(most(&before, &after) < 1e-6, "{what}: {}", most(&before, &after));
    }
    Ok(())
}

/// A fill of `#40404080` is the colour 64/255 at alpha 128/255: +1 stop doubles the colour and the alpha stays.
#[test]
fn a_look_on_a_half_transparent_fill_doubles_its_colour_and_keeps_its_alpha() -> Outcome {
    let drawn = scene(2, 2, [Layer::fill("#40404080")?.look(PLUS_ONE_STOP)?])?.render(2)?;
    let (level, alpha) = (128.0 / 255.0, 128.0 / 255.0);
    assert!(
        pixels(&drawn).all(|got| close(got, [level, level, level, alpha], 1e-6)),
        "{:?}",
        pixel(&drawn, 0, 0)
    );
    Ok(())
}

/// The corner a drawing leaves empty is empty after a look, whatever the look makes of black.
#[test]
fn a_look_on_a_drawing_leaves_what_it_does_not_cover_transparent() -> Outcome {
    let drawing = Layer::drawing(&rectangle((8, 8), (2.0, 2.0), (6.0, 6.0), "#ff0000"))?;
    let drawn = scene(8, 8, [drawing.look(SEPIA)?])?.render(8)?;
    assert_eq!(pixel(&drawn, 0, 0)[3], 0.0);
    assert_eq!(pixel(&drawn, 7, 7)[3], 0.0);
    assert_eq!(pixel(&drawn, 4, 4)[3], 1.0);
    Ok(())
}

/// A group's look sees the group's finished picture: the same as a picture of the inner scene's render given the look, at
/// the canvas size and at twice it (the group is drawn at the density it has).
#[test]
fn a_look_on_a_group_equals_the_look_on_a_picture_of_the_inner_scene_rendered_at_that_size() -> Outcome {
    for size in [40_u32, 80] {
        let inner = inner_scene()?;
        let group = scene(40, 30, [Layer::scene(inner.clone()).look(BLUR_2_PIXELS_AT_40)?])?.render(size)?;
        let finished = inner.render(size)?;
        let picture = Layer::picture(finished.clone()).look(BLUR_2_PIXELS_AT_40)?;
        let expected = scene(
            u32::try_from(finished.width())?,
            u32::try_from(finished.height())?,
            [picture],
        )?
        .render(size)?;
        assert!(
            most(&group, &expected) < 1e-5,
            "size {size}: {}",
            most(&group, &expected)
        );
    }
    Ok(())
}

/// A look runs at the size the layer is drawn at, so a drawing at `scale` 2 is looked at twice its size, as a picture of
/// that drawing is: the blur is image's own on the raster the layer has, on the canvas at that scale (160 by 120 pixels:
/// the 80 by 60 canvas drawn at 80, doubled), not one run small and enlarged.
#[test]
fn a_look_on_a_drawing_runs_at_the_size_it_is_drawn_at() -> Outcome {
    let drawing = || Layer::drawing(&rectangle((40, 30), (5.0, 4.0), (33.0, 22.0), "#cc3355"));
    let big = |layer: Layer| layer.place(Place::new().scale(2.0).anchor(0.0, 0.0));
    let plain = scene(80, 60, [big(drawing()?)?])?.render(80)?;
    let got = scene(80, 60, [big(drawing()?.look(BLUR_4_PIXELS_AT_40)?)?])?.render(80)?;
    let expected = imaged_on(&plain, BLUR_4_PIXELS_AT_40, Some((160, 120)))?;
    assert!(most(&got, &expected) < 1e-5, "{}", most(&got, &expected));
    // the blur reaches far enough to be seen: not a no-op
    assert!(most(&got, &plain) > 1e-2);
    Ok(())
}

/// Look then place: a drawing with a look, turned, equals a picture of the drawing's own raster with the same look
/// and place, since both run the look on the raster and put it down by one warp.
#[test]
fn a_look_on_a_drawing_then_turned_equals_the_same_on_a_picture_of_its_raster() -> Outcome {
    let drawing = Layer::drawing(&rectangle((40, 30), (5.0, 4.0), (33.0, 22.0), "#cc3355"))?;
    let raster = scene(40, 30, [drawing.clone()])?.render(40)?;
    let place = Place::new().rotate(Deg(17.0));
    let got = scene(40, 30, [drawing.look(BLUR_2_PIXELS_AT_40)?.place(place)?])?.render(40)?;
    let expected = scene(
        40,
        30,
        [Layer::picture(raster).look(BLUR_2_PIXELS_AT_40)?.place(place)?],
    )?
    .render(40)?;
    assert!(most(&got, &expected) < 1e-5, "{}", most(&got, &expected));
    Ok(())
}

/// A look on a text through `fit` and `place` runs on the glyphs' own raster at the size they are drawn at: a pointwise
/// look commutes with where it is put, up to the resampling at the edges of the box.
#[test]
fn a_look_on_a_text_fitted_and_turned_changes_its_colour_where_it_is_and_keeps_its_alpha() -> Outcome {
    let text = || {
        Layer::text(
            "Hi",
            &Text::new("bricolage_grotesque").option("size", 60.0).color("#336699"),
        )
    };
    let place = |layer: Layer| -> Result<Layer, Error> {
        layer
            .fit([5.0, 5.0, 50.0, 30.0], Fit::Contain, [0.5, 0.5])?
            .place(Place::new().rotate(Deg(17.0)))
    };
    let plain = scene(60, 40, [place(text()?)?])?.render(120)?;
    let looked = scene(60, 40, [place(text()?.look(PLUS_ONE_STOP)?)?])?.render(120)?;
    // grade doubles a colour (clipped), and a straight colour is not changed by the alpha it is placed at, so where the text
    // is fully drawn the colour is that of image's render of the text's colour
    let doubled = |level: f32| (2.0 * level).min(1.0);
    let covered: Vec<_> = pixels(&plain)
        .zip(pixels(&looked))
        .filter(|(p, _)| p[3] > 0.999)
        .collect();
    assert_ne!(covered.len(), 0);
    for (before, after) in covered {
        assert!(
            close(
                after,
                [doubled(before[0]), doubled(before[1]), doubled(before[2]), before[3]],
                1e-5
            ),
            "{before:?} {after:?}"
        );
    }
    assert!(
        plain
            .pixels()
            .chunks(4)
            .zip(looked.pixels().chunks(4))
            .all(|(p, l)| (p[3] - l[3]).abs() < 1e-5)
    );
    Ok(())
}

/// Each row: a look and the words its refusal carries. Every kind of layer refuses what a picture does, at the call.
#[test]
fn every_kind_of_layer_refuses_a_recipe_image_refuses_and_takes_one_it_takes() -> Outcome {
    for (what, layer) in kinds()? {
        for (rule, recipe, named) in [
            (
                "an unknown effect",
                r#"{"steps": [{"effect": "glitterbomb"}]}"#,
                "glitterbomb",
            ),
            (
                "a size of its own",
                r#"{"steps": [{"effect": "grade"}], "size": 100}"#,
                "size",
            ),
            (
                "a frame of its own",
                r#"{"steps": [{"effect": "grade"}], "frame": {"aspect": 1.0}}"#,
                "frame",
            ),
        ] {
            let refused = layer.clone().look(recipe);
            assert!(
                matches!(&refused, Err(Error::Invalid { address, message }) if address == "look" && message.contains(named)),
                "{what}, {rule}: {refused:?}"
            );
        }
        assert!(layer.clone().look(PLUS_ONE_STOP).is_ok(), "{what}");
    }
    Ok(())
}

/// Only a picture has maps: a look that needs one on any other layer fails at render, naming the layer, the step and the map.
#[test]
fn a_look_that_needs_a_map_fails_on_every_other_kind_of_layer_at_render_naming_it() -> Outcome {
    let masked = r#"{"steps": [{"effect": "grade", "params": {"exposure": 1.0}, "mask": "subject"}]}"#;
    for (what, layer) in kinds()? {
        let refused = scene(40, 30, [layer.look(masked)?.named("thing")?])?.render(40);
        assert!(
            matches!(&refused, Err(Error::Invalid { address, message }) if address.starts_with("thing") && message.contains("subject")),
            "{what}: {refused:?}"
        );
    }
    Ok(())
}

/// A reference to a fill inherits the fill's looks and adds its own after them, as a picture's does.
#[test]
fn a_reference_adds_its_looks_after_those_of_the_fill_it_names() -> Outcome {
    let flat = r#"{"steps": [{"effect": "grade", "params": {"contrast": 0.0}}]}"#;
    let named = Layer::fill("#404040")?.look(PLUS_ONE_STOP)?.named("a")?;
    let both = scene(2, 2, [named.clone(), Layer::of("a").look(flat)?])?.render(2)?;
    let expected = scene(2, 2, [Layer::fill("#404040")?.look(PLUS_ONE_STOP)?.look(flat)?])?.render(2)?;
    assert_eq!(both.pixels(), expected.pixels());
    // the stop alone would give 0.5019; the flat contrast after it is what makes 0.5
    assert!(pixels(&both).all(|got| close(got, [0.5, 0.5, 0.5, 1.0], 1e-6)));
    Ok(())
}

// --- a look's `px` are design pixels of the canvas -------------------------------------------------------------------

/// The part of `raster` from column `x`, row `y`, `width` by `height`.
fn crop(raster: &Raster, (x, y): (usize, usize), (width, height): (usize, usize)) -> Result<Raster, Error> {
    made(width, height, |column, row| pixel(raster, x + column, y + row))
}

/// How many pixels of `expected` differ from those of `whole` from its corner `at`, in any premultiplied channel, by more than
/// a step (the colour of a transparent pixel is nothing).
fn differing(whole: &Raster, at: (usize, usize), expected: &Raster) -> usize {
    let premultiplied = |[red, green, blue, alpha]: [f32; 4]| [red * alpha, green * alpha, blue * alpha, alpha];
    (0..expected.height())
        .flat_map(|y| (0..expected.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let (got, want) = (pixel(whole, at.0 + x, at.1 + y), pixel(expected, x, y));
            !close(premultiplied(got), premultiplied(want), 1e-6)
        })
        .count()
}

/// A look's `px` are the same number of output pixels on every layer of one canvas: a small inset and a full-bleed picture
/// given one blur are each image's own render of their picture on that canvas (the oracle, through the pinned crate), and
/// the inset's differs from image's own convention (its picture alone), which is what this replaces.
#[test]
fn a_looks_px_are_the_same_number_of_output_pixels_on_a_small_inset_and_a_full_bleed_layer() -> Outcome {
    let canvas = (40, 30);
    for (what, (width, height), (dx, dy)) in [("inset", (10, 8), (12, 9)), ("full bleed", (40, 30), (0, 0))] {
        let picture = varied(width, height)?;
        let place = Place::new()
            .dx(f64::from(u32::try_from(dx)?))
            .dy(f64::from(u32::try_from(dy)?));
        let drawn = scene(40, 30, [looked(picture.clone(), BLUR_4_PIXELS_AT_40)?.place(place)?])?.render(40)?;
        let on_canvas = imaged_on(&picture, BLUR_4_PIXELS_AT_40, Some(canvas))?;
        assert_eq!(
            differing(&drawn, (dx, dy), &on_canvas),
            0,
            "{what} is not image's render on the canvas"
        );
        let alone = imaged(&picture, BLUR_4_PIXELS_AT_40)?;
        assert_eq!(
            differing(&drawn, (dx, dy), &alone) == 0,
            (width, height) == (40, 30),
            "{what}: image's own convention"
        );
    }
    Ok(())
}

/// `place.scale` 2 draws the layer at twice the size and the canvas it is looked on at twice the size: the blur is twice as
/// many output pixels, as image's render of the doubled raster on the doubled canvas is.
#[test]
fn a_layer_placed_at_scale_two_has_the_look_of_the_doubled_canvas() -> Outcome {
    let picture = varied(10, 8)?;
    let place = Place::new().anchor(0.0, 0.0).scale(2.0).dx(4.0).dy(2.0);
    let plain = scene(40, 30, [Layer::picture(picture.clone()).place(place)?])?.render(40)?;
    let doubled = crop(&plain, (4, 2), (20, 16))?;
    let drawn = scene(40, 30, [looked(picture, BLUR_4_PIXELS_AT_40)?.place(place)?])?.render(40)?;
    let on_doubled = imaged_on(&doubled, BLUR_4_PIXELS_AT_40, Some((80, 60)))?;
    assert_eq!(
        differing(&drawn, (4, 2), &on_doubled),
        0,
        "image's render on a canvas of 80 by 60"
    );
    let on_canvas = imaged_on(&doubled, BLUR_4_PIXELS_AT_40, Some((40, 30)))?;
    assert!(differing(&drawn, (4, 2), &on_canvas) > 0, "the scale changed nothing");
    Ok(())
}

/// A layer larger than the canvas (a drawing's frame is drawn whole) is looked on the canvas's scale: image's render of the
/// whole frame on the canvas, cut to the canvas.
#[test]
fn a_layer_larger_than_the_canvas_is_looked_at_the_canvas_scale() -> Outcome {
    let frame = || Layer::drawing(&rectangle((100, 80), (10.0, 10.0), (90.0, 70.0), "#cc3355"));
    let whole = scene(100, 80, [frame()?])?.render(100)?;
    let on_canvas = imaged_on(&whole, BLUR_4_PIXELS_AT_40, Some((40, 30)))?;
    let drawn = scene(40, 30, [frame()?.look(BLUR_4_PIXELS_AT_40)?])?.render(40)?;
    assert_eq!(differing(&drawn, (0, 0), &crop(&on_canvas, (0, 0), (40, 30))?), 0);
    Ok(())
}

/// The looks of an inner scene's layers run on the inner canvas at the density the group is drawn at: a group at any size is
/// its scene rendered at that size, whether it is the whole of the outer canvas or enlarged to it.
#[test]
fn the_looks_inside_a_group_use_the_group_s_density() -> Outcome {
    let inner = scene(20, 15, [looked(varied(20, 15)?, BLUR_4_PIXELS_AT_40)?])?;
    for size in [20_u32, 40, 60] {
        let group = scene(20, 15, [Layer::scene(inner.clone())])?.render(size)?;
        assert_eq!(differing(&group, (0, 0), &inner.render(size)?), 0, "at size {size}");
    }
    let enlarged = scene(
        40,
        30,
        [Layer::scene(inner.clone()).place(Place::new().anchor(0.0, 0.0).scale(2.0))?],
    )?
    .render(40)?;
    assert_eq!(
        differing(&enlarged, (0, 0), &inner.render(40)?),
        0,
        "the group enlarged by place"
    );
    Ok(())
}
