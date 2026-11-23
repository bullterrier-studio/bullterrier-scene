//! A look's colour is a parameter of kind colour (`glow.look.point_light.color`): listed, settable by one parse path in image
//! (`#rrggbb`, `#rgb` or a triple), refused in words that name the scene's address. Oracles: the rendered pixels (the colour
//! of the light shows), a cold scene made with the colour, and image's own spellings of one colour.

use std::sync::Arc;

use bullterrier_scene::{Error, Kind, Layer, Map, Parameter, Place, Raster, Scene, Value, Values};

use crate::common::counting::Counting;
use crate::common::{Outcome, same_look, scene, varied};

/// A map of 0 to 1 over the 12 by 8 picture: how near each pixel is, which `point_light` lights.
fn depth() -> Result<Map, Error> {
    Map::new(
        12,
        8,
        (0..96)
            .map(|at| f32::from(u8::try_from(at).unwrap_or(0)) / 96.0)
            .collect(),
    )
}

const LAMP: &str = r##"{"steps": [{"effect": "point_light", "params": {"color": "#ffe2b8"}}]}"##;
const ADDRESS: &str = "glow.look.point_light.color";

/// A picture under a lamp, and a flat layer to leave alone.
fn lit() -> Result<Scene, Error> {
    scene(
        24,
        16,
        [
            Layer::fill("#202020")?.named("ground")?,
            Layer::picture(varied(12, 8)?)
                .map("depth", depth()?)?
                .look(LAMP)?
                .named("glow")?
                .place(Place::new().dx(2.0))?,
        ],
    )
}

fn parameter(scene: &Scene, address: &str) -> Result<Parameter, String> {
    scene
        .parameters()
        .into_iter()
        .find(|parameter| parameter.address == address)
        .ok_or_else(|| format!("{address} is not listed"))
}

fn render(scene: &Scene) -> Result<Raster, Error> {
    scene.render(48)
}

fn set(scene: &Scene, value: impl Into<Value>) -> Result<Scene, Error> {
    scene.with_values(&Values::new().set(ADDRESS, value))
}

#[test]
fn a_looks_colour_is_listed_as_a_colour_with_its_value_and_no_unit_or_range() -> Outcome {
    let found = parameter(&lit()?, ADDRESS)?;
    assert_eq!(found.kind, Kind::Color);
    assert_eq!(found.value, Value::Text("#ffe2b8".to_owned()));
    assert_eq!(found.default, found.value);
    assert_eq!((found.unit, found.min, found.max), (None, None, None));
    assert!(!found.cyclic && found.choices.is_empty());
    assert_ne!(found.meaning, "");
    Ok(())
}

#[test]
fn every_spelling_of_a_colour_gives_the_same_picture_and_it_is_not_the_default_one() -> Outcome {
    let base = lit()?;
    let by_hex = render(&set(&base, "#336699")?)?;
    assert!(same_look(&by_hex, &render(&set(&base, "#369")?)?));
    assert!(same_look(&by_hex, &render(&set(&base, Value::Rgb([0.2, 0.4, 0.6]))?)?));
    assert!(!same_look(&by_hex, &render(&base)?), "the colour of the lamp shows");
    // the value is read back as it was given, as text where it is a whole number of 255ths
    assert_eq!(
        parameter(&set(&base, "#369")?, ADDRESS)?.value,
        Value::Text("#336699".to_owned())
    );
    // a cold scene made with that colour draws the same
    let cold = scene(
        24,
        16,
        [
            Layer::fill("#202020")?.named("ground")?,
            Layer::picture(varied(12, 8)?)
                .map("depth", depth()?)?
                .look(&LAMP.replace("#ffe2b8", "#336699"))?
                .named("glow")?
                .place(Place::new().dx(2.0))?,
        ],
    )?;
    assert!(same_look(&by_hex, &render(&cold)?));
    Ok(())
}

#[test]
fn a_colour_between_two_levels_is_listed_as_the_triple_it_was_set_to() -> Outcome {
    let base = set(&lit()?, Value::Rgb([0.1, 0.25, 0.5]))?;
    let Value::Rgb([red, green, blue]) = parameter(&base, ADDRESS)?.value else {
        return Err("a triple is listed as a triple".into());
    };
    assert!((red - 0.1).abs() < 1e-6 && (green - 0.25).abs() < 1e-6 && (blue - 0.5).abs() < 1e-6);
    Ok(())
}

#[test]
fn a_bad_colour_or_a_wrong_kind_is_refused_naming_the_scene_address_and_nothing_is_set() -> Outcome {
    let base = lit()?;
    for (value, wanted) in [
        (Value::from("#12345"), "not a colour"),
        (Value::from("teal"), "'#rrggbb', '#rgb' or [r, g, b]"),
        (Value::Rgb([0.2, 1.5, 0.0]), "not a colour"),
        (Value::Number(0.5), "a colour"),
        (Value::Bool(true), "a colour"),
        (Value::Hole, "a colour"),
    ] {
        let refused = set(&base, value.clone()).err().map(|error| error.to_string());
        let refused = refused.ok_or_else(|| format!("{value:?} was accepted"))?;
        assert!(
            refused.contains(ADDRESS) && refused.contains(wanted),
            "{value:?}: {refused}"
        );
    }
    // a colour for a number is refused naming the address too
    let number = "glow.look.point_light.gain";
    for value in [Value::from("#336699"), Value::Rgb([0.2, 0.4, 0.6])] {
        let refused = base
            .with_values(&Values::new().set(number, value))
            .err()
            .map(|error| error.to_string())
            .ok_or("a colour set as a number")?;
        assert!(refused.contains(number) && refused.contains("a number"), "{refused}");
    }
    // all or nothing: a good number beside a bad colour changes nothing
    let both = Values::new().set("glow.opacity", 0.5).set(ADDRESS, "nope");
    assert!(base.with_values(&both).is_err());
    assert_eq!(base.parameters(), lit()?.parameters());
    Ok(())
}

#[test]
fn an_unset_optional_colour_is_listed_as_none_and_takes_a_colour() -> Outcome {
    let base = scene(
        24,
        16,
        [Layer::picture(varied(12, 8)?)
            .look(r#"{"steps": [{"effect": "bloom", "params": {"radius": 4.0, "threshold": 0.2}}]}"#)?
            .named("halo")?],
    )?;
    let found = parameter(&base, "halo.look.bloom.color")?;
    assert_eq!(
        (found.kind, &found.value, found.nullable),
        (Kind::Color, &Value::Hole, true)
    );
    // none for none changes nothing, a colour sets it, and it is never unset
    let own = base.with_values(&Values::new().set("halo.look.bloom.color", Value::Hole))?;
    assert!(same_look(&render(&own)?, &render(&base)?));
    let tinted = base.with_values(&Values::new().set("halo.look.bloom.color", "#ff0000"))?;
    assert!(!same_look(&render(&tinted)?, &render(&base)?));
    assert!(
        tinted
            .with_values(&Values::new().set("halo.look.bloom.color", Value::Hole))
            .is_err()
    );
    Ok(())
}

#[test]
fn a_layers_own_colour_still_works_beside_a_looks() -> Outcome {
    let base = lit()?;
    let both = base.with_values(&Values::new().set("ground.color", "#ff0000").set(ADDRESS, "#336699"))?;
    let by_hand = set(
        &base.with_values(&Values::new().set("ground.color", "#ff0000"))?,
        "#336699",
    )?;
    assert!(same_look(&render(&both)?, &render(&by_hand)?));
    assert!(!same_look(&render(&both)?, &render(&set(&base, "#336699")?)?));
    assert_eq!(
        parameter(&both, "ground.color")?.value,
        Value::Text("#ff0000".to_owned())
    );
    Ok(())
}

/// A changed colour of a look is a change for the cache and an unchanged one is a hit: the counting kind is drawn once for the
/// same colour set again, and the picture of a new colour is the cold scene's.
#[test]
fn the_cache_sees_a_changed_look_colour_and_serves_an_unchanged_one() -> Outcome {
    let kind = Counting::new(0.4);
    let under = |colour: &str| -> Result<Scene, Error> {
        let bloom = format!(
            r#"{{"steps": [{{"effect": "bloom", "params": {{"radius": 4.0, "threshold": 0.1, "color": "{colour}"}}}}]}}"#
        );
        scene(
            24,
            16,
            [Layer::make(Arc::clone(&kind) as _).look(&bloom)?.named("glow")?],
        )
    };
    let base = under("#ffe2b8")?;
    let first = render(&base)?;
    assert_eq!(kind.draws(), 1);
    let set = |colour: &str| base.with_values(&Values::new().set("glow.look.bloom.color", colour));
    let again = set("#ffe2b8")?;
    assert!(same_look(&render(&again)?, &first));
    assert_eq!(kind.draws(), 1, "the same colour is a hit");
    let changed = render(&set("#336699")?)?;
    assert!(!same_look(&changed, &first), "a new colour is a new picture");
    assert!(
        same_look(&changed, &render(&under("#336699")?)?),
        "and the cold scene's"
    );
    Ok(())
}
