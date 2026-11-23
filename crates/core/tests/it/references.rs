//! References (`Layer::of`): a layer that draws another layer of its scene again. A reference is checked against a second
//! copy of the layer it names (what it inherits is what the copy is built from), against hand-composed placements and
//! hand-worked rectangles, and its refusals against known-good and known-bad rows. Pictures are the small ones of
//! `common`, whose values are exact in a float.

use bullterrier_scene::{Blend, Deg, Error, Fit, Layer, MAX_DEPTH, MAX_LAYERS, Mask, Place, Raster, Scene, Use};

use crate::common::counting::Counting;
use crate::common::{Outcome, close, numbered, pixel, pixels, rectangle, same_look, scene};

const WHITE: &str = "#ffffff";
const CANVAS: (u32, u32) = (24, 16);
/// Sizes that draw a design pixel as one output pixel and as two thirds more.
const SIZES: [u32; 2] = [24, 40];
const PLUS_ONE_STOP: &str =
    r#"{"steps": [{"effect": "grade", "params": {"exposure": 1.0}, "opacity": 1.0, "blend_mode": "normal"}]}"#;
const FLAT: &str = r#"{"steps": [{"effect": "grade", "params": {"contrast": 0.0}}]}"#;

/// A translucent picture of 12 by 8 pixels, every pixel different.
fn body() -> Result<Layer, Error> {
    Ok(Layer::picture(numbered(12, 8)?))
}

fn drawn(layers: impl IntoIterator<Item = Layer>, size: u32) -> Result<Raster, Error> {
    scene(CANVAS.0, CANVAS.1, layers)?.render(size)
}

/// Whether two pictures are the same within `tolerance` in every channel.
fn alike(left: &Raster, right: &Raster, tolerance: f32) -> bool {
    (left.width(), left.height()) == (right.width(), right.height())
        && pixels(left).zip(pixels(right)).all(|(l, r)| close(l, r, tolerance))
}

/// The message of the refusal of `result`.
fn refusal<T>(result: Result<T, Error>) -> Result<String, String> {
    match result {
        Err(error @ Error::Invalid { .. }) => Ok(error.to_string()),
        Err(other) => Err(format!("not an Invalid error: {other:?}")),
        Ok(_) => Err("accepted".to_owned()),
    }
}

// --- what a reference inherits -------------------------------------------------------------------------------------

/// Every kind of layer, placed every way: a reference with nothing of its own is a second copy of the layer it names.
#[test]
fn a_reference_with_nothing_of_its_own_is_a_second_copy_of_the_body_it_names() -> Outcome {
    let map = bullterrier_scene::Map::new(
        12,
        8,
        (0..96)
            .map(|at| f32::from(u8::try_from(at % 7).unwrap_or(0)) / 6.0)
            .collect(),
    )?;
    let rows: Vec<(&str, Layer)> = vec![
        ("plain", body()?),
        ("moved by whole pixels", body()?.place(Place::new().dx(3.0).dy(2.0))?),
        (
            "moved, turned and scaled",
            body()?.place(Place::new().dx(5.3).dy(2.9).rotate(Deg(17.0)).scale(0.9))?,
        ),
        ("a quarter turn", body()?.place(Place::new().dx(4.0).rotate(Deg(90.0)))?),
        (
            "flipped and flattened",
            body()?.place(Place::new().flip_x(true).aspect(0.6).dx(2.0))?,
        ),
        (
            "skewed",
            body()?.place(Place::new().skew_x(Deg(20.0)).anchor(0.2, 0.7))?,
        ),
        ("fitted", body()?.fit([2.0, 1.0, 18.0, 9.0], Fit::Cover, [0.3, 0.8])?),
        (
            "in perspective",
            body()?.perspective([[2.0, 1.0], [20.0, 4.0], [17.0, 14.0], [3.0, 12.0]])?,
        ),
        (
            "a parallelogram",
            body()?.perspective([[2.0, 1.0], [20.0, 3.0], [20.0, 13.0], [2.0, 11.0]])?,
        ),
        (
            "a look and a map's mask",
            body()?
                .map("m", map)?
                .look(PLUS_ONE_STOP)?
                .mask("m")?
                .place(Place::new().rotate(Deg(8.0)))?,
        ),
        (
            "a drawing",
            Layer::drawing(&rectangle((12, 8), (1.5, 1.25), (9.0, 6.5), "#cc3355"))?.place(Place::new().dx(4.4))?,
        ),
        (
            "a fill",
            Layer::fill("#22aa6680")?.fit([3.0, 3.0, 9.0, 7.0], Fit::Stretch, [0.5, 0.5])?,
        ),
    ];
    for (what, layer) in rows {
        for size in SIZES {
            let by_reference = drawn([layer.clone().named("a")?, Layer::of("a")], size)?;
            let by_copy = drawn([layer.clone().named("a")?, layer.clone().named("b")?], size)?;
            assert!(same_look(&by_reference, &by_copy), "{what} at {size}");
        }
    }
    Ok(())
}

/// A reference owns stages 6 to 9: it keeps nothing of the named layer's layer masks, opacity, blend or light. Each row gives
/// the named layer one of them; the reference is still the body alone over what is below, as a copy of the body is.
#[test]
fn a_reference_inherits_no_layer_mask_opacity_blend_or_light() -> Outcome {
    let named = |layer: Layer| layer.named("a");
    let rows: Vec<(&str, Layer)> = vec![
        (
            "a layer mask",
            named(body()?.mask(
                Mask::new(Layer::fill(WHITE)?.fit([0.0, 0.0, 6.0, 16.0], Fit::Stretch, [0.5, 0.5])?).feather(2.0),
            )?)?,
        ),
        ("an opacity", named(body()?.opacity(0.25)?)?),
        ("a blend", named(body()?.blend(Blend::Multiply))?),
        ("light", named(body()?.as_light(true))?),
    ];
    for (what, layer) in rows {
        let below = Layer::fill("#336699")?;
        let by_reference = drawn([below.clone(), layer.clone(), Layer::of("a")], 24)?;
        let by_copy = drawn([below, layer, body()?.named("b")?], 24)?;
        assert!(same_look(&by_reference, &by_copy), "{what}");
    }
    Ok(())
}

/// A reference has the join of any layer: its own opacity, blend, light and masks.
#[test]
fn a_reference_owns_its_opacity_blend_light_and_masks() -> Outcome {
    let join =
        |layer: Layer| -> Result<Layer, Error> {
            layer.opacity(0.5)?.blend(Blend::Screen).as_light(true).mask(
                Mask::new(Layer::fill(WHITE)?.fit([0.0, 0.0, 14.0, 16.0], Fit::Stretch, [0.5, 0.5])?).feather(1.5),
            )
        };
    let named = body()?.named("a")?;
    let by_reference = drawn([Layer::fill("#336699")?, named.clone(), join(Layer::of("a"))?], 24)?;
    let by_copy = drawn([Layer::fill("#336699")?, named, join(body()?)?], 24)?;
    assert!(same_look(&by_reference, &by_copy));
    Ok(())
}

/// A reference adds its own looks after the named layer's, in the order given, and its own masks by the maps of the
/// picture it names.
#[test]
fn a_reference_adds_its_looks_after_the_named_layers_and_masks_by_its_maps() -> Outcome {
    let map = bullterrier_scene::Map::new(
        12,
        8,
        (0..96)
            .map(|at| f32::from(u8::try_from(at % 5).unwrap_or(0)) / 4.0)
            .collect(),
    )?;
    let layer = body()?.map("m", map)?;
    let a = layer.clone().look(PLUS_ONE_STOP)?.named("a")?;
    let own = Layer::of("a").look(FLAT)?.mask(Mask::new("m").feather(1.0).invert())?;
    let copy = layer
        .look(PLUS_ONE_STOP)?
        .look(FLAT)?
        .mask(Mask::new("m").feather(1.0).invert())?;
    for size in SIZES {
        assert!(same_look(
            &drawn([a.clone(), own.clone()], size)?,
            &drawn([a.clone(), copy.clone().named("b")?], size)?
        ));
    }
    // the order of the looks is the order they run in: the named layer's first
    let other = body()?.look(FLAT)?.look(PLUS_ONE_STOP)?.named("b")?;
    assert!(!same_look(&drawn([a, own], 24)?, &drawn([other], 24)?));
    Ok(())
}

/// A reference follows the layer it names: the scene is rebuilt with the named layer moved, and the reference, edited
/// not at all, is where the copy of the moved layer is.
#[test]
fn a_reference_follows_the_layer_it_names() -> Outcome {
    let reference = Layer::of("a").opacity(0.7)?;
    for dx in [0.0, 3.0, 7.5] {
        let a = body()?.place(Place::new().dx(dx).rotate(Deg(10.0)))?.named("a")?;
        let by_reference = drawn([a.clone(), reference.clone()], 24)?;
        let by_copy = drawn([a.clone(), a.named("b")?.opacity(0.7)?], 24)?;
        assert!(same_look(&by_reference, &by_copy), "dx {dx}");
    }
    Ok(())
}

/// A layer used by a reference is the same layer in two scenes: each scene's own layer is what is drawn again.
#[test]
fn a_reference_in_two_scenes_names_the_layer_of_each() -> Outcome {
    let reference = Layer::of("a");
    let red = Layer::fill("#ff0000")?.named("a")?;
    let blue = Layer::fill("#0000ff")?.named("a")?;
    let in_red = drawn([red.clone(), reference.clone()], 8)?;
    let in_blue = drawn([blue.clone(), reference], 8)?;
    assert_eq!(pixel(&in_red, 3, 3), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(pixel(&in_blue, 3, 3), [0.0, 0.0, 1.0, 1.0]);
    Ok(())
}

// --- order ---------------------------------------------------------------------------------------------------------

/// A reference is stacked where it is written, not where it is resolved: written before the layer it names, in the same
/// call, it draws below it, and the two orders draw differently.
#[test]
fn a_reference_is_stacked_where_it_is_written_whichever_comes_first_in_the_call() -> Outcome {
    let a = body()?.place(Place::new().dx(4.0).dy(2.0))?.named("a")?;
    let glow = Layer::of("a")
        .opacity(0.6)?
        .place(Place::new().dx(-3.0).scale(1.5))?
        .named("glow")?;
    let copy = body()?
        .place(Place::new().dx(4.0).dy(2.0))?
        .opacity(0.6)?
        .place(Place::new().dx(1.0).scale(1.5))?;
    let below = drawn([glow.clone(), a.clone()], 24)?;
    let above = drawn([a.clone(), glow], 24)?;
    // the copy of the reference's body, put below the named layer, is the picture of the reference written below it
    let by_copy = drawn([copy.named("c")?, a.clone()], 24)?;
    assert!(alike(&below, &by_copy, 1e-5), "written below, drawn below");
    assert!(!alike(&below, &above, 1e-3), "the two orders draw differently");
    // the same in two calls: the layer named first, the reference after
    let in_two_calls = scene(CANVAS.0, CANVAS.1, [a.clone()])?
        .add([Layer::of("a")])?
        .render(24)?;
    assert!(same_look(&in_two_calls, &drawn([a.clone(), Layer::of("a")], 24)?));
    Ok(())
}

// --- placement -----------------------------------------------------------------------------------------------------

/// The layer's own placement, an extra place of the reference, and the one placement of a copy that is the two composed.
/// A row is exact where the composition is of exact places (it is a bit for bit equal), otherwise within a tolerance.
struct Composed {
    id: &'static str,
    first: Place,
    then: Place,
    together: Place,
    exact: bool,
}

#[test]
fn an_extra_place_composes_with_the_named_layers_as_the_hand_composed_placement() -> Outcome {
    let rows = [
        Composed {
            id: "a move after a move",
            first: Place::new().dx(3.0).dy(2.0),
            then: Place::new().dx(4.0).dy(-1.0),
            together: Place::new().dx(7.0).dy(1.0),
            exact: true,
        },
        Composed {
            id: "a quarter turn after a quarter turn",
            first: Place::new().rotate(Deg(90.0)),
            then: Place::new().rotate(Deg(90.0)),
            together: Place::new().rotate(Deg(180.0)),
            exact: true,
        },
        Composed {
            id: "a quarter turn after a quarter turn and a move",
            first: Place::new().rotate(Deg(90.0)).dx(5.0),
            then: Place::new().rotate(Deg(90.0)),
            together: Place::new().rotate(Deg(180.0)).dx(5.0),
            exact: true,
        },
        Composed {
            id: "a flip after a flip",
            first: Place::new().flip_x(true).dx(2.0),
            then: Place::new().flip_x(true),
            together: Place::new().dx(2.0),
            exact: true,
        },
        Composed {
            id: "a halving and a doubling",
            first: Place::new().scale(0.5).dy(1.0),
            then: Place::new().scale(2.0),
            together: Place::new().dy(1.0),
            exact: true,
        },
        Composed {
            id: "a turn after a turn",
            first: Place::new().rotate(Deg(30.0)).dx(2.0),
            then: Place::new().rotate(Deg(25.0)),
            together: Place::new().rotate(Deg(55.0)).dx(2.0),
            exact: false,
        },
        Composed {
            id: "a turn about the corner of the bounding rectangle of a moved layer",
            first: Place::new().dx(3.0),
            then: Place::new().rotate(Deg(40.0)).anchor(0.0, 0.0),
            together: Place::new().rotate(Deg(40.0)).anchor(0.0, 0.0).dx(3.0),
            exact: false,
        },
        Composed {
            id: "a scale about the far corner of a quarter turned layer's rectangle",
            // the box 12 by 8 turned is 8 wide and 12 tall about its centre; its far corner is (4, 6) from the centre, and a
            // scale of a half about it moves the centre by half of that
            first: Place::new().rotate(Deg(90.0)),
            then: Place::new().scale(0.5).anchor(1.0, 1.0),
            together: Place::new().rotate(Deg(90.0)).scale(0.5).dx(2.0).dy(3.0),
            exact: false,
        },
    ];
    for row in rows {
        for size in SIZES {
            let a = body()?.place(row.first)?.named("a")?;
            let by_reference = drawn([a.clone(), Layer::of("a").place(row.then)?], size)?;
            let by_copy = drawn([a, body()?.place(row.together)?.named("b")?], size)?;
            if row.exact {
                assert!(same_look(&by_reference, &by_copy), "{} at {size}", row.id);
            } else {
                assert!(alike(&by_reference, &by_copy, 1e-4), "{} at {size}", row.id);
            }
        }
    }
    Ok(())
}

/// A reference's extra places chain: a reference to a reference composes the place of each after the one before.
#[test]
fn the_extra_places_of_a_chain_of_references_compose_in_order() -> Outcome {
    let a = body()?.place(Place::new().dx(1.0))?.named("a")?;
    let first = Layer::of("a")
        .place(Place::new().dx(2.0))?
        .opacity(0.0)?
        .named("first")?;
    let second = Layer::of("first").place(Place::new().dy(3.0));
    let by_reference = drawn([a.clone(), first, second?], 24)?;
    let by_copy = drawn([a, body()?.place(Place::new().dx(3.0).dy(3.0))?.named("b")?], 24)?;
    assert!(same_look(&by_reference, &by_copy));
    Ok(())
}

/// The api's own shadow: a layer flattened to 0.12 of its height about the bottom middle of the bounding rectangle of the
/// layer as it is placed. The named layer is a 20 by 40 opaque picture turned a quarter about its centre and moved by (10, 5):
/// its rectangle is x 0 to 40, y 15 to 35, so the shadow is 2.4 design pixels tall at its bottom, rows 32.6 to 35, over the
/// same columns.
#[test]
fn a_shadow_by_reference_is_the_bounding_rectangle_of_the_placed_layer_squashed_about_its_bottom_middle() -> Outcome {
    let dog = Layer::picture(Raster::rgba(20, 40, [0.2, 0.4, 0.8, 1.0].repeat(800))?)
        .place(Place::new().rotate(Deg(90.0)).dx(10.0).dy(5.0))?
        .named("dog")?;
    // the shadow alone: the dog is the layer it names, put below at no opacity, so that only the shadow shows
    let hidden = dog.opacity(0.0)?.named("dog")?;
    let shadow = Layer::of("dog").place(Place::new().aspect(0.12).anchor(0.5, 1.0))?;
    let raster = scene(60, 60, [hidden, shadow])?.render(60)?;
    for x in 0..60 {
        let column = if (0..40).contains(&x) { 1.0 } else { 0.0 };
        // the bottom two rows are covered whole, the one above them by 0.4, those above that not at all
        for (y, covered) in [(34, 1.0), (33, 1.0), (32, 0.4), (31, 0.0), (36, 0.0)] {
            let got = pixel(&raster, x, y)[3];
            assert!(
                (got - column * covered).abs() < 1e-4,
                "({x}, {y}): alpha {got}, expected {}",
                column * covered
            );
        }
    }
    Ok(())
}

/// An extra place after a perspective is one homography: the layer with the extra place equals a copy whose quadrilateral
/// is the named layer's moved by it, worked out by hand here.
#[test]
fn an_extra_place_after_a_perspective_moves_the_quadrilateral() -> Outcome {
    let corners = [[2.0, 1.0], [20.0, 4.0], [17.0, 14.0], [3.0, 12.0]];
    let a = body()?.perspective(corners)?.named("a")?;
    let moved = corners.map(|[x, y]| [x + 3.0, y - 2.0]);
    let by_reference = drawn([a.clone(), Layer::of("a").place(Place::new().dx(3.0).dy(-2.0))?], 24)?;
    let by_copy = drawn([a.clone(), body()?.perspective(moved)?.named("b")?], 24)?;
    assert!(alike(&by_reference, &by_copy, 1e-4));
    // the scale is about the top left of the rectangle the quadrilateral had before it, and the move follows it
    let both = drawn(
        [
            a.clone(),
            Layer::of("a").place(Place::new().scale(0.5).anchor(0.0, 0.0).dx(3.0).dy(-2.0))?,
        ],
        24,
    )?;
    let hand = corners.map(|[x, y]| [2.0 + 0.5 * (x - 2.0) + 3.0, 1.0 + 0.5 * (y - 1.0) - 2.0]);
    let by_hand = drawn([a, body()?.perspective(hand)?.named("b")?], 24)?;
    assert!(alike(&both, &by_hand, 1e-4));
    Ok(())
}

// --- masks ---------------------------------------------------------------------------------------------------------

/// A reference as a mask layer reads the colour of the named layer as it is drawn, with the reference's own place and looks,
/// and the alpha of the named layer's own masks of a map but not its layer masks.
#[test]
fn a_reference_as_a_mask_is_the_named_layer_drawn_on_the_canvas_with_its_own_place() -> Outcome {
    let named = body()?
        .place(Place::new().dx(2.0).dy(1.0).rotate(Deg(12.0)))?
        .named("a")?;
    let hidden = |layer: Layer| layer.opacity(0.0);
    let masked =
        |by: Layer| Layer::fill("#cc3355").and_then(|fill| fill.mask(Mask::new(by).using(Use::Luminance).feather(1.5)));
    let copy = body()?.place(Place::new().dx(2.0).dy(1.0).rotate(Deg(12.0)))?;
    for size in SIZES {
        let by_reference = drawn([hidden(named.clone())?, masked(Layer::of("a"))?], size)?;
        let by_copy = drawn([hidden(named.clone())?, masked(copy.clone())?], size)?;
        assert!(same_look(&by_reference, &by_copy), "{size}");
    }
    // the named layer's own layer mask is not in it
    let cut = body()?
        .place(Place::new().dx(2.0))?
        .mask(Layer::fill(WHITE)?.fit([0.0, 0.0, 5.0, 16.0], Fit::Stretch, [0.5, 0.5])?)?
        .named("a")?;
    let plain = body()?.place(Place::new().dx(2.0))?;
    let by_reference = drawn([cut.opacity(0.0)?.named("a")?, masked(Layer::of("a"))?], 24)?;
    let by_copy = drawn([body()?.opacity(0.0)?.named("a")?, masked(plain)?], 24)?;
    assert!(same_look(&by_reference, &by_copy));
    Ok(())
}

/// The api's shadow by a mask: a fill masked by a reference to the dog, flattened under it and feathered.
#[test]
fn a_fill_masked_by_a_flattened_reference_is_the_shadow_of_the_named_layer() -> Outcome {
    let dog = body()?
        .place(Place::new().dx(6.0).dy(2.0).rotate(Deg(35.0)))?
        .named("dog")?;
    let flat = Place::new().aspect(0.12).anchor(0.5, 1.0);
    let by_reference = drawn(
        [
            Layer::fill("#000000")?.mask(Mask::new(Layer::of("dog").place(flat)?).feather(2.0))?,
            dog.clone(),
        ],
        24,
    )?;
    // the dog's rectangle is the turned box's: the shadow is that rectangle's own flattening, which is not the box's
    let by_wrong_box = drawn(
        [
            Layer::fill("#000000")?.mask(
                Mask::new(
                    body()?.place(
                        Place::new()
                            .dx(6.0)
                            .dy(2.0)
                            .rotate(Deg(35.0))
                            .aspect(0.12)
                            .anchor(0.5, 1.0),
                    )?,
                )
                .feather(2.0),
            )?,
            dog,
        ],
        24,
    )?;
    assert!(
        !alike(&by_reference, &by_wrong_box, 1e-3),
        "a flattened tilted layer is not a tilted flattened box"
    );
    Ok(())
}

// --- refusals ------------------------------------------------------------------------------------------------------

#[test]
fn a_reference_that_names_no_layer_of_the_scene_is_refused_at_add() -> Outcome {
    let fill = |name: &str| Layer::fill("#102030")?.named(name);
    // known good: a layer added earlier, or in the same call, in either order
    assert!(scene(4, 4, [fill("a")?, Layer::of("a")]).is_ok());
    assert!(scene(4, 4, [Layer::of("a"), fill("a")?]).is_ok());
    assert!(scene(4, 4, [fill("a")?])?.add([Layer::of("a")]).is_ok());
    // known bad
    let message = refusal(scene(4, 4, [fill("a")?, Layer::of("b").named("glow")?]))?;
    assert!(message.starts_with("glow.of") && message.contains("\"b\""), "{message}");
    let message = refusal(scene(4, 4, [Layer::of("b")]))?;
    assert!(message.starts_with("layer1.of"), "{message}");
    // a layer added by a LATER call is not found
    let first = scene(4, 4, [Layer::of("a")]);
    assert!(refusal(first)?.contains("\"a\""));
    assert!(
        refusal(
            scene(4, 4, [fill("z")?])?
                .add([Layer::of("a")])
                .and_then(|s| s.add([fill("a")?]))
        )?
        .contains("\"a\"")
    );
    // a reference in a mask layer names a layer too
    let message = refusal(scene(4, 4, [fill("a")?.mask(Layer::of("nobody"))?]))?;
    assert!(message.contains("\"nobody\""), "{message}");
    Ok(())
}

#[test]
fn a_chain_of_references_that_leads_back_to_itself_is_refused_naming_the_cycle() -> Outcome {
    let fill = |name: &str| Layer::fill("#102030")?.named(name);
    let reference = |name: &str, to: &str| Layer::of(to).named(name);
    let rows: Vec<(&str, Result<Scene, Error>, Option<&str>)> = vec![
        (
            "a reference to a layer",
            scene(4, 4, [fill("a")?, reference("b", "a")?]),
            None,
        ),
        (
            "a chain",
            scene(4, 4, [fill("a")?, reference("b", "a")?, reference("c", "b")?]),
            None,
        ),
        (
            "a chain written backwards",
            scene(4, 4, [reference("c", "b")?, reference("b", "a")?, fill("a")?]),
            None,
        ),
        ("itself", scene(4, 4, [reference("a", "a")?]), Some("a -> a")),
        (
            "two",
            scene(4, 4, [reference("a", "b")?, reference("b", "a")?]),
            Some("a -> b -> a"),
        ),
        (
            "three",
            scene(4, 4, [reference("a", "b")?, reference("b", "c")?, reference("c", "a")?]),
            Some("a -> b -> c -> a"),
        ),
        (
            "three, after a good layer",
            scene(
                4,
                4,
                [
                    fill("z")?,
                    reference("a", "b")?,
                    reference("b", "c")?,
                    reference("c", "a")?,
                ],
            ),
            Some("a -> b -> c -> a"),
        ),
        (
            "two through masks",
            scene(
                4,
                4,
                [fill("a")?.mask(Layer::of("b"))?, fill("b")?.mask(Layer::of("a"))?],
            ),
            Some("a -> b -> a"),
        ),
        (
            "a reference through the mask of the layer it names",
            scene(4, 4, [fill("a")?.mask(Layer::of("b"))?, reference("b", "a")?]),
            Some("a -> b -> a"),
        ),
        (
            "a layer masked by a reference to itself",
            scene(4, 4, [fill("a")?.mask(Layer::of("a"))?]),
            Some("a -> a"),
        ),
        (
            "through the mask of a mask",
            scene(
                4,
                4,
                [
                    fill("a")?.mask(Layer::fill(WHITE)?.opacity(0.5)?.mask(Layer::of("b"))?)?,
                    fill("b")?.mask(Layer::of("a"))?,
                ],
            ),
            Some("a -> b -> a"),
        ),
    ];
    for (what, result, cycle) in rows {
        match (result, cycle) {
            (Ok(_), None) => {}
            (result, Some(cycle)) => {
                let message = refusal(result)?;
                assert!(message.contains(cycle), "{what}: {message}");
            }
            (Err(error), None) => return Err(format!("{what}: {error}").into()),
        }
    }
    Ok(())
}

/// A reference has no source, no `fit` and no `perspective`; its placement is `place`.
#[test]
fn a_reference_refuses_a_fit_a_perspective_and_a_map() -> Outcome {
    let reference = || Layer::of("a");
    for (what, result) in [
        ("fit", reference().fit([0.0, 0.0, 4.0, 4.0], Fit::Cover, [0.5, 0.5])),
        (
            "perspective",
            reference().perspective([[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]]),
        ),
        (
            "map",
            reference().map("m", bullterrier_scene::Map::new(1, 1, vec![1.0])?),
        ),
    ] {
        let message = refusal(result)?;
        assert!(message.starts_with(what) && message.contains("\"a\""), "{message}");
    }
    assert!(reference().place(Place::new().dx(1.0)).is_ok());
    // a bad value of `place` is refused at the call, as it is on any layer
    assert!(refusal(reference().place(Place::new().scale(0.0)))?.starts_with("scale"));
    Ok(())
}

/// What a reference's masks by a map need of the layer it names is found when it is added: that layer must have the map. A
/// look needs nothing of it: every layer takes one, and a recipe image refuses is refused at the call.
#[test]
fn a_mask_by_a_map_the_named_layer_lacks_is_refused_at_add_naming_the_reference() -> Outcome {
    let picture = body()?.named("a")?;
    let fill = Layer::fill("#102030")?.named("a")?;
    assert!(scene(4, 4, [picture.clone(), Layer::of("a").look(PLUS_ONE_STOP)?]).is_ok());
    assert!(scene(4, 4, [fill.clone(), Layer::of("a").look(PLUS_ONE_STOP)?]).is_ok());
    // a recipe image refuses is refused at the call, before there is a layer to run it on
    assert!(refusal(Layer::of("a").look(r#"{"steps": [{"effect": "nonesuch"}]}"#))?.starts_with("look"));
    let message = refusal(scene(4, 4, [picture, Layer::of("a").mask("matte")?.named("glow")?]))?;
    assert!(
        message.starts_with("glow.mask") && message.contains("matte"),
        "{message}"
    );
    let message = refusal(scene(4, 4, [fill, Layer::of("a").mask("matte")?.named("glow")?]))?;
    assert!(
        message.starts_with("glow.mask") && message.contains("matte"),
        "{message}"
    );
    Ok(())
}

// --- depth and count -----------------------------------------------------------------------------------------------

/// `links` references in a chain on a fill, each naming the one before.
fn chain(links: usize) -> Result<Vec<Layer>, Error> {
    let mut layers = vec![Layer::fill("#ffffff80")?.named("l0")?];
    for link in 1..=links {
        layers.push(Layer::of(&format!("l{}", link - 1)).named(&format!("l{link}"))?);
    }
    Ok(layers)
}

/// A chain of `MAX_DEPTH` references is drawn on a thread of 256 KB, and one more is refused when it is added: the walks
/// over references are iterative, and the bound is on the chain.
#[test]
fn a_chain_of_sixteen_references_is_drawn_on_a_small_stack_and_a_chain_of_seventeen_is_refused() -> Outcome {
    assert_eq!(MAX_DEPTH, 16);
    let worked = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| -> Result<usize, String> {
            let scene =
                scene(4, 4, chain(MAX_DEPTH).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
            let shown = format!("{:?}", scene.layers());
            let copy = scene.clone();
            let raster = copy.render(4).map_err(|error| error.to_string())?;
            drop(scene);
            Ok(raster.pixels().len() + shown.len().min(1))
        })?;
    assert_eq!(worked.join().map_err(|_| "the thread panicked")??, 64 + 1);
    let message = refusal(scene(4, 4, chain(MAX_DEPTH + 1)?))?;
    assert!(message.contains("16") && message.contains("17"), "{message}");
    Ok(())
}

/// A stack layer masked by a nest of `masks` plain mask layers round a reference to the end of a chain of `links`
/// references: it leans on `masks + links + 2` layers deep (the nest is `masks + 1` mask layers, the innermost of them a
/// reference, which is one link more than the layer it names).
fn leaning(masks: usize, links: usize) -> Result<Vec<Layer>, Error> {
    let mut layers = chain(links)?;
    let mut nest = Layer::of(&format!("l{links}"));
    for _ in 0..masks {
        nest = Layer::fill(WHITE)?.mask(nest)?;
    }
    layers.push(Layer::fill("#334455")?.mask(nest)?.named("top")?);
    Ok(layers)
}

#[test]
fn masks_and_references_share_one_depth_and_the_limit_is_exact() -> Outcome {
    let worked = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| -> Result<usize, String> {
            // 6 plain mask layers round a reference to the end of a chain of 8: 6 + 8 + 2 = 16
            let layers = leaning(6, 8).map_err(|error| error.to_string())?;
            let scene = scene(4, 4, layers).map_err(|error| error.to_string())?;
            Ok(scene.render(4).map_err(|error| error.to_string())?.pixels().len())
        })?;
    assert_eq!(worked.join().map_err(|_| "the thread panicked")??, 64);
    for (masks, links) in [(6, 8), (7, 8), (6, 9), (14, 0), (15, 0), (0, 14), (0, 15)] {
        let accepted = scene(4, 4, leaning(masks, links)?).is_ok();
        assert_eq!(accepted, masks + links + 2 <= MAX_DEPTH, "{masks} masks, {links} links");
    }
    Ok(())
}

/// The layers a scene draws are counted at each use: a mask layer shared by every layer is drawn once for each.
/// 500 layers, each masked by one shared mask layer, draw 1,000 layers; 501 draw 1,002, though a scene holds a thousand.
#[test]
fn the_layers_a_scene_draws_are_counted_at_each_use_of_a_mask_layer_and_a_reference() -> Outcome {
    let shared = Layer::fill(WHITE)?;
    let masked = |count: usize| -> Result<Vec<Layer>, Error> {
        let mut layers = vec![Layer::fill("#102030")?.mask(shared.clone())?.named("a")?];
        for _ in 1..count {
            layers.push(Layer::of("a").mask(shared.clone())?);
        }
        Ok(layers)
    };
    assert_eq!(MAX_LAYERS, 1_000);
    assert!(scene(2, 2, masked(500)?).is_ok());
    let message = refusal(scene(2, 2, masked(501)?))?;
    assert!(message.contains("1002") && message.contains("1000"), "{message}");
    // and across calls: the count is the scene's, kept as layers are added
    let half = scene(2, 2, masked(250)?)?;
    let more = (0..250)
        .map(|_| Layer::of("a").mask(shared.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    assert!(half.add(more.clone()).is_ok());
    assert!(
        refusal(
            half.add(more)
                .and_then(|full| full.add([Layer::of("a").mask(shared.clone())?]))
        )?
        .contains("1002")
    );
    // a scene of a thousand layers with no masks is still accepted
    assert!(
        scene(
            2,
            2,
            chain(MAX_LAYERS - 1)?
                .into_iter()
                .enumerate()
                .filter(|(at, _)| *at < MAX_DEPTH + 1)
                .map(|(_, layer)| layer)
        )
        .is_ok()
    );
    Ok(())
}

/// A layer at opacity 0 cannot change the canvas in any blend mode, so it is not drawn where it is joined, nor are its masks;
/// the layers that refer to it draw its body.
#[test]
fn a_layer_at_opacity_zero_costs_nothing_where_it_is_joined_and_its_references_still_draw_it() -> Outcome {
    let (kind, masking) = (Counting::new(0.3), Counting::new(0.6));
    let base = || Layer::fill("#336699");
    let hidden = Layer::make(kind.clone())
        .named("source")?
        .blend(Blend::Multiply)
        .mask(Layer::make(masking.clone()))?
        .opacity(0.0)?;
    let alone = drawn([base()?, hidden.clone()], 24)?;
    assert_eq!((kind.draws(), masking.draws()), (0, 0), "joined at opacity 0");
    assert!(same_look(&alone, &drawn([base()?], 24)?));
    let shown = drawn([base()?, hidden, Layer::of("source")], 24)?;
    assert_eq!(kind.draws(), 1, "its reference draws it");
    let visible = drawn([base()?, Layer::make(Counting::new(0.3))], 24)?;
    assert!(same_look(&shown, &visible) && !same_look(&shown, &alone));
    Ok(())
}

/// A layer whose masks leave nothing visible (every value 0) cannot change the canvas in any blend mode, so its source is not
/// drawn where it is joined; a mask that leaves something visible still lets it draw, and either way the picture is the same.
#[test]
fn a_layer_masked_to_nothing_is_not_drawn_where_it_is_joined() -> Outcome {
    let base = || Layer::fill("#336699");
    let nothing = || Layer::fill("#ffffff").map(|white| Mask::new(white).invert());
    let (kind, shown) = (Counting::new(0.3), Counting::new(0.3));
    let hidden = Layer::make(kind.clone()).blend(Blend::Multiply).mask(nothing()?)?;
    let alone = drawn([base()?, hidden], 24)?;
    assert_eq!(kind.draws(), 0, "masked to nothing");
    assert!(same_look(&alone, &drawn([base()?], 24)?));
    let half = Layer::make(shown.clone()).mask(Layer::fill("#ffffff")?.opacity(0.5)?)?;
    drawn([base()?, half], 24)?;
    assert_eq!(shown.draws(), 1, "masked to half");
    Ok(())
}
