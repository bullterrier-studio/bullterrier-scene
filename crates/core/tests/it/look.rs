//! An image look on a picture layer: checked when it is given, run on the picture at the output size. Expected values come
//! from the documented rule of the effect (exposure +1 stop doubles a value), never from the code's own arithmetic. The
//! equality with image's own render is in `data_boundary`.

use bullterrier_scene::{Error, Layer, Raster};

use crate::common::{Outcome, close, pixel, pixels, scene, solid, varied};

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

/// A `format` is a version of a recipe file, which image's reader reads 1 to 3 of. A recipe written with one is taken if the
/// linked crate reads it, and refused naming the version found and the versions read if not.
#[test]
fn a_format_the_linked_image_crate_reads_is_taken_and_one_it_does_not_is_refused_naming_both() -> Outcome {
    let picture = solid(2, 2, [0.25, 0.25, 0.25, 1.0])?;
    for format in [1, 2, 3] {
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

#[test]
fn a_look_on_a_layer_that_is_not_a_picture_is_refused_until_the_batch_that_brings_it() -> Outcome {
    let drawing = Layer::drawing(&crate::common::rectangle((4, 4), (0.0, 0.0), (4.0, 4.0), "#ff0000"))?;
    for (what, layer) in [("a fill", Layer::fill("#ff0000")?), ("a drawing", drawing)] {
        let refused = layer.look(PLUS_ONE_STOP);
        assert!(
            matches!(&refused, Err(Error::Invalid { address, message }) if address == "look" && message.contains("picture")),
            "{what}: {refused:?}"
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
