//! Layer kinds by name: the rules for getting into a registry, the JSON form of a layer made through one, the cache of what
//! it draws, and the contract every registered kind keeps (found in the registry, so a new kind needs no registration here).

use std::path::Path;
use std::sync::Arc;

use bullterrier_scene::{Error, Layer, LayerKind, LayerKindFactory, LayerKinds, Raster, Scene};
use serde_json::{Value, json};

use crate::common::counting::{Counting, CountingFactory};
use crate::common::{Outcome, scene};

/// `ceil(length * scale)`, a product within 1e-12 of a whole number counting as that number.
#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a test size")]
fn pixels(length: f64, scale: f64) -> usize {
    ((length * scale) * (1.0 - 1e-12)).ceil().max(0.0) as usize
}

/// A kind whose picture is a checkerboard of cells `period` design pixels wide, red on the even cells.
struct Stripes(f64);

impl LayerKind for Stripes {
    fn name(&self) -> &'static str {
        "stripes"
    }

    fn options(&self) -> Value {
        json!({ "period": self.0 })
    }

    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "a test size"
    )]
    fn draw(&self, area: [f64; 4], scale: [f64; 2]) -> Result<Raster, Error> {
        let (columns, rows) = (pixels(area[2], scale[0]), pixels(area[3], scale[1]));
        let cell = |origin: f64, step: usize, scale: f64| ((origin + step as f64 / scale) / self.0).floor() as i64;
        let mut values = Vec::with_capacity(columns * rows * 4);
        for row in 0..rows {
            for column in 0..columns {
                let even = (cell(area[0], column, scale[0]) + cell(area[1], row, scale[1])) % 2 == 0;
                values.extend([if even { 1.0 } else { 0.0 }, 0.0, 0.5, 1.0]);
            }
        }
        Raster::rgba(columns, rows, values)
    }
}

struct StripesFactory;

impl LayerKindFactory for StripesFactory {
    fn name(&self) -> &'static str {
        "stripes"
    }

    fn identity(&self) -> &'static str {
        "stripes-factory"
    }

    fn create(&self, options: &Value) -> Result<Arc<dyn LayerKind>, Error> {
        match options.get("period").and_then(Value::as_f64) {
            Some(period) if period > 0.0 => Ok(Arc::new(Stripes(period))),
            _ => Err(Error::Invalid {
                address: "period".to_owned(),
                message: format!("a positive number, got {options}"),
            }),
        }
    }
}

/// A factory under any name, to try the rules of the registry; it makes the counting kind.
struct Named(String, &'static str);

impl LayerKindFactory for Named {
    fn name(&self) -> &str {
        &self.0
    }

    fn identity(&self) -> &str {
        self.1
    }

    fn create(&self, options: &Value) -> Result<Arc<dyn LayerKind>, Error> {
        CountingFactory::new("x").create(options)
    }
}

fn registry() -> Result<(LayerKinds, Arc<CountingFactory>), Error> {
    let counting = CountingFactory::new("counting-factory");
    let mut kinds = LayerKinds::new();
    kinds.register(counting.clone())?;
    kinds.register(Arc::new(StripesFactory))?;
    Ok((kinds, counting))
}

/// The refusal of `name` by the registry (its message), or `None` if it was accepted.
fn refused_by(kinds: &mut LayerKinds, name: &str, identity: &'static str) -> Option<String> {
    match kinds.register(Arc::new(Named(name.to_owned(), identity))) {
        Err(Error::Invalid { address, message }) if address == "kind" => Some(message),
        Err(other) => unreachable!("{other:?}"),
        Ok(()) => None,
    }
}

#[test]
fn a_name_is_refused_unless_it_is_lower_case_letters_digits_and_the_marks_of_a_name_and_not_a_scene_key() {
    let (long, huge) = ("a".repeat(65), "a".repeat(1 << 20));
    for bad in [
        "",
        "Upper",
        "has space",
        "-leading",
        ".leading",
        "_leading",
        "ünï",
        "a/b",
        &long,
        &huge,
        "picture",
        "drawing",
        "text",
        "fill",
        "scene",
        "of",
        "make",
    ] {
        let mut kinds = LayerKinds::new();
        let message = refused_by(&mut kinds, bad, "x");
        assert!(message.is_some(), "{:?} was accepted", &bad[..bad.len().min(20)]);
        assert!(
            message.is_some_and(|text| text.len() < 400),
            "a name is cut in the message"
        );
        assert_eq!(kinds.names(), Vec::<&str>::new());
    }
    let longest = "a".repeat(64);
    for good in ["a", "0", "my.kind-2_x", &longest, "pictures", "text2"] {
        assert!(
            refused_by(&mut LayerKinds::new(), good, "x").is_none(),
            "{good} was refused"
        );
    }
}

#[test]
fn the_same_identity_replaces_a_kind_and_another_may_not_take_its_name() -> Outcome {
    let (mut kinds, _) = registry()?;
    let again = CountingFactory::new("counting-factory");
    kinds.register(again.clone())?;
    kinds.make("counting", &json!({"tone": 0.5}))?;
    assert_eq!((again.made().len(), kinds.names()), (1, vec!["counting", "stripes"]));
    let message = refused_by(&mut kinds, "counting", "somebody-else").ok_or("a taken name was given away")?;
    assert!(
        message.contains("counting") && message.contains("counting-factory"),
        "{message}"
    );
    kinds.make("counting", &json!({"tone": 0.5}))?;
    assert_eq!(again.made().len(), 2, "the first kind still holds the name");
    Ok(())
}

#[test]
fn an_unknown_kind_lists_the_kinds_and_the_nearest() -> Outcome {
    let (kinds, _) = registry()?;
    let Err(Error::Invalid { address, message }) = kinds.make("countin", &json!({})) else {
        return Err("an unknown kind was made".into());
    };
    assert_eq!(address, "kind");
    assert!(
        message.contains("counting, stripes") && message.contains("did you mean counting"),
        "{message}"
    );
    let Err(Error::Invalid { message, .. }) = LayerKinds::new().make("x", &json!({})) else {
        return Err("a kind of an empty registry was made".into());
    };
    assert!(message.contains("none"), "{message}");
    // a kind of a megabyte is named in a line
    let Err(Error::Invalid { message, .. }) = kinds.make(&"k".repeat(1 << 20), &json!({})) else {
        return Err("a huge name was made".into());
    };
    assert!(message.len() < 300, "{}", message.len());
    Ok(())
}

#[test]
fn options_the_factory_refuses_are_named_and_a_factory_of_another_name_is_refused() -> Outcome {
    let (mut kinds, _) = registry()?;
    let Err(Error::Invalid { address, message }) = kinds.make("counting", &json!({"tone": 2})) else {
        return Err("a tone of 2 was accepted".into());
    };
    assert_eq!(address, "options");
    assert!(message.contains("\"tone\":2"), "{message}");
    kinds.register(Arc::new(Named("liar".to_owned(), "x")))?;
    assert!(matches!(
        kinds.make("liar", &json!({"tone": 0.1})),
        Err(Error::Invalid { address, message }) if address == "kind" && message.contains("liar") && message.contains("counting")
    ));
    Ok(())
}

fn picture_bits(scene: &Scene, size: u32) -> Result<Vec<u32>, Error> {
    Ok(scene
        .render(size)?
        .pixels()
        .iter()
        .map(|value| value.to_bits())
        .collect())
}

fn plugged(kinds: &LayerKinds) -> Result<Scene, Error> {
    scene(
        40,
        20,
        [
            Layer::fill("#102030")?,
            kinds.make("stripes", &json!({"period": 3.5}))?.opacity(0.75)?,
            kinds
                .make("counting", &json!({"tone": 0.25}))?
                .named("tone")?
                .opacity(0.5)?,
        ],
    )
}

#[test]
fn a_scene_with_layers_made_by_name_is_written_and_read_back_equal_and_draws_the_same() -> Outcome {
    let (kinds, _) = registry()?;
    let one = plugged(&kinds)?;
    let text = one.to_json()?;
    let document: Value = serde_json::from_str(&text)?;
    assert_eq!(
        document["layers"][1],
        json!({"make": {"kind": "stripes", "options": {"period": 3.5}}, "opacity": 0.75})
    );
    let (again, _) = registry()?;
    let back = Scene::from_json_with(&text, Path::new(""), &again)?;
    assert_eq!(back, one);
    assert_eq!(back.to_json()?, text);
    for size in [40, 81] {
        assert_eq!(picture_bits(&back, size)?, picture_bits(&one, size)?);
    }
    Ok(())
}

#[test]
fn a_document_naming_a_kind_the_registry_lacks_is_a_format_error_at_the_kind() -> Outcome {
    let (kinds, _) = registry()?;
    let text = plugged(&kinds)?.to_json()?;
    let swapped = text.replace("\"stripes\"", "\"stripez\"");
    for table in [LayerKinds::new(), kinds.clone()] {
        let Err(Error::Format { pointer, message }) = Scene::from_json_with(&swapped, Path::new(""), &table) else {
            return Err("an unknown kind was read".into());
        };
        assert_eq!(pointer, "/layers/1/make/kind");
        assert!(
            message.contains("stripez") && message.contains("the kinds are"),
            "{message}"
        );
    }
    // the default reading holds no kinds
    assert!(matches!(
        Scene::from_json(&text, Path::new("")),
        Err(Error::Format { pointer, .. }) if pointer == "/layers/1/make/kind"
    ));
    Ok(())
}

#[test]
fn a_document_with_options_the_factory_refuses_or_a_made_layer_of_the_wrong_shape_is_a_format_error() -> Outcome {
    let (kinds, _) = registry()?;
    let text = plugged(&kinds)?.to_json()?;
    let cases = [
        (
            text.replace("\"period\":3.5", "\"period\":-1"),
            "/layers/1/make/options",
        ),
        (text.replace("\"kind\":\"stripes\",", ""), "/layers/1/make"),
        (
            text.replace("\"kind\":\"stripes\"", "\"kind\":7"),
            "/layers/1/make/kind",
        ),
        (
            text.replace("\"kind\":\"stripes\"", "\"kind\":\"stripes\",\"extra\":1"),
            "/layers/1/make/extra",
        ),
        (
            text.replace("{\"make\":{", "{\"fill\":\"#fff\",\"make\":{"),
            "/layers/1",
        ),
    ];
    for (document, pointer) in cases {
        let refused = Scene::from_json_with(&document, Path::new(""), &kinds);
        assert!(
            matches!(&refused, Err(Error::Format { pointer: at, .. }) if at == pointer),
            "{pointer}: {refused:?}"
        );
    }
    // a megabyte of options is read once and refused by the factory in a line
    let huge = text.replace("\"period\":3.5", &format!("\"period\":\"{}\"", "9".repeat(1 << 20)));
    assert!(matches!(
        Scene::from_json_with(&huge, Path::new(""), &kinds),
        Err(Error::Format { pointer, .. }) if pointer == "/layers/1/make/options"
    ));
    Ok(())
}

#[test]
fn a_layer_given_to_layer_make_has_no_json_form_and_says_why() -> Outcome {
    let refused = scene(10, 10, [Layer::make(Counting::new(0.1))])?.to_json();
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message }) if address.ends_with("make") && message.contains("registry")),
        "{refused:?}"
    );
    // equal to the layer of the same name and options made through a registry
    let (kinds, _) = registry()?;
    assert_eq!(
        scene(10, 10, [Layer::make(Counting::new(0.1))])?,
        scene(10, 10, [kinds.make("counting", &json!({"tone": 0.1}))?])?
    );
    Ok(())
}

#[test]
fn what_a_registered_kind_drew_is_kept_and_a_changed_option_is_never_served_stale() -> Outcome {
    let (kinds, counting) = registry()?;
    let base = Scene::new(40, 20)?;
    let with = |tone: f64| -> Result<Scene, Error> { base.add([kinds.make("counting", &json!({ "tone": tone }))?]) };
    let (first, second) = (with(0.2)?, with(0.6)?);
    let warm = picture_bits(&first, 80)?;
    assert_eq!(picture_bits(&first, 80)?, warm);
    let draws: Vec<usize> = counting.made().iter().map(|kind| kind.draws()).collect();
    assert_eq!(draws, [1, 0], "the second render of one scene asked the kind again");
    // the other tone shares the cache of `base`, and draws what a scene of its own draws
    let other = picture_bits(&second, 80)?;
    assert_ne!(other, warm);
    let cold = scene(40, 20, [Layer::make(Counting::new(0.6))])?;
    assert_eq!(other, picture_bits(&cold, 80)?);
    assert_eq!(
        picture_bits(&first, 80)?,
        warm,
        "the first tone is served again, not the second"
    );
    Ok(())
}

// --- the contract of every registered kind -----------------------------------------------------------------------------

/// Some options each registered kind accepts: a kind added to `registry` needs a row here, or the next test fails.
fn options_of(name: &str) -> Option<Value> {
    match name {
        "counting" => Some(json!({"tone": 0.4})),
        "stripes" => Some(json!({"period": 7.0})),
        _ => None,
    }
}

#[test]
fn every_registered_kind_has_options_to_try() -> Outcome {
    for name in registry()?.0.names() {
        assert!(options_of(name).is_some(), "{name}: add its options to `options_of`");
    }
    Ok(())
}

#[test]
fn every_registered_kind_answers_every_rectangle_and_scale_with_a_whole_straight_alpha_raster() -> Outcome {
    let (kinds, _) = registry()?;
    let rectangles = [
        [0.0, 0.0, 100.0, 50.0],
        [3.25, 1.5, 10.0, 7.5],
        [0.0, 0.0, 0.001, 0.001],
        [90.0, 40.0, 100.0, 100.0], // reaches beyond a canvas of 100 by 50
        [-5.0, -5.0, 8.0, 3.0],
    ];
    let scales = [
        [1.0, 1.0],
        [2.0, 2.0],
        [0.37, 0.37],
        [1.5, 1.500_000_1],
        [0.01, 0.01],
        [3.0, 2.999_999_9],
    ];
    for name in kinds.names() {
        let options = options_of(name).ok_or("a kind without options")?;
        let first = kinds.get(name)?.create(&options)?;
        assert_eq!(first.name(), name);
        // what a kind says of itself reads back into the same kind
        let again = kinds.get(name)?.create(&first.options())?;
        assert_eq!(again.options(), first.options());
        for area in rectangles {
            for scale in scales {
                let raster = first.draw(area, scale)?;
                assert_eq!(
                    (raster.width(), raster.height()),
                    (pixels(area[2], scale[0]), pixels(area[3], scale[1])),
                    "{name} {area:?} {scale:?}"
                );
                assert!(
                    raster.pixels().iter().all(|value| (0.0..=1.0).contains(value)),
                    "{name} {area:?} {scale:?}"
                );
                assert_eq!(raster, again.draw(area, scale)?, "{name}: not deterministic");
            }
        }
    }
    Ok(())
}
