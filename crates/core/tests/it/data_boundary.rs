//! What the pinned image and vector crates read and give at scene's data boundary: the recipe the Python `to_dict()`
//! writes, the drawing `to_json()` writes, and what each renders. A bump of a pin runs these first; a change in a
//! sibling's form fails here, naming it, before it fails in a layer.

use bullterrier_image::{Options, Photo, Picture, Presets, Recipe};
use bullterrier_vector::{Drawing, Inputs, Size, Values};

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// The three recipes as `bullterrier_image.Recipe.to_dict()` writes them at the pinned revision (the dicts printed by
/// the pinned package itself): a step has its parameters, its `opacity` and its `blend_mode`, a mask after them; a preset
/// is already expanded into the parameters; a region is a table with its `kind`; there is no `format` (a recipe file has
/// one, a recipe does not).
///
/// `Recipe([fx.grade(exposure=1.0)])`
const PLAIN_STEP: &str = r#"{"steps": [{"effect": "grade", "params": {"exposure": 1.0}, "opacity": 1.0, "blend_mode": "normal"}]}"#;

/// `Recipe([fx.gradient_map(preset="sepia")])`
const PRESET_EXPANDED: &str = r##"{"steps": [{"effect": "gradient_map", "params": {"stops": ["#1c1008", "#8a5a30", "#f1dfb8"]}, "opacity": 1.0, "blend_mode": "normal"}]}"##;

/// A step that names its preset, which only a recipe file read into data writes (`to_dict()` never does).
const PRESET_BY_NAME: &str = r#"{"steps": [{"effect": "gradient_map", "preset": "sepia", "opacity": 1.0, "blend_mode": "normal"}]}"#;

/// `Recipe([fx.grade(exposure=1.0, mask="here")], regions={"here": LinearGradient(angle=30)})`
const WITH_REGIONS: &str = r#"{"steps": [{"effect": "grade", "params": {"exposure": 1.0}, "opacity": 1.0, "blend_mode": "normal", "mask": "here"}], "regions": {"here": {"kind": "linear_gradient", "angle": 30.0}}}"#;

const RECIPES: [(&str, &str); 4] = [
    ("a plain step", PLAIN_STEP),
    ("a preset, expanded", PRESET_EXPANDED),
    ("a preset by name", PRESET_BY_NAME),
    ("a recipe with regions", WITH_REGIONS),
];

/// A picture of `height` by `width` pixels whose three channels all differ, so a mix-up of rows, columns or channels
/// shows.
fn pixels(height: usize, width: usize) -> Vec<f32> {
    (0..height * width * 3)
        .map(|index| f32::from(u8::try_from(index * 37 % 251).unwrap_or(0)) / 255.0)
        .collect()
}

/// What image's own render of `recipe` gives for `values`, a `height` by `width` picture.
fn rendered(recipe: &str, height: usize, width: usize, values: &[f32]) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let presets = Presets::shipped()?;
    let recipe = Recipe::from_json(recipe)?.checked(&presets)?;
    let photo = Photo::new(Picture::new(height, width, values)?);
    let image = photo.render(&recipe, &presets, Options::default())?;
    assert_eq!((image.height(), image.width(), image.channels()), (height, width, 3));
    Ok(image.into_values())
}

#[test]
fn the_pinned_image_reader_reads_every_form_to_dict_writes_and_renders_it() -> Outcome {
    let values = pixels(12, 20);
    for (what, recipe) in RECIPES {
        let image = rendered(recipe, 12, 20, &values).map_err(|error| format!("{what}: {error}"))?;
        assert_eq!(image.len(), values.len(), "{what}");
    }
    Ok(())
}

#[test]
fn a_recipe_that_carries_a_format_is_refused_by_the_pinned_reader() {
    let text = r#"{"format": 3, "steps": []}"#;
    let refused = Recipe::from_json(text).map_err(|error| error.to_string());
    assert!(refused.is_err_and(|message| message.contains("format")));
}

/// Hand computation, from `grade`'s documented rule: exposure +1 stop doubles every value, and with contrast 1 and no
/// colour balance nothing else moves. 0.25 becomes 0.5 and 0.4 becomes 0.8; 0.6 doubles past 1 and is clipped to 1.
#[test]
fn exposure_of_one_stop_doubles_the_values_and_clips_at_one() -> Outcome {
    let values = [0.25, 0.4, 0.6, 0.25, 0.4, 0.6];
    let image = rendered(PLAIN_STEP, 1, 2, &values)?;
    for (got, expected) in image.iter().zip([0.5, 0.8, 1.0, 0.5, 0.8, 1.0]) {
        assert!((got - expected).abs() < 1e-6, "{got} is not {expected}");
    }
    Ok(())
}

/// A drawing as `Drawing.to_json()` writes it at the pinned revision, for `Drawing(Path.rect(10, 5, 20, 10), size=(40,
/// 20)).fill("#ff0000")`: a 40 by 20 frame with a red rectangle from (10, 5) to (30, 15).
const RECTANGLE: &str = r##"{"root":[{"stack":{"items":[[{"path":"M10 5 L30 5 L30 15 L10 15 Z"}]],"size":[40,20]}},{"fill":{"color":"#ff0000"}}],"version":1}"##;

/// The straight RGBA bytes vector draws `RECTANGLE` into, `width` by `height`.
fn drawn(width: u32, height: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let drawing = Drawing::from_json(RECTANGLE, &Inputs::new())?;
    Ok(drawing.to_rgba(Size::px(f64::from(width), f64::from(height)), &Values::new())?)
}

#[test]
fn the_pinned_vector_reader_gives_the_frame_and_draws_the_rectangle_exactly() -> Outcome {
    let drawing = Drawing::from_json(RECTANGLE, &Inputs::new())?;
    assert_eq!(drawing.size(), Some((40.0, 20.0)));
    for scale in [1_u32, 2, 4] {
        let rgba = drawn(40 * scale, 20 * scale)?;
        let red = rgba.chunks_exact(4).filter(|pixel| *pixel == [255, 0, 0, 255]).count();
        let clear = rgba.chunks_exact(4).filter(|pixel| pixel[3] == 0).count();
        let covered = usize::try_from(20 * 10 * scale * scale)?;
        assert_eq!(red, covered, "red at {scale}x");
        assert_eq!(clear, rgba.len() / 4 - covered, "everything else is clear at {scale}x");
    }
    Ok(())
}

#[test]
fn a_drawing_without_a_frame_has_no_size() -> Outcome {
    let text = r##"{"version": 1, "root": [{"path": "M0 0 H10 V10 H0 Z"}, {"fill": {"color": "#ff0000"}}]}"##;
    assert_eq!(Drawing::from_json(text, &Inputs::new())?.size(), None);
    Ok(())
}
