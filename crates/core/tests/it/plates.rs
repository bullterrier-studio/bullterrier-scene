//! `Scene::plates`: one plane of ink coverage for each ink. The oracles are worked out outside the code under test: the
//! overprint rule of print (a channel is `paper * prod (1 - coverage * (1 - ink / paper))`, written in the test), a hand-made union of
//! two shapes, and the composite the scene draws itself.

use bullterrier_scene::{Error, Fit, Layer, Plates, Raster, Scene};
use serde_json::json;

use crate::common::{Outcome, made, scene};

/// `#f4efe4`, `#e8505b` and `#2a6fb5` as the levels they are.
const PAPER: [f32; 3] = [244.0 / 255.0, 239.0 / 255.0, 228.0 / 255.0];
const PINK: [f32; 3] = [232.0 / 255.0, 80.0 / 255.0, 91.0 / 255.0];
const BLUE: [f32; 3] = [42.0 / 255.0, 111.0 / 255.0, 181.0 / 255.0];

/// A halftone look that prints `inks` over `paper` as spot inks, with every print-realism option off (a speck of bare paper is
/// not on a plate, so the plates reproduce the picture only without them).
fn halftone(inks: &[&str], paper: &str) -> String {
    json!({"steps": [{"effect": "halftone", "params": {
        "separation": "spot", "inks": inks, "paper": paper, "cell": 4.0, "shape": "dot"
    }}]})
    .to_string()
}

/// A 24 by 16 picture whose colour changes across it, so that each ink has some coverage nearly everywhere.
fn photo() -> Result<Raster, Error> {
    made(24, 16, |x, y| {
        let (u, v) = (f32::from(u8::try_from(x).unwrap_or(0)) / 23.0, f32::from(u8::try_from(y).unwrap_or(0)) / 15.0);
        [0.35 + 0.5 * u, 0.4 + 0.4 * v, 0.55 + 0.3 * (1.0 - u), 1.0]
    })
}

/// A scene that is the photo printed in two spot inks.
fn print() -> Result<Scene, Error> {
    scene(
        24,
        16,
        [Layer::picture(photo()?)
            .look(&halftone(&["#e8505b", "#2a6fb5"], "#f4efe4"))?
            .inks(["pink", "blue"])?],
    )
}

/// The plane `name` of `plates`.
fn plane<'a>(plates: &'a Plates, name: &str) -> Result<&'a [f32], Box<dyn std::error::Error>> {
    plates.plane(name).ok_or_else(|| format!("no plate {name}").into())
}

#[test]
fn plates_laid_over_the_paper_by_the_overprint_rule_are_the_composite() -> Outcome {
    let drawn = print()?;
    let composite = drawn.render(24)?;
    let plates = drawn.plates(24)?;
    assert_eq!(plates.names(), ["pink", "blue"]);
    assert_eq!((plates.width(), plates.height()), (composite.width(), composite.height()));
    let (pink, blue) = (plane(&plates, "pink")?, plane(&plates, "blue")?);
    assert_eq!((pink.len(), blue.len()), (24 * 16, 24 * 16));
    let mut worst = 0.0_f32;
    let (mut pink_total, mut blue_total) = (0.0_f32, 0.0_f32);
    for (at, pixel) in composite.pixels().as_chunks::<4>().0.iter().enumerate() {
        for channel in 0..3 {
            // an ink over its own paper is the ink's colour: it lets the share `ink / paper` of the light through
            let through = |ink: [f32; 3]| ink[channel] / PAPER[channel];
            let expected = PAPER[channel] * (1.0 - pink[at] * (1.0 - through(PINK))) * (1.0 - blue[at] * (1.0 - through(BLUE)));
            worst = worst.max((pixel[channel] - expected).abs());
        }
        pink_total += pink[at];
        blue_total += blue[at];
    }
    // the inks of the two plates are in the picture: neither plate is empty, and neither is solid
    assert!(pink_total > 20.0 && blue_total > 20.0, "{pink_total} {blue_total}");
    assert!(pink_total < 24.0 * 16.0 - 20.0 && blue_total < 24.0 * 16.0 - 20.0);
    assert!(worst < 1e-4, "the largest difference from the overprint is {worst}");
    Ok(())
}

#[test]
fn a_layer_with_no_look_is_one_shape_in_one_ink_and_shapes_in_one_ink_overprint_as_a_union() -> Outcome {
    // a box from column 2 to 10 and row 2 to 6 at full strength, and one from 6 to 14 and 4 to 8 at half: where they meet the
    // coverage is 1 - (1 - 1)(1 - 0.5) = 1, and the half box alone is 0.5; a third shape of half strength over the half box
    // makes 1 - 0.5 * 0.5 = 0.75
    let drawn = scene(
        16,
        10,
        [
            Layer::fill("#123456")?.fit([2.0, 2.0, 8.0, 4.0], Fit::Stretch, [0.5, 0.5])?.inks(["black"])?,
            Layer::fill("#abcdef")?
                .fit([6.0, 4.0, 8.0, 4.0], Fit::Stretch, [0.5, 0.5])?
                .opacity(0.5)?
                .inks(["black"])?,
            Layer::fill("#abcdef")?
                .fit([12.0, 4.0, 2.0, 2.0], Fit::Stretch, [0.5, 0.5])?
                .opacity(0.5)?
                .inks(["red"])?,
            Layer::fill("#abcdef")?
                .fit([12.0, 4.0, 2.0, 2.0], Fit::Stretch, [0.5, 0.5])?
                .opacity(0.5)?
                .inks(["red"])?,
        ],
    )?
    .plates(16)?;
    assert_eq!(drawn.names(), ["black", "red"]);
    let black = plane(&drawn, "black")?;
    let at = |x: usize, y: usize| y * 16 + x;
    let expected = |x: usize, y: usize| -> f32 {
        let full = (2..10).contains(&x) && (2..6).contains(&y);
        let half = (6..14).contains(&x) && (4..8).contains(&y);
        match (full, half) {
            (true, true) => 1.0,
            (true, false) => 1.0,
            (false, true) => 0.5,
            (false, false) => 0.0,
        }
    };
    for y in 0..10 {
        for x in 0..16 {
            assert!((black[at(x, y)] - expected(x, y)).abs() < 1e-6, "black at ({x}, {y}): {}", black[at(x, y)]);
        }
    }
    let red = plane(&drawn, "red")?;
    for y in 0..10 {
        for x in 0..16 {
            let inside = (12..14).contains(&x) && (4..6).contains(&y);
            let want = if inside { 0.75 } else { 0.0 };
            assert!((red[at(x, y)] - want).abs() < 1e-6, "red at ({x}, {y}): {}", red[at(x, y)]);
        }
    }
    Ok(())
}

#[test]
fn plates_and_the_composite_are_the_same_whichever_is_drawn_first() -> Outcome {
    let first = print()?;
    let composite = first.render(24)?;
    let plates = first.plates(24)?;
    // the other order, on a scene made apart (its own cache)
    let second = print()?;
    assert_eq!(second.plates(24)?, plates, "plates first");
    assert_eq!(second.render(24)?, composite, "then the composite, which the plates' draws must not have changed");
    // and the same scene again, its cache full
    assert_eq!(first.plates(24)?, plates);
    assert_eq!(first.render(24)?, composite);
    Ok(())
}

#[test]
fn a_scene_that_asks_for_plates_wrongly_is_refused_naming_inks_and_the_layer() -> Outcome {
    let two = halftone(&["#e8505b", "#2a6fb5"], "#f4efe4");
    let grain = json!({"steps": [{"effect": "grain", "params": {"amount": 0.1}}]}).to_string();
    let rows: Vec<(&str, Scene, &str)> = vec![
        ("no layer names an ink", scene(4, 4, [Layer::fill("#ffffff")?])?, "inks"),
        (
            "three inks named and the look makes two planes",
            scene(24, 16, [Layer::picture(photo()?).look(&two)?.inks(["a", "b", "c"])?])?,
            "inks",
        ),
        (
            "two inks named by a layer with no look",
            scene(4, 4, [Layer::fill("#ffffff")?.inks(["a", "b"])?])?,
            "inks",
        ),
        (
            "a look that separates nothing",
            scene(24, 16, [Layer::picture(photo()?).look(&grain)?.inks(["a"])?])?,
            "inks",
        ),
        (
            "a light layer",
            scene(4, 4, [Layer::fill("#ffffff")?.as_light(true).inks(["a"])?])?,
            "inks",
        ),
    ];
    for (what, drawn, named) in rows {
        let error = drawn.plates(24).err();
        assert!(
            matches!(&error, Some(Error::Invalid { address, .. }) if address.contains(named)),
            "{what}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn ink_names_that_are_not_allowed_are_refused_naming_inks() {
    let rows: [(&str, Vec<&str>); 6] = [
        ("none", vec![]),
        ("nine", vec!["a", "b", "c", "d", "e", "f", "g", "h", "i"]),
        ("twice", vec!["a", "a"]),
        ("a slash", vec!["a/b"]),
        ("a leading digit", vec!["1a"]),
        ("empty", vec![""]),
    ];
    for (what, names) in rows {
        let error = Layer::fill("#ffffff").and_then(|layer| layer.inks(names)).err();
        assert!(
            matches!(&error, Some(Error::Invalid { address, .. }) if address == "inks"),
            "{what}: {error:?}"
        );
    }
}

#[test]
fn a_size_that_is_wrong_or_would_hold_too_many_values_is_refused_naming_size_before_drawing() -> Outcome {
    let drawn = print()?;
    for size in [0_i64, -5, 65_001, i64::MAX] {
        let error = drawn.plates(size).err();
        assert!(matches!(&error, Some(Error::Invalid { address, .. }) if address == "size"), "{size}: {error:?}");
    }
    // 65,000 by 43,333 pixels is 2.8e9 values of one plate: past what plates hold, said before a pixel is drawn
    let wide = scene(
        65_000,
        43_333,
        [Layer::fill("#000000")?.inks(["a"])?],
    )?;
    let error = wide.plates(65_000).err();
    assert!(matches!(&error, Some(Error::Invalid { address, .. }) if address == "size"), "{error:?}");
    Ok(())
}
