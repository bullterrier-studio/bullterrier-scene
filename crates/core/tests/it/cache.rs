//! The cache (C7): what a layer drew is kept, and asked for again only when something it depends on changed. The one
//! promise is that the cache changes no picture. Its oracle is a cold scene (one built afresh, sharing no cache): every
//! address of the scenes below is changed on a scene whose cache has been used, and the picture is compared bit for bit with
//! the cold scene's. A counting layer kind shows what is not drawn again.

use std::path::Path;
use std::sync::Arc;

use bullterrier_scene::{
    Blend, Error, Fit, Kind, Layer, Map, Mask, Parameter, Place, Raster, Scene, Text, Use, Value, Values,
};

use crate::common::counting::Counting;
use crate::common::{Outcome, made, pixels, scene, varied};
use crate::parameters::drawable;
use crate::saved::{Files, rich, write_picture};

const SIZE: u32 = 40;

/// A small whole number as a float.
fn step(n: usize) -> f32 {
    f32::from(u8::try_from(n).unwrap_or(0))
}

fn same_bits(left: &Raster, right: &Raster) -> bool {
    (left.width(), left.height()) == (right.width(), right.height())
        && pixels(left)
            .zip(pixels(right))
            .all(|(l, r)| l.map(f32::to_bits) == r.map(f32::to_bits))
}

/// Two renders agree: the same picture bit for bit, or the same refusal.
fn agree(warm: &Result<Raster, Error>, cold: &Result<Raster, Error>) -> bool {
    match (warm, cold) {
        (Ok(warm), Ok(cold)) => same_bits(warm, cold),
        (Err(warm), Err(cold)) => warm == cold,
        _ => false,
    }
}

/// A valid value of `parameter` that is not the one it holds: `None` for a hole. A picture file is given `other`.
fn another(parameter: &Parameter, other: &Path) -> Option<Value> {
    Some(match (parameter.kind, &parameter.value) {
        (Kind::Bool, Value::Bool(on)) => Value::Bool(!on),
        (Kind::Choice, Value::Text(now)) => Value::Text(parameter.choices.iter().find(|name| *name != now)?.clone()),
        (Kind::Color, Value::Text(now)) => Value::Text(if now == "#13a7c5" { "#c5a713" } else { "#13a7c5" }.to_owned()),
        (Kind::Text, Value::Text(now)) => Value::Text(format!("{now}x")),
        (Kind::Float | Kind::Int, Value::Number(now)) => {
            let (low, high) = (parameter.min?, parameter.max?);
            let moved = if parameter.unit.as_deref() == Some("ratio") {
                (now * 1.37).clamp(low, high)
            } else {
                let step = if high - low <= 360.0 { 0.17 * (high - low) } else { 7.5 };
                if now + step <= high { now + step } else { now - step }
            };
            Value::Number(if parameter.kind == Kind::Int {
                moved.round()
            } else {
                moved
            })
        }
        (Kind::Picture, Value::Picture(held)) => Value::Picture(Arc::new(
            Raster::rgba(
                held.width(),
                held.height(),
                held.pixels().iter().map(|value| value * 0.5 + 0.3).collect(),
            )
            .ok()?,
        )),
        (Kind::Picture, Value::Map(held)) => Value::Map(Arc::new(
            Map::new(
                held.width(),
                held.height(),
                held.values().iter().map(|v| 1.0 - v).collect(),
            )
            .ok()?,
        )),
        (Kind::Picture, Value::File(_)) => Value::File(other.to_path_buf()),
        _ => return None,
    })
}

/// Every address of the scene `fresh` makes, changed on a scene that has drawn before, draws what a scene made afresh draws.
fn every_address_draws_as_a_cold_scene_does(
    fresh: impl Fn() -> Result<Scene, Box<dyn std::error::Error>>,
    other: &Path,
    least_shown: usize,
) -> Outcome {
    let used = fresh()?;
    let base = used.render(SIZE);
    assert!(agree(&base, &fresh()?.render(SIZE)));
    let (mut mismatched, mut tried, mut seen) = (Vec::new(), 0, 0);
    for parameter in used.parameters() {
        let Some(value) = another(&parameter, other) else {
            continue;
        };
        let values = Values::new().set(parameter.address.clone(), value);
        // the used scene has drawn the scene and every change before this one, and shares one cache with its derived scenes
        let warm = used.with_values(&values).and_then(|changed| changed.render(SIZE));
        let cold = fresh()?.with_values(&values).and_then(|changed| changed.render(SIZE));
        tried += 1;
        // the oracle sees only what shows: most of the changes must show, or the scene hides what it is meant to test
        seen += usize::from(!agree(&warm, &base));
        if !agree(&warm, &cold) {
            mismatched.push(parameter.address);
        }
    }
    assert!(
        tried > 10 && seen * least_shown >= tried,
        "{seen} of {tried} changed addresses changed the picture"
    );
    assert_eq!(mismatched, Vec::<String>::new());
    Ok(())
}

/// A poster in which every layer shows through the ones above it: a looked picture with its map and masks, a drawing with Params,
/// type, a group, a reference with a look and a place, and a feathered mask layer.
fn poster() -> Result<Scene, Box<dyn std::error::Error>> {
    const GRADE: &str = r#"{"steps": [{"effect": "grade", "params": {"exposure": 0.5}}]}"#;
    const SLIDER: &str = r##"{"params":[{"name":"x","default":0,"min":-10,"max":30,"unit":"px","meaning":"slide"}],"root":[{"stack":{"items":[[{"path":"M0 0 L18 0 L18 12 L0 12 Z"}]],"size":[60,40]}},{"translate":{"dx":{"param":"x"},"dy":0}},{"fill":{"color":"#ff0000"}}],"version":1}"##;
    let matte = Map::new(
        12,
        8,
        (0..96)
            .map(|at| f32::from(u8::try_from(at).unwrap_or(0)) / 96.0)
            .collect(),
    )?;
    let badge = scene(
        20,
        10,
        [Layer::text(
            "Hi",
            &Text::new("bricolage_grotesque").color("#ffffff").option("size", 8.0),
        )?
        .named("label")?],
    )?;
    Ok(scene(
        60,
        40,
        [
            Layer::fill("#204080")?.named("ground")?,
            Layer::picture(varied(12, 8)?)
                .map("subject", matte)?
                .named("photo")?
                .fit([0.0, 0.0, 48.0, 32.0], Fit::Cover, [0.5, 0.5])?
                .look(GRADE)?
                .mask("subject")?
                .mask(Layer::fill("#ffffff")?.fit([2.0, 2.0, 40.0, 24.0], Fit::Stretch, [0.5, 0.5])?)?
                .opacity(0.8)?,
            Layer::drawing(SLIDER)?.named("logo")?.opacity(0.7)?,
            Layer::text(
                "Title",
                &Text::new("bricolage_grotesque").color("#ffffff").option("size", 14.0),
            )?
            .named("title")?
            .place(Place::new().dx(5.0).dy(30.0))?,
            Layer::scene(badge)
                .named("badge")?
                .place(Place::new().dx(35.0).dy(25.0))?
                .opacity(0.9)?,
            Layer::of("photo")
                .named("glow")?
                .look(GRADE)?
                .place(Place::new().dx(6.0).dy(4.0).scale(0.8))?
                .mask(Layer::fill("#808080")?.place(Place::new().dx(4.0))?)?
                .opacity(0.5)?,
        ],
    )?)
}

#[test]
fn every_address_of_a_poster_where_every_layer_shows_draws_as_a_cold_scene_does() -> Outcome {
    let files = Files::new()?;
    write_picture(&files.path("other.png"), &varied(12, 8)?)?;
    every_address_draws_as_a_cold_scene_does(poster, &files.path("other.png"), 2)
}

#[test]
fn every_address_of_the_scene_of_every_kind_draws_as_a_cold_scene_does() -> Outcome {
    let files = Files::new()?;
    write_picture(&files.path("other.png"), &varied(12, 8)?)?;
    // the band and `layer7` (a white fill) cover all that is below them: made see-through, each layer shows
    let shown = || -> Result<Scene, Box<dyn std::error::Error>> {
        Ok(drawable()?.with_values(&Values::new().set("layer7.opacity", 0.0).set("band.opacity", 0.0))?)
    };
    every_address_draws_as_a_cold_scene_does(shown, &files.path("other.png"), 6)
}

#[test]
fn every_address_of_the_scene_with_every_part_of_the_document_draws_as_a_cold_scene_does() -> Outcome {
    let files = Files::new()?;
    write_picture(
        &files.path("other.png"),
        &made(12, 8, |x, y| [step(x) / 12.0, step(y) / 8.0, 0.5, 1.0])?,
    )?;
    let filled = || -> Result<Scene, Box<dyn std::error::Error>> {
        Ok(rich(&files)?.with_values(
            &Values::new()
                .set("hole.picture", files.path("photo.png"))
                .set("photo.map.later", files.path("matte.png")),
        )?)
    };
    every_address_draws_as_a_cold_scene_does(filled, &files.path("other.png"), 3)
}

#[test]
fn every_address_of_the_cover_template_draws_as_a_cold_scene_does() -> Outcome {
    let files = Files::new()?;
    write_picture(&files.path("other.png"), &varied(12, 8)?)?;
    let template =
        || -> Result<Scene, Box<dyn std::error::Error>> { Ok(crate::cover_template::build(&files.path("photo.png"))?) };
    every_address_draws_as_a_cold_scene_does(template, &files.path("other.png"), 3)
}

/// A static layer of the counting kind under a headline, and the kind to ask how often it was drawn.
fn headline_over(kind: &Arc<Counting>) -> Result<Scene, Error> {
    scene(
        100,
        50,
        [
            Layer::make(kind.clone()).named("static")?,
            Layer::text(
                "Hi",
                &Text::new("bricolage_grotesque").color("#ffffff").option("size", 20),
            )?
            .named("headline")?
            .place(Place::new().dy(30.0))?,
        ],
    )
}

#[test]
fn a_layer_that_did_not_change_is_drawn_once_however_often_the_scene_is_drawn_and_changed_elsewhere() -> Outcome {
    let kind = Counting::new(0.2);
    let scene_ = headline_over(&kind)?;
    scene_.render(100)?;
    for dx in [10.0, 20.5, 30.0] {
        let frame = scene_.with_values(&Values::new().set("headline.dx", dx))?;
        frame.render(100)?;
        frame.render(100)?;
    }
    assert_eq!(kind.draws(), 1);
    // a value of the static layer that does not change what it draws: the layer is moved, turned, made see-through
    for (address, value) in [
        ("static.dx", 7.0),
        ("static.dy", -3.5),
        ("static.rotate", 20.0),
        ("static.opacity", 0.5),
        ("static.skew_x", 12.0),
        ("static.aspect", 1.5),
    ] {
        scene_.with_values(&Values::new().set(address, value))?.render(100)?;
    }
    assert_eq!(
        kind.draws(),
        1,
        "a layer that only moves is warped again, not drawn again"
    );
    Ok(())
}

#[test]
fn a_layer_is_drawn_again_for_what_it_draws_from_and_not_for_the_rest() -> Outcome {
    let kind = Counting::new(0.2);
    let scene_ = headline_over(&kind)?;
    scene_.render(100)?;
    scene_
        .with_values(&Values::new().set("static.scale", 1.5))?
        .render(100)?;
    assert_eq!(kind.draws(), 2, "its scale is what it is drawn at");
    scene_.render(50)?;
    assert_eq!(kind.draws(), 3, "its size is what it is drawn at");
    scene_.render(100)?;
    scene_.render(50)?;
    scene_
        .with_values(&Values::new().set("static.scale", 1.5))?
        .render(100)?;
    assert_eq!(
        kind.draws(),
        3,
        "what was drawn is kept for the sizes and scales that come back"
    );
    // another option is another layer, and the same options in another object are the same layer
    let other = Counting::new(0.4);
    headline_over(&other)?.render(100)?;
    assert_eq!(other.draws(), 1);
    let twin = Counting::new(0.2);
    let again = headline_over(&twin)?;
    again.render(100)?;
    assert_eq!(twin.draws(), 1, "a scene made apart shares nothing");
    Ok(())
}

#[test]
fn a_mask_layer_used_by_several_layers_and_a_reference_that_only_moves_are_drawn_once() -> Outcome {
    let kind = Counting::new(0.6);
    let users = ["a", "b", "c"].map(|name| Layer::fill("#336699").and_then(|fill| fill.named(name)));
    let mut layers = Vec::new();
    for user in users {
        layers.push(user?.mask(Layer::make(kind.clone()))?);
    }
    scene(100, 50, layers)?.render(100)?;
    assert_eq!(kind.draws(), 1, "one mask layer, one plane, three users");

    let shown = Counting::new(0.7);
    let with_reference = scene(
        100,
        50,
        [
            Layer::make(shown.clone()).named("base")?,
            Layer::of("base").named("echo")?,
        ],
    )?;
    let moved = with_reference.with_values(&Values::new().set("echo.dx", 10.0).set("echo.rotate", 15.0))?;
    moved.render(100)?;
    assert_eq!(
        shown.draws(),
        1,
        "a reference whose extra place leaves the density shares the raster"
    );
    with_reference
        .with_values(&Values::new().set("echo.scale", 0.5))?
        .render(100)?;
    assert_eq!(shown.draws(), 2, "one that zooms is drawn at its own density");
    Ok(())
}

#[test]
fn what_is_drawn_inside_a_group_is_kept_when_the_group_is_drawn_again() -> Outcome {
    let kind = Counting::new(0.3);
    let inner = scene(40, 20, [Layer::make(kind.clone()).named("tile")?])?;
    let outer = scene(
        100,
        50,
        [Layer::scene(inner).named("badge")?.place(Place::new().dx(10.0))?],
    )?;
    outer.render(100)?;
    // by a whole pixel: the group's raster is the same; by a fraction: the group is drawn at another phase, its layer is not
    outer.with_values(&Values::new().set("badge.dx", 25.0))?.render(100)?;
    outer.with_values(&Values::new().set("badge.dx", 25.5))?.render(100)?;
    assert_eq!(kind.draws(), 1);
    Ok(())
}

#[test]
fn a_file_that_changed_is_read_again_and_one_that_did_not_is_the_same_picture() -> Outcome {
    let files = Files::new()?;
    let path = files.path("watched.png");
    let layer = || -> Result<Scene, Error> {
        scene(
            12,
            8,
            [Layer::picture_file(&path).look(r#"{"steps": [{"effect": "grade", "params": {"exposure": 0.5}}]}"#)?],
        )
    };
    write_picture(&path, &varied(12, 8)?)?;
    let used = layer()?;
    let first = used.render(24)?;
    assert!(same_bits(&first, &used.render(24)?));
    // another picture of another size is another file, whatever the path says
    write_picture(&path, &varied(16, 10)?)?;
    let second = used.render(24)?;
    assert!(!same_bits(&first, &second));
    assert!(same_bits(&second, &layer()?.render(24)?));
    Ok(())
}

#[test]
fn a_picture_swapped_by_a_value_is_the_other_picture_and_equal_pixels_are_the_same_picture() -> Outcome {
    let template = scene(
        12,
        8,
        [Layer::picture_hole()
            .named("photo")?
            .look(r#"{"steps": [{"effect": "grade", "params": {"exposure": 0.5}}]}"#)?],
    )?;
    let (one, two) = (
        varied(12, 8)?,
        made(12, 8, |x, y| [step(y) / 8.0, step(x) / 12.0, 0.1, 1.0])?,
    );
    let fill = |picture: &Raster| template.with_values(&Values::new().set("photo.picture", picture.clone()));
    let first = fill(&one)?.render(24)?;
    let second = fill(&two)?.render(24)?;
    assert!(!same_bits(&first, &second));
    assert!(same_bits(&first, &fill(&one.clone())?.render(24)?));
    assert!(same_bits(
        &first,
        &template
            .with_values(&Values::new().set("photo.picture", one))?
            .render(24)?
    ));
    Ok(())
}

#[test]
fn equal_scenes_are_equal_whatever_their_caches_hold() -> Outcome {
    let (drawn, never) = (headline_over(&Counting::new(0.2))?, headline_over(&Counting::new(0.2))?);
    drawn.render(100)?;
    assert_eq!(drawn, never);
    assert_eq!(drawn.parameters(), never.parameters());
    let moved = drawn.with_values(&Values::new().set("headline.dx", 5.0))?;
    moved.render(100)?;
    assert_ne!(moved, drawn);
    Ok(())
}

#[test]
fn scenes_that_share_a_cache_drawn_by_several_threads_draw_what_cold_scenes_draw() -> Outcome {
    let used = drawable()?;
    let frames: Vec<Scene> = (0..6)
        .map(|n| {
            used.with_values(
                &Values::new()
                    .set("title.dx", f64::from(n) * 2.5)
                    .set("band.opacity", 0.5 + f64::from(n) / 12.0),
            )
        })
        .collect::<Result<_, _>>()?;
    let cold: Vec<Raster> = (0..6)
        .map(|n| {
            drawable()?
                .with_values(
                    &Values::new()
                        .set("title.dx", f64::from(n) * 2.5)
                        .set("band.opacity", 0.5 + f64::from(n) / 12.0),
                )?
                .render(SIZE)
                .map_err(Into::into)
        })
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
    std::thread::scope(|threads| {
        let handles: Vec<_> = (0..4_usize)
            .map(|which| {
                let (frames, cold) = (&frames, &cold);
                threads.spawn(move || {
                    (0..12_usize).all(|turn| {
                        let n = (turn * (which + 1)) % 6;
                        frames[n].render(SIZE).is_ok_and(|drawn| same_bits(&drawn, &cold[n]))
                    })
                })
            })
            .collect();
        for handle in handles {
            assert!(
                handle.join().unwrap_or(false),
                "a thread drew another picture than a cold scene"
            );
        }
    });
    Ok(())
}

#[test]
fn a_cropped_and_placed_layer_of_a_kind_draws_as_it_does_cold() -> Outcome {
    let build = |kind: &Arc<Counting>| -> Result<Scene, Error> {
        scene(
            100,
            50,
            [Layer::make(kind.clone())
                .fit([10.0, 10.0, 40.0, 40.0], Fit::Cover, [0.5, 0.5])?
                .place(Place::new().dx(5.0))?],
        )
    };
    let used = build(&Counting::new(0.5))?;
    used.render(100)?;
    assert!(agree(&used.render(100), &build(&Counting::new(0.5))?.render(100)));
    Ok(())
}

/// What a layer is drawn from that no address names: `fit`, a perspective, and what a mask layer or a mask by a map reads,
/// blurs and inverts. Each variant is added to a scene that has drawn the layer before and must draw as a cold scene does.
#[test]
fn what_no_address_names_is_in_the_key_too() -> Outcome {
    type Variant = fn(Layer) -> Result<Layer, Error>;
    let kind = Counting::new(0.45);
    let table: Vec<(&str, Variant)> = vec![
        ("fit cover", |l| l.fit([0.0, 0.0, 20.0, 14.0], Fit::Cover, [0.5, 0.5])),
        ("fit contain", |l| {
            l.fit([0.0, 0.0, 20.0, 14.0], Fit::Contain, [0.5, 0.5])
        }),
        ("fit stretch", |l| {
            l.fit([0.0, 0.0, 20.0, 14.0], Fit::Stretch, [0.5, 0.5])
        }),
        ("fit align", |l| l.fit([0.0, 0.0, 20.0, 14.0], Fit::Cover, [0.0, 1.0])),
        ("fit box", |l| l.fit([2.0, 1.0, 21.0, 15.0], Fit::Cover, [0.5, 0.5])),
        ("perspective", |l| {
            l.perspective([[0.0, 0.0], [20.0, 2.0], [22.0, 14.0], [1.0, 15.0]])
        }),
        ("map mask", |l| l.mask("subject")),
        ("map mask feathered", |l| l.mask(Mask::new("subject").feather(1.5))),
        ("map mask inverted", |l| l.mask(Mask::new("subject").invert())),
        ("layer mask", |l| {
            l.mask(Layer::fill("#ffffff")?.fit([2.0, 2.0, 10.0, 8.0], Fit::Stretch, [0.5, 0.5])?)
        }),
        ("layer mask feathered", |l| {
            l.mask(
                Mask::new(Layer::fill("#ffffff")?.fit([2.0, 2.0, 10.0, 8.0], Fit::Stretch, [0.5, 0.5])?).feather(2.0),
            )
        }),
        ("layer mask feathered more", |l| {
            l.mask(
                Mask::new(Layer::fill("#ffffff")?.fit([2.0, 2.0, 10.0, 8.0], Fit::Stretch, [0.5, 0.5])?).feather(4.0),
            )
        }),
        ("layer mask inverted", |l| {
            l.mask(Mask::new(Layer::fill("#ffffff")?.fit([2.0, 2.0, 10.0, 8.0], Fit::Stretch, [0.5, 0.5])?).invert())
        }),
        ("layer mask by luminance", |l| {
            l.mask(
                Mask::new(Layer::fill("#808080")?.fit([2.0, 2.0, 10.0, 8.0], Fit::Stretch, [0.5, 0.5])?)
                    .using(Use::Luminance),
            )
        }),
        ("layer mask with its own opacity", |l| {
            l.mask(
                Layer::fill("#ffffff")?
                    .fit([2.0, 2.0, 10.0, 8.0], Fit::Stretch, [0.5, 0.5])?
                    .opacity(0.5)?,
            )
        }),
        ("a look", |l| {
            l.look(r#"{"steps": [{"effect": "grade", "params": {"exposure": 0.7}}]}"#)
        }),
        ("two looks", |l| {
            l.look(r#"{"steps": [{"effect": "grade", "params": {"exposure": 0.7}}]}"#)?
                .look(r#"{"steps": [{"effect": "grade", "params": {"contrast": 0.4}}]}"#)
        }),
        ("a blend", |l| Ok(l.blend(Blend::Multiply))),
        ("light", |l| Ok(l.as_light(true))),
    ];
    let picture =
        || -> Result<Layer, Error> { Layer::picture(varied(12, 8)?).map("subject", Map::new(12, 8, vec![0.25; 96])?) };
    let under =
        || -> Result<Vec<Layer>, Error> { Ok(vec![Layer::make(kind.clone()).named("under")?, picture()?.named("v")?]) };
    let used = scene(24, 16, under()?)?;
    used.render(48)?;
    for (name, variant) in table {
        // the variant joins the used scene by `add`, which keeps its cache, and a scene made afresh by the same layers
        let layer = variant(picture()?)?.named("w")?;
        let warm = used.add([layer.clone()]).and_then(|warm| warm.render(48));
        let cold = scene(24, 16, [under()?, vec![layer]].concat()).and_then(|cold| cold.render(48));
        assert!(agree(&warm, &cold), "{name}");
    }
    Ok(())
}
