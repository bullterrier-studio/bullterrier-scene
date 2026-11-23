//! What an editor asks of a scene: which layer shows at a pixel (`Scene::layer_at`), and a new stack of layers on the same
//! canvas (`Scene::with_layers`). The hit test's oracle is a scene laid out by hand: boxes on whole design pixels, drawn at one
//! output pixel a design pixel, asked at pixels well inside or well outside each shape.

use std::collections::BTreeSet;

use bullterrier_scene::{Error, Fit, Layer, Place, Scene, Values};

use crate::common::counting::Counting;
use crate::common::{Outcome, same_look, scene};

/// A flat colour in the box `[x, y, width, height]`.
fn boxed(color: &str, area: [f64; 4]) -> Result<Layer, Error> {
    Layer::fill(color)?.fit(area, Fit::Stretch, [0.5, 0.5])
}

/// A canvas of 100 by 100, bottom first: white paper; a red card (10..50 each way); a blue sheet over everything at opacity 0;
/// a green badge (30..70) masked to its left half (x 30..50); a black quad in perspective with corners (60, 60), (95, 60),
/// (95, 95), (80, 95); a group of the canvas moved 50 right, holding `a` (0..10 by 80..90) under `b` (5..15 by 85..95).
fn laid_out() -> Result<Scene, Error> {
    let group = scene(
        100,
        100,
        [
            boxed("#ff00ff", [0.0, 80.0, 10.0, 10.0])?.named("a")?,
            boxed("#00ffff", [5.0, 85.0, 10.0, 10.0])?.named("b")?,
        ],
    )?;
    scene(
        100,
        100,
        [
            Layer::fill("#ffffff")?.named("paper")?,
            boxed("#ff0000", [10.0, 10.0, 40.0, 40.0])?.named("card")?,
            Layer::fill("#0000ff")?.opacity(0.0)?.named("sheet")?,
            boxed("#00ff00", [30.0, 30.0, 40.0, 40.0])?
                .mask(boxed("#ffffff", [30.0, 30.0, 20.0, 40.0])?)?
                .named("badge")?,
            Layer::fill("#000000")?
                .perspective([[60.0, 60.0], [95.0, 60.0], [95.0, 95.0], [80.0, 95.0]])?
                .named("quad")?,
            Layer::scene(group).place(Place::new().dx(50.0))?.named("g")?,
        ],
    )
}

#[test]
fn the_layer_at_a_pixel_is_the_topmost_that_covers_it_through_masks_perspective_groups_and_opacity() -> Outcome {
    let scene_ = laid_out()?;
    for ((x, y), expected) in [
        ((5, 5), Some("paper")),
        ((20, 20), Some("card")),      // the sheet above it is at opacity 0
        ((40, 40), Some("badge")),     // inside its box and its mask
        ((60, 40), Some("paper")),     // inside its box, outside its mask; the card ends at 50
        ((90, 65), Some("quad")),      // inside the quad
        ((66, 80), Some("paper")),     // left of the quad's slanted edge (x 71.4 at y 80)
        ((52, 82), Some("g.scene.a")), // the group is moved 50 right
        ((57, 87), Some("g.scene.b")), // b over a
        ((2, 82), Some("paper")),      // where a would be, unmoved
    ] {
        assert_eq!(scene_.layer_at(x, y, 100)?.as_deref(), expected, "({x}, {y})");
    }
    Ok(())
}

#[test]
fn a_pixel_where_no_layer_covers_has_none_and_one_outside_the_picture_is_refused() -> Outcome {
    let bare = scene(100, 50, [boxed("#ff0000", [0.0, 0.0, 10.0, 10.0])?])?;
    assert_eq!(
        bare.layer_at(5, 5, 100)?.as_deref(),
        Some("layer1"),
        "an unnamed layer by its place"
    );
    assert_eq!(bare.layer_at(50, 25, 100)?, None);
    // the picture at size 50 is 50 by 25: the pixel (40, 20) is in it, (50, 0) and (0, 25) are not
    assert_eq!(bare.layer_at(2, 2, 50)?.as_deref(), Some("layer1"));
    assert_eq!(bare.layer_at(40, 20, 50)?, None);
    for (x, y, named) in [(50, 0, "x"), (0, 25, "y"), (-1, 0, "x")] {
        assert!(
            matches!(bare.layer_at(x, y, 50), Err(Error::Invalid { ref address, .. }) if address == named),
            "({x}, {y})"
        );
    }
    Ok(())
}

#[test]
fn a_faint_layer_is_at_a_pixel_where_an_eight_bit_picture_keeps_its_alpha() -> Outcome {
    // over the paper, a veil at opacity 0.004 shows (255 x 0.004 + 0.5 cuts to 1), one at 1/1000 does not (0.755 cuts to 0)
    for (opacity, expected) in [(0.004, "veil"), (0.001, "paper")] {
        let veiled = scene(
            10,
            10,
            [
                Layer::fill("#ffffff")?.named("paper")?,
                Layer::fill("#000000")?.opacity(opacity)?.named("veil")?,
            ],
        )?;
        assert_eq!(
            veiled.layer_at(5, 5, 10)?.as_deref(),
            Some(expected),
            "opacity {opacity}"
        );
    }
    Ok(())
}

/// The addresses `scene` lists, as a set.
fn addresses(scene: &Scene) -> BTreeSet<String> {
    scene
        .parameters()
        .into_iter()
        .map(|parameter| parameter.address)
        .collect()
}

#[test]
fn a_new_stack_keeps_every_named_layers_addresses_and_values_and_draws_as_a_scene_made_afresh() -> Outcome {
    let set = laid_out()?.with_values(&Values::new().set("card.dx", 3.0).set("g.scene.b.opacity", 0.5))?;
    let layers = set.layers().to_vec();
    let names = |stack: &Scene| {
        stack
            .layers()
            .iter()
            .map(|layer| layer.name().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    // reordered: the card on top
    let mut reordered = layers.clone();
    let card = reordered.remove(1);
    reordered.push(card);
    let moved = set.with_layers(reordered.clone())?;
    assert_eq!(names(&moved).last(), Some(&Some("card".to_owned())));
    assert_eq!(addresses(&moved), addresses(&set));
    let value = |stack: &Scene, address: &str| {
        stack
            .parameters()
            .into_iter()
            .find(|p| p.address == address)
            .map(|p| p.value)
    };
    assert_eq!(value(&moved, "card.dx"), value(&set, "card.dx"));
    assert_eq!(value(&moved, "g.scene.b.opacity"), value(&set, "g.scene.b.opacity"));
    assert!(same_look(
        &moved.render(100)?,
        &Scene::new(100, 100)?.add(reordered)?.render(100)?
    ));
    // removed: the badge's addresses go, and only they
    let without = set.with_layers(layers.iter().filter(|layer| layer.name() != Some("badge")).cloned())?;
    let gone: BTreeSet<_> = addresses(&set).difference(&addresses(&without)).cloned().collect();
    assert!(
        !gone.is_empty() && gone.iter().all(|address| address.starts_with("badge.")),
        "{gone:?}"
    );
    assert!(addresses(&without).is_subset(&addresses(&set)));
    // regrouped: the card and the badge in a group of the canvas, their addresses under `pair.scene.`
    let (inside, outside): (Vec<_>, Vec<_>) = layers
        .iter()
        .cloned()
        .partition(|layer| matches!(layer.name(), Some("card" | "badge")));
    let pair = Layer::scene(Scene::new(100, 100)?.add(inside)?).named("pair")?;
    let regrouped = set.with_layers(outside.into_iter().chain([pair]))?;
    for address in addresses(&set)
        .iter()
        .filter(|address| address.starts_with("card.") || address.starts_with("badge."))
    {
        assert!(
            addresses(&regrouped).contains(&format!("pair.scene.{address}")),
            "{address}"
        );
    }
    assert_eq!(value(&regrouped, "pair.scene.card.dx"), value(&set, "card.dx"));
    Ok(())
}

#[test]
fn a_new_stack_shares_the_cache_so_a_layer_that_did_not_change_is_not_drawn_again() -> Outcome {
    let kind = Counting::new(0.2);
    let first = scene(
        100,
        50,
        [
            Layer::make(kind.clone()).named("static")?,
            Layer::fill("#ff0000")?.opacity(0.5)?.named("tint")?,
        ],
    )?;
    first.render(100)?;
    let flipped: Vec<Layer> = first.layers().iter().rev().cloned().collect();
    first.with_layers(flipped)?.render(100)?;
    assert_eq!(kind.draws(), 1);
    Ok(())
}

#[test]
fn a_look_read_again_with_a_whole_number_written_another_way_is_found_in_the_cache() -> Outcome {
    // JavaScript writes 2.0 as 2: a layer read back from an edited document is the same look, and is not drawn again
    let kind = Counting::new(0.2);
    let looked = |exposure: &str| -> Result<Layer, Error> {
        Layer::make(kind.clone())
            .look(&format!(
                r#"{{"steps": [{{"effect": "grade", "params": {{"exposure": {exposure}}}}}]}}"#
            ))?
            .named("photo")
    };
    let first = scene(100, 50, [looked("2.0")?])?;
    first.render(100)?;
    first.with_layers([looked("2")?])?.render(100)?;
    assert_eq!(kind.draws(), 1);
    Ok(())
}

#[test]
fn a_new_stack_is_checked_as_add_checks_it() -> Outcome {
    let source = scene(
        10,
        10,
        [
            Layer::fill("#ff0000")?.named("ground")?,
            Layer::of("ground").named("echo")?,
        ],
    )?;
    let echo_alone = source.layers()[1..].to_vec();
    assert!(matches!(source.with_layers(echo_alone), Err(Error::Invalid { ref address, .. }) if address == "echo.of"));
    Ok(())
}
