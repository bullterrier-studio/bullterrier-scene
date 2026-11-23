//! Parameters and `with_values`: the addresses a scene lists, the values it takes, and what it refuses. The oracles are the
//! addresses the api table writes, the layer built with the same values by its own calls, the sibling crates' own setters, and
//! the round trip: every parameter set to its own value changes nothing.

use std::path::PathBuf;

use bullterrier_image::{Presets, Recipe};
use bullterrier_scene::{Blend, Deg, Error, Fit, Kind, Layer, Map, Parameter, Place, Raster, Scene, Value, Values};

use crate::common::{Outcome, all_exactly, numbered, rectangle, same_look, scene, solid};

const GRADE: &str = r#"{"steps": [{"effect": "grade", "params": {"exposure": 0.5}}]}"#;

/// A drawing of 40 by 20 design pixels with a rectangle that slides by the Param `x` and turns by the Param `spin`.
const SLIDER: &str = r##"{"params":[{"name":"x","default":0,"min":-10,"max":30,"unit":"px","meaning":"slide"},{"name":"spin","default":0,"min":0,"max":360,"unit":"deg","meaning":"turn"}],"root":[{"stack":{"items":[[{"path":"M0 0 L8 0 L8 8 L0 8 Z"}]],"size":[40,20]}},{"translate":{"dx":{"param":"x"},"dy":0}},{"rotate":{"angle":{"param":"spin"}}},{"fill":{"color":"#ff0000"}}],"version":1}"##;

/// The addresses of `parameters`.
fn addresses(parameters: &[Parameter]) -> Vec<&str> {
    parameters.iter().map(|parameter| parameter.address.as_str()).collect()
}

/// The parameter at `address`.
fn at<'a>(parameters: &'a [Parameter], address: &str) -> Result<&'a Parameter, String> {
    parameters
        .iter()
        .find(|parameter| parameter.address == address)
        .ok_or_else(|| format!("no parameter at {address}"))
}

/// The recipe of `GRADE`, and the addresses image lists for it.
fn grade_numbers() -> Result<Vec<String>, Error> {
    let recipe = Recipe::from_json(GRADE).map_err(|error| Error::Format {
        pointer: String::new(),
        message: error.to_string(),
    })?;
    let numbers = recipe.parameters().map_err(|error| Error::Format {
        pointer: String::new(),
        message: error.to_string(),
    })?;
    Ok(numbers
        .into_iter()
        .map(|number| number.address)
        .filter(|address| !address.starts_with("frame."))
        .collect())
}

fn picture() -> Result<Raster, Error> {
    numbered(6, 4)
}

fn matte() -> Result<Map, Error> {
    Map::new(
        6,
        4,
        (0..24)
            .map(|at| f32::from(u8::try_from(at).unwrap_or(0)) / 24.0)
            .collect(),
    )
}

/// One layer of each kind, a reference, a mask layer, a look, a drawing with Params and a group: the scene every listing test
/// reads. Canvas 40 by 20.
fn everything() -> Result<Scene, Error> {
    let badge = scene(
        20,
        10,
        [Layer::text(
            "Hi",
            &bullterrier_scene::Text::new("bricolage_grotesque").option("size", 8.0),
        )?
        .named("label")?],
    )?;
    scene(
        40,
        20,
        [
            Layer::picture(picture()?)
                .map("subject", matte()?)?
                .look(GRADE)?
                .mask("subject")?
                .mask(Layer::fill("#ffffff")?.place(Place::new().dx(3.0))?)?
                .named("photo")?,
            Layer::fill("#101018")?.named("band")?,
            Layer::text(
                "Title",
                &bullterrier_scene::Text::new("bricolage_grotesque")
                    .color("#FFFFFF")
                    .option("size", 8.0),
            )?
            .named("title")?,
            Layer::drawing(SLIDER)?.named("logo")?,
            Layer::scene(badge).named("badge")?,
            Layer::of("photo")
                .look(GRADE)?
                .place(Place::new().dx(1.0))?
                .named("glow")?,
            Layer::fill("#ffffff")?,
            Layer::fill("#00ff00")?
                .perspective([[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]])?
                .named("slant")?,
            Layer::picture_hole().named("blank")?,
        ],
    )
}

/// The nine fields of a placement, as `parameters()` lists them.
const PLACEMENT: [&str; 9] = [
    "dx", "dy", "scale", "aspect", "rotate", "skew_x", "skew_y", "flip_x", "flip_y",
];

fn under(layer: &str, fields: &[&str]) -> Vec<String> {
    fields.iter().map(|field| format!("{layer}.{field}")).collect()
}

/// The api table's addresses for `everything()`, written by hand, in order: a layer's fields, its source's, its looks', its
/// masks'.
#[test]
fn a_scene_lists_the_addresses_of_the_api_table_in_order() -> Outcome {
    let grade = grade_numbers()?;
    let mut expected: Vec<String> = Vec::new();
    // photo: fields, picture and map, look, then masks by number: the map mask is the first (no parameter), the layer is `mask#2`
    expected.extend(under("photo", &["opacity", "blend", "light"]));
    expected.extend(under("photo", &PLACEMENT));
    expected.extend(under("photo", &["picture", "map.subject"]));
    expected.extend(grade.iter().map(|address| format!("photo.look.{address}")));
    expected.extend(under("photo", &["mask#2.opacity"]));
    expected.extend(under(
        "photo",
        &PLACEMENT
            .map(|field| format!("mask#2.{field}"))
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    ));
    expected.extend(under("photo", &["mask#2.color"]));
    expected.extend(under("band", &["opacity", "blend", "light"]));
    expected.extend(under("band", &PLACEMENT));
    expected.extend(under("band", &["color"]));
    expected.extend(under("title", &["opacity", "blend", "light"]));
    expected.extend(under("title", &PLACEMENT));
    expected.extend(under("title", &["text", "color"]));
    expected.extend(under("logo", &["opacity", "blend", "light"]));
    expected.extend(under("logo", &PLACEMENT));
    expected.extend(under("logo", &["drawing.x", "drawing.spin"]));
    expected.extend(under("badge", &["opacity", "blend", "light"]));
    expected.extend(under("badge", &PLACEMENT));
    expected.extend(under(
        "badge",
        &["scene.label.opacity", "scene.label.blend", "scene.label.light"],
    ));
    expected.extend(under(
        "badge",
        &PLACEMENT
            .map(|field| format!("scene.label.{field}"))
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    ));
    expected.extend(under("badge", &["scene.label.text", "scene.label.color"]));
    // a reference: the fields of a layer and its own looks, no source
    expected.extend(under("glow", &["opacity", "blend", "light"]));
    expected.extend(under("glow", &PLACEMENT));
    expected.extend(grade.iter().map(|address| format!("glow.look.{address}")));
    expected.extend(under("layer7", &["opacity", "blend", "light"]));
    expected.extend(under("layer7", &PLACEMENT));
    expected.extend(under("layer7", &["color"]));
    // a layer under `perspective` has no placement: its corners are not a parameter
    expected.extend(under("slant", &["opacity", "blend", "light"]));
    expected.extend(under("slant", &["color"]));
    expected.extend(under("blank", &["opacity", "blend", "light"]));
    expected.extend(under("blank", &PLACEMENT));
    expected.extend(under("blank", &["picture"]));
    let listed = everything()?.parameters();
    let got: Vec<&str> = addresses(&listed);
    assert_eq!(got, expected.iter().map(String::as_str).collect::<Vec<_>>());
    Ok(())
}

/// Every field of a parameter, by hand, for the kinds the layer itself has.
#[test]
fn each_kind_of_parameter_is_described_as_the_format_says() -> Outcome {
    let listed = everything()?.parameters();
    let opacity = at(&listed, "photo.opacity")?;
    assert_eq!(
        (
            opacity.kind,
            opacity.unit.as_deref(),
            opacity.min,
            opacity.max,
            opacity.cyclic
        ),
        (Kind::Float, Some("fraction"), Some(0.0), Some(1.0), false)
    );
    assert_eq!(
        (&opacity.default, &opacity.value),
        (&Value::Number(1.0), &Value::Number(1.0))
    );

    let blend = at(&listed, "photo.blend")?;
    assert_eq!(
        (blend.kind, blend.choices.len(), &blend.default),
        (Kind::Choice, 16, &Value::Text("normal".into()))
    );
    assert_eq!(blend.choices[0], "normal");
    assert_eq!(blend.choices[6], "color-dodge");
    assert!(!blend.nullable && blend.unit.is_none() && blend.min.is_none());

    let light = at(&listed, "photo.light")?;
    assert_eq!(
        (light.kind, &light.value, light.unit.as_deref()),
        (Kind::Bool, &Value::Bool(false), None)
    );

    let rotate = at(&listed, "photo.rotate")?;
    assert_eq!(
        (
            rotate.kind,
            rotate.unit.as_deref(),
            rotate.min,
            rotate.max,
            rotate.cyclic
        ),
        (Kind::Float, Some("degrees"), Some(0.0), Some(360.0), true)
    );
    let ranges = [
        ("dx", "px", -1e6, 1e6),
        ("dy", "px", -1e6, 1e6),
        ("scale", "ratio", 0.001, 1000.0),
        ("aspect", "ratio", 0.001, 1000.0),
        ("skew_x", "degrees", -89.0, 89.0),
        ("skew_y", "degrees", -89.0, 89.0),
    ];
    for (field, unit, low, high) in ranges {
        let parameter = at(&listed, &format!("photo.{field}"))?;
        assert_eq!(
            (
                parameter.unit.as_deref(),
                parameter.min,
                parameter.max,
                parameter.cyclic
            ),
            (Some(unit), Some(low), Some(high), false),
            "{field}"
        );
    }
    assert_eq!(at(&listed, "photo.scale")?.default, Value::Number(1.0));
    assert_eq!(at(&listed, "photo.flip_x")?.kind, Kind::Bool);

    let picture = at(&listed, "photo.picture")?;
    assert_eq!(
        (picture.kind, picture.nullable, &picture.default),
        (Kind::Picture, true, &Value::Hole)
    );
    assert!(matches!(picture.value, Value::Picture(_)));
    assert!(matches!(at(&listed, "photo.map.subject")?.value, Value::Map(_)));
    assert_eq!(at(&listed, "blank.picture")?.value, Value::Hole);

    let text = at(&listed, "title.text")?;
    assert_eq!(
        (text.kind, text.min_length, &text.value),
        (Kind::Text, Some(1), &Value::Text("Title".into()))
    );
    let color = at(&listed, "title.color")?;
    assert_eq!(
        (color.kind, &color.value),
        (Kind::Color, &Value::Text("#ffffff".into()))
    );

    let slide = at(&listed, "logo.drawing.x")?;
    assert_eq!(
        (
            slide.unit.as_deref(),
            slide.min,
            slide.max,
            slide.cyclic,
            slide.meaning.as_str()
        ),
        (Some("px"), Some(-10.0), Some(30.0), false, "slide")
    );
    let spin = at(&listed, "logo.drawing.spin")?;
    assert_eq!(
        (spin.unit.as_deref(), spin.min, spin.max, spin.cyclic),
        (Some("degrees"), Some(0.0), Some(360.0), true)
    );

    // a look's numbers are image's, as it describes them
    let exposure = at(&listed, "photo.look.grade.exposure")?;
    assert_eq!((exposure.kind, &exposure.value), (Kind::Float, &Value::Number(0.5)));
    Ok(())
}

/// A whole picture drawn as the scene holds it, for before and after comparisons.
fn drawn(scene: &Scene) -> Result<Raster, Error> {
    scene.render(40)
}

/// The scene to render: `everything()` without the picture hole.
pub(crate) fn drawable() -> Result<Scene, Box<dyn std::error::Error>> {
    let all = everything()?;
    let hole = all.parameters();
    let blank = at(&hole, "blank.picture")?.address.clone();
    Ok(all.with_values(&Values::new().set(blank, solid(6, 4, [0.2, 0.4, 0.6, 1.0])?))?)
}

/// The round trip: every parameter set to the value it holds is accepted and changes nothing: the parameters are the same
/// and the render looks the same. An address that is not listed is refused.
#[test]
fn every_parameter_set_to_its_own_value_changes_nothing() -> Outcome {
    let before = drawable()?;
    let listed = before.parameters();
    let mut all = Values::new();
    for parameter in &listed {
        all = all.set(parameter.address.clone(), parameter.value.clone());
    }
    let after = before.with_values(&all)?;
    assert_eq!(after.parameters(), listed);
    assert!(same_look(&drawn(&after)?, &drawn(&before)?));
    // one at a time, each is accepted
    for parameter in &listed {
        let one = Values::new().set(parameter.address.clone(), parameter.value.clone());
        let changed = before
            .with_values(&one)
            .map_err(|error| format!("{} refused its own value: {error}", parameter.address))?;
        assert_eq!(changed.parameters(), listed, "{}", parameter.address);
    }
    Ok(())
}

/// The listing agrees with what `with_values` takes: a near-miss of each address is refused, and so is every address of a
/// layer another kind has.
#[test]
fn an_address_the_scene_does_not_list_is_refused() -> Outcome {
    let before = drawable()?;
    let listed = before.parameters();
    for parameter in &listed {
        for wrong in [
            format!("{}x", parameter.address),
            format!("{}.", parameter.address),
            parameter.address.to_uppercase(),
        ] {
            assert!(
                before
                    .with_values(&Values::new().set(wrong.clone(), parameter.value.clone()))
                    .is_err(),
                "{wrong} was accepted"
            );
        }
    }
    // a perspective layer has no `dx`, a reference no `picture`, a fill no `text`
    for wrong in [
        "slant.dx",
        "glow.picture",
        "band.text",
        "photo.look#1.grade.exposure",
        "photo.look#0.grade.exposure",
        "badge.scene.nothing.opacity",
    ] {
        assert!(
            before.with_values(&Values::new().set(wrong, 1.0)).is_err(),
            "{wrong} was accepted"
        );
    }
    Ok(())
}

/// A layer set through `with_values` equals the layer built with the call that sets the field: the differential for every
/// field of a placement and the three joins.
#[test]
fn a_value_is_what_the_layers_own_call_would_set() -> Outcome {
    let base = || Layer::picture(numbered(6, 4).unwrap_or_else(|error| unreachable!("{error}"))).named("p");
    let canvas = (12, 8);
    let cases: Vec<(Vec<(&str, Value)>, Layer)> = vec![
        (
            vec![
                ("p.dx", 3.0.into()),
                ("p.dy", (-2.0).into()),
                ("p.scale", 1.5.into()),
                ("p.aspect", 0.75.into()),
                ("p.rotate", 20.0.into()),
                ("p.skew_x", 10.0.into()),
                ("p.skew_y", (-5.0).into()),
                ("p.flip_x", true.into()),
                ("p.flip_y", true.into()),
            ],
            base()?.place(
                Place::new()
                    .dx(3.0)
                    .dy(-2.0)
                    .scale(1.5)
                    .aspect(0.75)
                    .rotate(Deg(20.0))
                    .skew_x(Deg(10.0))
                    .skew_y(Deg(-5.0))
                    .flip_x(true)
                    .flip_y(true),
            )?,
        ),
        (
            vec![
                ("p.opacity", 0.25.into()),
                ("p.blend", "multiply".into()),
                ("p.light", true.into()),
            ],
            base()?.opacity(0.25)?.blend(Blend::Multiply).as_light(true),
        ),
        (
            vec![("p.rotate", 380.0.into())],
            base()?.place(Place::new().rotate(Deg(380.0)))?,
        ),
        (
            vec![("p.rotate", (-90.0).into())],
            base()?.place(Place::new().rotate(Deg(270.0)))?,
        ),
    ];
    let backdrop = || Layer::fill("#336699").and_then(|layer| layer.named("backdrop"));
    for (values, layer) in cases {
        let mut set = Values::new();
        for (address, value) in values {
            set = set.set(address, value);
        }
        let got = scene(canvas.0, canvas.1, [backdrop()?, base()?])?
            .with_values(&set)?
            .render(24)?;
        let expected = scene(canvas.0, canvas.1, [backdrop()?, layer])?.render(24)?;
        assert!(same_look(&got, &expected), "{set:?}");
    }
    Ok(())
}

/// What `with_values` gives for one value, as the refusal it is.
fn refusal(scene: &Scene, address: &str, value: impl Into<Value>) -> Result<String, String> {
    match scene.with_values(&Values::new().set(address, value)) {
        Err(error @ Error::Invalid { .. }) => Ok(error.to_string()),
        Err(other) => Err(format!("not an Invalid error: {other:?}")),
        Ok(_) => Err(format!("{address} was accepted")),
    }
}

/// Known-bad rows: each refusal names the address and what is wrong; the good rows beside them are accepted.
#[test]
fn a_value_that_breaks_its_rule_is_refused_naming_the_address_and_the_value() -> Outcome {
    let before = drawable()?;
    let bad: Vec<(&str, Value, &str)> = vec![
        ("photo.opacity", 1.5.into(), "1.5"),
        ("photo.opacity", (-0.1).into(), "-0.1"),
        ("photo.opacity", f64::NAN.into(), "NaN"),
        ("photo.opacity", f64::INFINITY.into(), "inf"),
        ("photo.opacity", true.into(), "a number is wanted"),
        ("photo.opacity", "0.5".into(), "a number is wanted"),
        ("photo.dx", 1.000_000_1e6.into(), "1000000"),
        ("photo.dx", (-1.000_000_1e6).into(), "-1000000"),
        ("photo.scale", 0.0.into(), "0.001"),
        ("photo.scale", 1000.5.into(), "1000"),
        ("photo.aspect", 0.0.into(), "0.001"),
        ("photo.skew_x", 89.5.into(), "-89"),
        ("photo.skew_y", (-90.0).into(), "-89"),
        ("photo.rotate", f64::NAN.into(), "degrees"),
        ("photo.flip_x", 1.0.into(), "a bool is wanted"),
        ("photo.light", "yes".into(), "a bool is wanted"),
        ("photo.blend", "mulitply".into(), "multiply"),
        ("photo.blend", "Multiply".into(), "multiply"),
        ("photo.blend", 3.0.into(), "a text is wanted"),
        ("band.color", "red".into(), "#rrggbb"),
        ("band.color", "#12345".into(), "#rrggbb"),
        ("band.color", 1.0.into(), "a text is wanted"),
        ("title.text", "".into(), "at least 1 character"),
        ("title.text", "   ".into(), "text"),
        ("title.text", 4.0.into(), "a text is wanted"),
        ("title.color", "not a colour".into(), "color"),
        ("photo.picture", "photo.png".into(), "a picture"),
        ("photo.picture", matte()?.into(), "a picture"),
        ("photo.map.subject", picture()?.into(), "a map"),
        ("photo.map.subject", 0.5.into(), "a map"),
        ("logo.drawing.x", 31.0.into(), "30"),
        ("logo.drawing.x", f64::NAN.into(), "finite"),
        ("logo.drawing.x", "far".into(), "a number is wanted"),
        ("photo.look.grade.exposure", 99.0.into(), "exposure"),
        ("photo.look.grade.exposure", true.into(), "a number is wanted"),
        ("photo.look.grade.exposure", f64::NAN.into(), "finite"),
        ("badge.scene.label.text", "".into(), "at least 1 character"),
        ("photo.mask#2.opacity", 2.0.into(), "photo.mask#2.opacity"),
        ("glow.look.grade.exposure", 99.0.into(), "glow.look.grade.exposure"),
    ];
    for (address, value, named) in bad {
        let message =
            refusal(&before, address, value.clone()).map_err(|why| format!("{address} = {value:?}: {why}"))?;
        assert!(
            message.starts_with(address) || message.contains(address),
            "{address}: `{message}` does not name the address"
        );
        assert!(
            message.contains(named),
            "{address} = {value:?}: `{message}` lacks `{named}`"
        );
    }
    // the same rows' good neighbours
    let good = Values::new()
        .set("photo.opacity", 1.0)
        .set("photo.opacity", 0.0)
        .set("photo.dx", 1e6)
        .set("photo.dy", -1e6)
        .set("photo.scale", 0.001)
        .set("photo.aspect", 1000.0)
        .set("photo.skew_x", 89.0)
        .set("photo.skew_y", -89.0)
        .set("photo.rotate", 1e9)
        .set("photo.blend", "soft-light")
        .set("title.text", "Pilot")
        .set("band.color", "#ABCDEF80");
    before.with_values(&good)?;
    Ok(())
}

/// A cyclic number wraps into its turn; a whole-number parameter takes `3.0` and refuses `3.5`.
#[test]
fn a_cyclic_value_wraps_and_a_whole_number_must_be_whole() -> Outcome {
    let before = drawable()?;
    for (given, wrapped) in [(370.0, 10.0), (-90.0, 270.0), (360.0, 0.0), (720.0, 0.0), (45.0, 45.0)] {
        let after = before.with_values(&Values::new().set("photo.rotate", given).set("logo.drawing.spin", given))?;
        let listed = after.parameters();
        assert_eq!(at(&listed, "photo.rotate")?.value, Value::Number(wrapped), "{given}");
        assert_eq!(
            at(&listed, "logo.drawing.spin")?.value,
            Value::Number(wrapped),
            "{given}"
        );
    }
    // a tiny negative angle is not 360 (the end of the turn is not in it)
    let after = before.with_values(&Values::new().set("photo.rotate", -1e-20))?;
    assert_eq!(at(&after.parameters(), "photo.rotate")?.value, Value::Number(0.0));
    Ok(())
}

/// All or nothing: a bad value among good ones changes nothing, and the original scene is never changed.
#[test]
fn a_refused_set_changes_nothing_and_the_original_is_never_changed() -> Outcome {
    let before = drawable()?;
    let listed = before.parameters();
    let picture = drawn(&before)?;
    let mixed = Values::new()
        .set("band.color", "#ff0000")
        .set("photo.opacity", 0.5)
        .set("title.text", "later")
        .set("logo.drawing.x", 99.0);
    assert!(before.with_values(&mixed).is_err());
    assert_eq!(before.parameters(), listed);
    assert!(same_look(&drawn(&before)?, &picture));
    // a good set returns a new scene; the first one is as it was
    let changed = before.with_values(&Values::new().set("band.color", "#ff0000"))?;
    assert_ne!(changed.parameters(), listed);
    assert_eq!(before.parameters(), listed);
    assert!(same_look(&drawn(&before)?, &picture));
    // determinism: the same values give the same scene
    let again = before.with_values(&Values::new().set("band.color", "#ff0000"))?;
    assert!(same_look(&drawn(&changed)?, &drawn(&again)?));
    Ok(())
}

/// The error of an address that is no parameter lists the addresses there are and the nearest.
#[test]
fn an_unknown_address_lists_what_there_is_and_the_nearest() -> Outcome {
    let before = drawable()?;
    let near = refusal(&before, "title.tex", 1.0)?;
    assert!(near.starts_with("title.tex"), "{near}");
    assert!(near.contains("opacity") && near.contains("did you mean text"), "{near}");
    let layer = refusal(&before, "ttle.text", 1.0)?;
    assert!(
        layer.contains("title") && layer.contains("photo") && layer.contains("did you mean"),
        "{layer}"
    );
    // `layer1` is not the name of a layer that has one
    let default_name = refusal(&before, "layer1.opacity", 0.5)?;
    assert!(default_name.contains("layer1.opacity"), "{default_name}");
    let bare = refusal(&before, "title", 1.0)?;
    assert!(bare.contains("<layer>.<field>"), "{bare}");
    // a huge address is cut in the message and costs no search
    let huge = format!("title.{}", "x".repeat(1_000_000));
    let message = refusal(&before, &huge, 1.0)?;
    assert!(message.len() < 4000, "{} characters", message.len());
    Ok(())
}

/// A hole is refused at render, naming the layer and the field, until `with_values` fills it; it can be made again.
#[test]
fn a_hole_is_filled_by_a_value_and_can_be_made_again() -> Outcome {
    let blank = scene(6, 4, [Layer::picture_hole().named("photo")?.map_hole("subject")?])?;
    let message = blank.render(6).err().map(|error| error.to_string()).ok_or("rendered")?;
    assert!(message.contains("photo.picture"), "{message}");
    let photo_only = blank.with_values(&Values::new().set("photo.picture", picture()?))?;
    let message = photo_only
        .render(6)
        .err()
        .map(|error| error.to_string())
        .ok_or("rendered")?;
    assert!(message.contains("photo.map.subject"), "{message}");
    let full = photo_only.with_values(&Values::new().set("photo.map.subject", matte()?))?;
    let rendered = full.render(6)?;
    let direct = scene(6, 4, [Layer::picture(picture()?).named("photo")?])?.render(6)?;
    assert!(same_look(&rendered, &direct));
    let again = full.with_values(&Values::new().set("photo.picture", Value::Hole))?;
    assert!(again.render(6).is_err());
    // a path is a value too: read at render, so a missing file is a read error naming the layer
    let by_file = blank.with_values(&Values::new().set("photo.picture", PathBuf::from("nowhere.png")))?;
    assert!(matches!(by_file.render(6), Err(Error::Read { .. })));
    Ok(())
}

/// A look's number is set through image's own setter: the scene's look is the recipe `Recipe::with_values` makes, and the cross-field
/// rules of image hold for the set.
#[test]
fn a_look_address_is_set_as_images_own_with_values_would() -> Outcome {
    let layer = || Layer::picture(numbered(6, 4).unwrap_or_else(|error| unreachable!("{error}"))).look(GRADE);
    let template = scene(6, 4, [layer()?.named("p")?])?;
    let got = template
        .with_values(&Values::new().set("p.look.grade.exposure", -1.0))?
        .render(6)?;
    let recipe = Recipe::from_json(GRADE)
        .and_then(|recipe| recipe.with_value("grade.exposure", -1.0, &Presets::shipped()?))
        .map_err(|error| error.to_string())?;
    let json = serde_json::to_string(&recipe)?;
    let expected = scene(6, 4, [Layer::picture(numbered(6, 4)?).look(&json)?.named("p")?])?.render(6)?;
    assert!(same_look(&got, &expected));
    // a second look is `look#2`, and only its own number changes
    let two = scene(6, 4, [layer()?.look(GRADE)?.named("p")?])?;
    let second = two.with_values(&Values::new().set("p.look#2.grade.exposure", 1.0))?;
    let listed = second.parameters();
    assert_eq!(at(&listed, "p.look.grade.exposure")?.value, Value::Number(0.5));
    assert_eq!(at(&listed, "p.look#2.grade.exposure")?.value, Value::Number(1.0));
    Ok(())
}

/// A drawing's Param changes that drawing as vector's own Params do, and still lists after it is set.
#[test]
fn a_drawing_param_moves_the_drawing_and_stays_listed() -> Outcome {
    // the rectangle drawn where the Param puts it: a document of its own, with no Param
    let slid = |x: f64| -> Result<Raster, Error> {
        let moved = rectangle((40, 20), (x, 0.0), (x + 8.0, 8.0), "#ff0000");
        scene(40, 20, [Layer::drawing(&moved)?])?.render(40)
    };
    let template = scene(40, 20, [Layer::drawing(SLIDER)?.named("logo")?])?;
    for x in [0.0, 12.0, -4.0, 30.0] {
        let set = template.with_values(&Values::new().set("logo.drawing.x", x))?;
        assert!(same_look(&set.render(40)?, &slid(x)?), "x = {x}");
        // set once, listed still, and set again
        let listed = set.parameters();
        assert_eq!(at(&listed, "logo.drawing.x")?.value, Value::Number(x));
        assert_eq!(at(&listed, "logo.drawing.x")?.default, Value::Number(0.0));
        let twice = set.with_values(&Values::new().set("logo.drawing.x", 5.0))?;
        assert!(same_look(&twice.render(40)?, &slid(5.0)?));
    }
    Ok(())
}

/// A scene inside a layer is changed as a value: its own addresses sit under the layer, a reference inside it follows, and the
/// scene that was used is not changed.
#[test]
fn an_address_under_a_group_changes_the_inner_scene_as_a_value() -> Outcome {
    let inner = scene(
        20,
        10,
        [
            Layer::fill("#ff0000")?
                .named("base")?
                .fit([0.0, 0.0, 10.0, 10.0], Fit::Stretch, [0.5, 0.5])?,
            Layer::of("base").named("again")?.place(Place::new().dx(10.0))?,
        ],
    )?;
    let outer = scene(20, 10, [Layer::scene(inner.clone()).named("badge")?])?;
    let set = outer.with_values(&Values::new().set("badge.scene.base.color", "#00ff00"))?;
    // the reference inside the group follows the layer it names
    let direct = inner.with_values(&Values::new().set("base.color", "#00ff00"))?;
    assert!(same_look(&set.render(20)?, &direct.render(20)?));
    assert!(!same_look(&set.render(20)?, &outer.render(20)?));
    // the scene that was used stays as it was
    assert!(
        inner
            .parameters()
            .iter()
            .all(|parameter| parameter.address != "badge.scene.base.color")
    );
    assert_eq!(
        at(&inner.parameters(), "base.color")?.value,
        Value::Text("#ff0000".into())
    );
    // an inner address that is no parameter is refused under the group's name
    let message = refusal(&outer, "badge.scene.base.nothing", 1.0)?;
    assert!(message.contains("badge.scene.base.nothing"), "{message}");
    Ok(())
}

/// A mask layer is changed through its mask's number: `mask` is the first call, `mask#2` the second, a map mask counting.
#[test]
fn a_mask_layer_moves_with_its_address() -> Outcome {
    let make = |dx: f64| -> Result<Scene, Error> {
        scene(
            12,
            8,
            [
                Layer::fill("#ffffff")?,
                Layer::picture(numbered(12, 8)?)
                    .mask(
                        Layer::fill("#000000")?
                            .fit([0.0, 0.0, 4.0, 8.0], Fit::Stretch, [0.5, 0.5])?
                            .place(Place::new().dx(dx))?,
                    )?
                    .named("photo")?,
            ],
        )
    };
    let moved = make(0.0)?.with_values(&Values::new().set("photo.mask.dx", 4.0))?;
    assert!(same_look(&moved.render(12)?, &make(4.0)?.render(12)?));
    // a mask of a mask is `mask.mask`
    let nested = scene(
        12,
        8,
        [Layer::fill("#ff0000")?
            .mask(Layer::fill("#ffffff")?.mask(Layer::fill("#ffffff")?)?)?
            .named("x")?],
    )?;
    let listed = nested.parameters();
    assert!(listed.iter().any(|parameter| parameter.address == "x.mask.mask.dx"));
    let set = nested.with_values(&Values::new().set("x.mask.mask.opacity", 0.25))?;
    assert_eq!(at(&set.parameters(), "x.mask.mask.opacity")?.value, Value::Number(0.25));
    assert_eq!(at(&set.parameters(), "x.mask.opacity")?.value, Value::Number(1.0));
    Ok(())
}

/// A reference follows a changed named layer, and its own fields are its own.
#[test]
fn a_reference_follows_the_layer_it_names_when_that_layer_is_changed() -> Outcome {
    let template = scene(
        12,
        8,
        [
            Layer::fill("#102030")?,
            Layer::picture(numbered(4, 4)?).named("figure")?,
            Layer::of("figure").named("copy")?.place(Place::new().dx(6.0))?,
        ],
    )?;
    let set = template.with_values(&Values::new().set("figure.dx", 2.0).set("copy.opacity", 0.5))?;
    let expected = scene(
        12,
        8,
        [
            Layer::fill("#102030")?,
            Layer::picture(numbered(4, 4)?)
                .named("figure")?
                .place(Place::new().dx(2.0))?,
            Layer::of("figure")
                .named("copy")?
                .place(Place::new().dx(6.0))?
                .opacity(0.5)?,
        ],
    )?;
    assert!(same_look(&set.render(24)?, &expected.render(24)?));
    // the reference's own `dx` is its extra place, and it is listed apart from the layer's
    let listed = set.parameters();
    assert_eq!(at(&listed, "copy.dx")?.value, Value::Number(6.0));
    assert_eq!(at(&listed, "figure.dx")?.value, Value::Number(2.0));
    Ok(())
}

/// Many values over many layers: the count and the order of a thousand layers do not matter to the result.
#[test]
fn a_thousand_layers_take_a_value_each() -> Outcome {
    let layers = (0..1000)
        .map(|number| Layer::fill("#101010").and_then(|layer| layer.named(&format!("l{number}"))))
        .collect::<Result<Vec<_>, _>>()?;
    let template = scene(8, 8, layers)?;
    let mut values = Values::new();
    for number in 0..1000 {
        values = values.set(format!("l{number}.opacity"), f64::from(number % 2));
    }
    let set = template.with_values(&values)?;
    assert!(all_exactly(
        &set.render(8)?,
        [16.0 / 255.0, 16.0 / 255.0, 16.0 / 255.0, 1.0]
    ));
    assert_eq!(set.parameters().len(), 1000 * (3 + 9 + 1));
    Ok(())
}

/// A drawing with no Param lists none.
#[test]
fn a_drawing_without_params_lists_none() -> Outcome {
    let flat = scene(
        10,
        10,
        [Layer::drawing(&rectangle((10, 10), (0.0, 0.0), (5.0, 5.0), "#ff0000"))?.named("r")?],
    )?;
    assert!(
        flat.parameters()
            .iter()
            .all(|parameter| !parameter.address.starts_with("r.drawing."))
    );
    Ok(())
}

/// Image's rule between two numbers of a look (a band's `low` is at most its `high`) holds for the set given together, and a
/// number that image says is whole takes `4.0` and refuses `3.5`.
#[test]
fn a_look_takes_the_rules_between_its_numbers_and_its_whole_numbers_from_image() -> Outcome {
    let band = r#"{"steps": [{"effect": "grade", "mask": "above"}], "regions": {"down": {"kind": "linear_gradient", "angle": 90.0}, "above": {"kind": "band", "of": "down", "low": 0.0, "high": 0.6, "feather": 0.05}}}"#;
    let template = scene(6, 4, [Layer::picture(numbered(6, 4)?).look(band)?.named("p")?])?;
    let low = "p.look.region.above.low";
    let high = "p.look.region.above.high";
    // each alone is fine inside the other's bound, and `low` above `high` is image's refusal, with its words
    template.with_values(&Values::new().set(low, 0.5))?;
    let alone = refusal(&template, low, 0.8)?;
    assert!(alone.contains("p.look"), "{alone}");
    // together they are a set that holds
    template.with_values(&Values::new().set(low, 0.8).set(high, 0.9))?;
    assert!(
        template
            .with_values(&Values::new().set(low, 0.8).set(high, 0.7))
            .is_err()
    );
    // a whole number
    let bloom = r#"{"steps": [{"effect": "bloom", "params": {"octaves": 4}}]}"#;
    let glow = scene(6, 4, [Layer::picture(numbered(6, 4)?).look(bloom)?.named("p")?])?;
    let listed = glow.parameters();
    let octaves = at(&listed, "p.look.bloom.octaves")?;
    assert_eq!(octaves.kind, Kind::Int);
    glow.with_values(&Values::new().set("p.look.bloom.octaves", 6.0))?;
    let message = refusal(&glow, "p.look.bloom.octaves", 3.5)?;
    assert!(message.contains("whole"), "{message}");
    Ok(())
}

/// Masks are numbered by the layer's `mask` calls, those by a map counting: the second call is `mask#2`, and a value set there
/// lands on that mask layer and no other.
#[test]
fn a_value_at_the_second_mask_lands_on_the_second_mask() -> Outcome {
    let before = drawable()?;
    let after = before.with_values(
        &Values::new()
            .set("photo.mask#2.opacity", 0.25)
            .set("photo.mask#2.dx", 5.0),
    )?;
    let listed = after.parameters();
    assert_eq!(at(&listed, "photo.mask#2.opacity")?.value, Value::Number(0.25));
    assert_eq!(at(&listed, "photo.mask#2.dx")?.value, Value::Number(5.0));
    // nothing else changed
    let changed: Vec<&str> = listed
        .iter()
        .zip(before.parameters().iter())
        .filter(|(now, was)| now.value != was.value)
        .map(|(now, _)| now.address.as_str())
        .collect();
    assert_eq!(changed, ["photo.mask#2.opacity", "photo.mask#2.dx"]);
    Ok(())
}
