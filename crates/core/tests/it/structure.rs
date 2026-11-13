//! What a scene and a layer refuse, and what a scene is as a value: immutable, deterministic, independent of any other.
//! Every checker has a good row and a bad row, and a bad row names what is wrong.

use bullterrier_scene::{Error, Layer, MAX_LAYERS, Scene};

use crate::common::{Outcome, all_exactly, scene};

/// The `Invalid` error `result` is, as its address and message together.
fn refusal<T>(result: Result<T, Error>) -> Result<String, String> {
    match result {
        Err(error @ Error::Invalid { .. }) => Ok(error.to_string()),
        Err(other) => Err(format!("not an Invalid error: {other:?}")),
        Ok(_) => Err("accepted".to_owned()),
    }
}

/// Each row: a value, whether it is accepted, and (for a refusal) a word the message must carry.
fn check<T>(what: &str, rows: Vec<(String, bool, &str, Result<T, Error>)>) -> Outcome {
    for (given, good, named, result) in rows {
        match (good, result) {
            (true, Ok(_)) => {}
            (true, Err(error)) => return Err(format!("{what} {given}: refused: {error}").into()),
            (false, result) => {
                let message = refusal(result).map_err(|why| format!("{what} {given}: {why}"))?;
                if !message.contains(named) {
                    return Err(format!("{what} {given}: `{message}` does not name `{named}`").into());
                }
            }
        }
    }
    Ok(())
}

#[test]
fn a_canvas_is_whole_pixels_from_1_to_100000() -> Outcome {
    let rows = [
        (1, 1, true),
        (100_000, 100_000, true),
        (0, 5, false),
        (5, 0, false),
        (100_001, 5, false),
        (5, 100_001, false),
    ];
    check(
        "canvas",
        rows.iter()
            .map(|&(width, height, good)| (format!("{width}x{height}"), good, "canvas", Scene::new(width, height)))
            .collect(),
    )
}

#[test]
fn an_opacity_is_from_0_to_1_and_a_number() -> Outcome {
    let rows = [
        (0.0, true),
        (1.0, true),
        (0.5, true),
        (-0.1, false),
        (1.1, false),
        (f64::NAN, false),
        (f64::INFINITY, false),
    ];
    check(
        "opacity",
        rows.iter()
            .map(|&(value, good)| {
                (
                    format!("{value}"),
                    good,
                    "opacity",
                    Layer::fill("#000000").and_then(|layer| layer.opacity(value)),
                )
            })
            .collect(),
    )
}

#[test]
fn a_colour_is_hash_and_six_or_eight_hex_digits() -> Outcome {
    let rows = [
        ("#ff00aa", true),
        ("#FF00AAcc", true),
        ("#000000", true),
        ("ff00aa", false),
        ("#fff", false),
        ("#ff00a", false),
        ("#ggg000", false),
        ("red", false),
        ("", false),
        ("#ff00aa00cc", false),
    ];
    check(
        "colour",
        rows.iter()
            .map(|&(text, good)| (format!("{text:?}"), good, "fill", Layer::fill(text)))
            .collect(),
    )
}

#[test]
fn a_name_is_letters_digits_and_underscores_starting_with_a_letter() -> Outcome {
    let rows = [
        ("photo", true),
        ("Title_2", true),
        ("a", true),
        ("2photo", false),
        ("_photo", false),
        ("my photo", false),
        ("naïve", false),
        ("", false),
        ("a.b", false),
    ];
    check(
        "name",
        rows.iter()
            .map(|&(name, good)| {
                (
                    format!("{name:?}"),
                    good,
                    "name",
                    Layer::fill("#000000").and_then(|layer| layer.named(name)),
                )
            })
            .collect(),
    )
}

#[test]
fn a_name_used_twice_is_refused_naming_both_layers() -> Outcome {
    let layers = [
        Layer::fill("#000000")?.named("ground")?,
        Layer::fill("#111111")?,
        Layer::fill("#222222")?.named("ground")?,
    ];
    let message = refusal(scene(4, 4, layers))?;
    assert!(message.contains("ground"), "{message}");
    assert!(message.contains("layer 1") && message.contains("layer 3"), "{message}");
    Ok(())
}

#[test]
fn a_name_used_in_an_earlier_add_is_taken_too() -> Outcome {
    let base = scene(4, 4, [Layer::fill("#000000")?.named("ground")?])?;
    let message = refusal(base.add([Layer::fill("#111111")?.named("ground")?]))?;
    assert!(
        message.contains("ground") && message.contains("layer 1") && message.contains("layer 2"),
        "{message}"
    );
    Ok(())
}

/// A layer without a name is `layer1`, `layer2`... by its place; `layer<n>` is a name only at place `n`.
#[test]
fn a_default_name_is_taken_only_at_its_own_place() -> Outcome {
    let at_place_two = scene(
        4,
        4,
        [Layer::fill("#000000")?, Layer::fill("#111111")?.named("layer2")?],
    );
    assert!(at_place_two.is_ok());
    let at_place_one = refusal(scene(4, 4, [Layer::fill("#000000")?.named("layer2")?]))?;
    assert!(at_place_one.contains("layer2"), "{at_place_one}");
    let clash = refusal(scene(
        4,
        4,
        [Layer::fill("#000000")?.named("layer2")?, Layer::fill("#111111")?],
    ));
    assert!(clash.is_ok_and(|message| message.contains("layer2")));
    Ok(())
}

#[test]
fn a_scene_holds_at_most_max_layers_layers() -> Outcome {
    assert_eq!(MAX_LAYERS, 1_000);
    let fills = |count: usize| {
        (0..count)
            .map(|_| Layer::fill("#00000001"))
            .collect::<Result<Vec<_>, _>>()
    };
    assert_eq!(scene(2, 2, fills(MAX_LAYERS)?)?.layers().len(), MAX_LAYERS);
    let message = refusal(scene(2, 2, fills(MAX_LAYERS + 1)?))?;
    assert!(message.contains("1000") && message.contains("1001"), "{message}");
    // the limit counts every layer of the scene, added in one call or several
    let nearly = scene(2, 2, fills(MAX_LAYERS - 1)?)?;
    assert!(nearly.add(fills(1)?).is_ok());
    assert!(nearly.add(fills(2)?).is_err());
    Ok(())
}

#[test]
fn the_size_of_a_render_is_a_whole_number_from_1_to_65000() -> Outcome {
    let canvas = scene(10, 10, [])?;
    check(
        "size",
        [(1, true), (100, true), (0, false), (65_001, false)]
            .iter()
            .map(|&(size, good)| (format!("{size}"), good, "size", canvas.render(size)))
            .collect(),
    )
}

/// Over 2^27 pixels in all (134,217,728): refused before anything is allocated, though each side is allowed.
#[test]
fn a_render_of_more_than_2_pow_27_pixels_is_refused_naming_the_size() -> Outcome {
    let wide = scene(16_385, 8_192, [])?;
    let message = refusal(wide.render(16_385))?;
    assert!(message.contains("size") && message.contains("134217728"), "{message}");
    Ok(())
}

#[test]
fn add_returns_a_new_scene_and_the_old_one_is_unchanged() -> Outcome {
    let before = scene(3, 3, [Layer::fill("#ff0000")?])?;
    let rendered = before.render(3)?;
    let after = before.add([Layer::fill("#0000ff")?])?;
    assert_eq!((before.layers().len(), after.layers().len()), (1, 2));
    assert_eq!(before.render(3)?.pixels(), rendered.pixels());
    assert!(all_exactly(&after.render(3)?, [0.0, 0.0, 1.0, 1.0]));
    Ok(())
}

#[test]
fn a_layer_can_be_used_in_two_scenes_and_in_one_scene_twice() -> Outcome {
    let red = Layer::fill("#ff0000")?;
    let one = scene(2, 2, [red.clone().named("a")?, red.clone().named("b")?])?;
    let two = scene(2, 2, [red])?;
    assert_eq!(one.render(2)?.pixels(), two.render(2)?.pixels());
    Ok(())
}

#[test]
fn a_scene_reports_its_canvas() -> Outcome {
    let canvas = Scene::new(1280, 720)?;
    assert_eq!((canvas.width(), canvas.height()), (1280, 720));
    Ok(())
}

#[test]
fn the_same_call_gives_the_same_picture_and_two_scenes_do_not_touch() -> Outcome {
    let build = |color: &str| scene(7, 5, [Layer::fill("#102030")?, Layer::fill(color)?.opacity(0.3)?]);
    let (one, other) = (build("#ff8000")?, build("#0080ff")?);
    let first = one.render(21)?;
    let beside = other.render(21)?;
    assert_eq!(one.render(21)?.pixels(), first.pixels());
    assert_ne!(first.pixels(), beside.pixels());
    assert_eq!(other.render(21)?.pixels(), beside.pixels());
    Ok(())
}
