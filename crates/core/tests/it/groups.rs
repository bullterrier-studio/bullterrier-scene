//! Scenes as layers (`Layer::scene`): a group is drawn on its own, then joined as one picture. Checked against the inner
//! scene's own render, closed forms of the compositing equation and the inner render placed as a picture. Pictures and colours
//! are the small ones of `common`, whose values are exact in a float.

use bullterrier_scene::{Blend, Deg, Error, Fit, Layer, MAX_DEPTH, MAX_LAYERS, Place, Raster, Scene};

use crate::common::{Outcome, close, exactly, made, pixel, pixels, rectangle, scene};

/// Whether two pictures are the same bit for bit.
fn same(left: &Raster, right: &Raster) -> bool {
    (left.width(), left.height()) == (right.width(), right.height())
        && pixels(left).zip(pixels(right)).all(|(l, r)| exactly(l, r))
}

/// The largest difference of one premultiplied channel (alpha included) of two pictures of one size: the colour of a nearly
/// transparent pixel is noise and counts for what its alpha lets through.
fn most(left: &Raster, right: &Raster) -> f32 {
    assert_eq!((left.width(), left.height()), (right.width(), right.height()));
    let premultiplied = |[red, green, blue, alpha]: [f32; 4]| [red * alpha, green * alpha, blue * alpha, alpha];
    pixels(left)
        .zip(pixels(right))
        .flat_map(|(l, r)| {
            premultiplied(l)
                .into_iter()
                .zip(premultiplied(r))
                .map(|(l, r)| (l - r).abs())
        })
        .fold(0.0, f32::max)
}

/// A colour of 8-bit levels as the floats it is drawn as.
fn rgb(red: u8, green: u8, blue: u8) -> [f32; 3] {
    [red, green, blue].map(|level| f32::from(level) / 255.0)
}

/// `most` split in two: over the pixels both pictures cover fully, and over the others (the edges).
fn most_inside_and_at_edges(left: &Raster, right: &Raster) -> (f32, f32) {
    let premultiplied = |[red, green, blue, alpha]: [f32; 4]| [red * alpha, green * alpha, blue * alpha, alpha];
    let mut found = (0.0_f32, 0.0_f32);
    for (l, r) in pixels(left).zip(pixels(right)) {
        let gap = premultiplied(l)
            .into_iter()
            .zip(premultiplied(r))
            .map(|(l, r)| (l - r).abs())
            .fold(0.0, f32::max);
        let slot = if l[3] >= 1.0 - 1e-6 && r[3] >= 1.0 - 1e-6 {
            &mut found.0
        } else {
            &mut found.1
        };
        *slot = slot.max(gap);
    }
    found
}

// --- a group is its scene -------------------------------------------------------------------------------------------

/// An inner scene of 24 by 16 design pixels: a ramp picture under a translucent fill band and a drawing.
fn inner() -> Result<Scene, Error> {
    let ramp = made(24, 16, |x, y| {
        #[expect(clippy::cast_precision_loss, reason = "a pixel index of at most 24")]
        let level = |at: usize| at as f32 / 32.0;
        [level(x), level(y), 0.25, 1.0]
    })?;
    scene(
        24,
        16,
        [
            Layer::picture(ramp),
            Layer::fill("#2255aa80")?.fit([2.0, 3.0, 12.0, 6.0], Fit::Stretch, [0.5, 0.5])?,
            Layer::drawing(&rectangle((24, 16), (10.0, 4.0), (20.0, 13.0), "#ffcc00"))?,
        ],
    )
}

/// A group with no place, blend, opacity or mask is its inner scene drawn at the size the outer canvas gives it: bit for bit
/// where the picture is opaque, and a translucent one within the float's step (the group's raster is straight alpha).
#[test]
fn a_group_alone_is_its_scene_drawn_at_the_same_size() -> Outcome {
    for size in [24, 48, 40] {
        let expected = inner()?.render(size)?;
        let got = scene(24, 16, [Layer::scene(inner()?)])?.render(size)?;
        assert!(
            same(&got, &expected),
            "opaque inner at {size}: most {}",
            most(&got, &expected)
        );
    }
    // a translucent inner scene: the same within a step of a 32-bit float
    let faint = scene(
        24,
        16,
        [Layer::fill("#2255aa80")?.fit([2.0, 3.0, 12.0, 6.0], Fit::Stretch, [0.5, 0.5])?],
    )?;
    for size in [24, 40] {
        let got = scene(24, 16, [Layer::scene(faint.clone())])?.render(size)?;
        assert!(most(&got, &faint.render(size)?) <= 2e-6, "translucent at {size}");
    }
    Ok(())
}

/// A group moved by whole pixels is the inner render put there as a picture, bit for bit at one output pixel to a design
/// pixel.
#[test]
fn a_group_moved_by_whole_pixels_is_its_render_as_a_picture_moved() -> Outcome {
    let place = Place::new().dx(5.0).dy(3.0);
    let group = scene(40, 24, [Layer::scene(inner()?).place(place)?])?.render(40)?;
    let picture = scene(40, 24, [Layer::picture(inner()?.render(24)?).place(place)?])?.render(40)?;
    assert!(same(&group, &picture), "most {}", most(&group, &picture));
    Ok(())
}

/// The group is drawn at the density it will have, so a scaled group is sharp where the picture of its render is resampled:
/// a group twice as large equals the inner scene rendered at twice the size, as a picture, bit for bit.
#[test]
fn a_group_is_drawn_at_the_scale_it_will_have() -> Outcome {
    let group = scene(
        48,
        32,
        [Layer::scene(inner()?).fit([0.0, 0.0, 48.0, 32.0], Fit::Stretch, [0.5, 0.5])?],
    )?
    .render(48)?;
    let render = inner()?.render(48)?;
    assert!(same(&group, &render), "most {}", most(&group, &render));
    Ok(())
}

/// A group put by `place` is the inner render put there as a picture within a stated tolerance: the group is drawn at its
/// placed scale and the picture is resampled once more. The inner picture is a ramp, which a bilinear and an area average both
/// keep, so where both cover the pixel they agree within 0.005 (seen: 0.002, after a turn of 17 degrees and a scale of 0.9);
/// at the edge of a turned group the pixel's alpha is a hard raster edge blurred by the warp, against the picture's by the
/// coverage of its clip: within 0.3 (seen: 0.24). A wrong placement, the last check, is far outside.
#[test]
fn a_placed_group_is_its_render_as_a_picture_placed_within_a_tolerance() -> Outcome {
    let ramp = || -> Result<Scene, Error> {
        scene(
            24,
            16,
            [Layer::picture(made(24, 16, |x, y| {
                #[expect(clippy::cast_precision_loss, reason = "a pixel index of at most 24")]
                let level = |at: usize| at as f32 / 32.0;
                [level(x), level(y), 0.25, 1.0]
            })?)],
        )
    };
    let places = [
        Place::new().dx(6.4).dy(2.7),
        Place::new().dx(8.0).dy(4.0).rotate(Deg(17.0)).scale(0.9),
        Place::new().dx(6.0).dy(3.0).scale(0.6),
        Place::new().dx(4.0).dy(2.0).skew_x(Deg(20.0)),
        Place::new().dx(9.0).dy(3.0).flip_x(true).aspect(0.7),
    ];
    for (at, place) in places.into_iter().enumerate() {
        let group = scene(48, 32, [Layer::scene(ramp()?).place(place)?])?.render(48)?;
        let picture = scene(48, 32, [Layer::picture(ramp()?.render(24)?).place(place)?])?.render(48)?;
        let (inside, edge) = most_inside_and_at_edges(&group, &picture);
        assert!(inside <= 0.005, "row {at}: inside {inside}");
        assert!(edge <= 0.3, "row {at}: edge {edge}");
    }
    let wrong = scene(
        48,
        32,
        [Layer::scene(ramp()?).place(Place::new().dx(8.0).dy(4.0).rotate(Deg(-17.0)).scale(0.9))?],
    )?
    .render(48)?;
    let right = scene(
        48,
        32,
        [Layer::picture(ramp()?.render(24)?).place(Place::new().dx(8.0).dy(4.0).rotate(Deg(17.0)).scale(0.9))?],
    )?
    .render(48)?;
    assert!(most(&wrong, &right) > 0.3);
    Ok(())
}

/// One badge used twice, at two places and sizes, each as the inner scene's render put there.
#[test]
fn a_scene_used_twice_is_drawn_at_each_use_by_that_use() -> Outcome {
    let badge = scene(
        12,
        12,
        [
            Layer::fill("#cc3344")?,
            Layer::drawing(&rectangle((12, 12), (3.0, 3.0), (9.0, 9.0), "#ffffff"))?,
        ],
    )?;
    // one output pixel to a design pixel: the second use is twice as wide, so it is drawn at twice the density
    let outer = scene(
        48,
        24,
        [
            Layer::scene(badge.clone()).named("small")?,
            Layer::scene(badge.clone())
                .named("large")?
                .fit([20.0, 0.0, 24.0, 24.0], Fit::Stretch, [0.5, 0.5])?,
        ],
    )?
    .render(48)?;
    let by_hand = scene(
        48,
        24,
        [
            Layer::picture(badge.render(12)?),
            Layer::picture(badge.render(24)?).place(Place::new().dx(20.0))?,
        ],
    )?
    .render(48)?;
    assert!(same(&outer, &by_hand), "most {}", most(&outer, &by_hand));
    Ok(())
}

// --- isolation -----------------------------------------------------------------------------------------------------

/// Two opaque children overlapping in a group, the group at half opacity over a base: the overlap is `0.5 top + 0.5 base`
/// (the group is one picture faded once), and not the two children each faded.
#[test]
fn a_group_fades_as_one_picture() -> Outcome {
    let (base, bottom, top) = (rgb(240, 200, 40), rgb(200, 30, 30), rgb(20, 60, 220));
    let hex = |[red, green, blue]: [u8; 3]| format!("#{red:02x}{green:02x}{blue:02x}");
    let children = |opacity: f64| -> Result<[Layer; 2], Error> {
        Ok([
            Layer::fill(&hex([200, 30, 30]))?
                .fit([0.0, 0.0, 12.0, 8.0], Fit::Stretch, [0.5, 0.5])?
                .opacity(opacity)?,
            Layer::fill(&hex([20, 60, 220]))?
                .fit([6.0, 0.0, 12.0, 8.0], Fit::Stretch, [0.5, 0.5])?
                .opacity(opacity)?,
        ])
    };
    let paper = || Layer::fill(&hex([240, 200, 40]));
    let group = scene(
        18,
        8,
        [paper()?, Layer::scene(scene(18, 8, children(1.0)?)?).opacity(0.5)?],
    )?
    .render(18)?;
    let half = |of: [f32; 3]| -> [f32; 4] {
        let mixed: [f32; 3] = std::array::from_fn(|channel| 0.5 * of[channel] + 0.5 * base[channel]);
        [mixed[0], mixed[1], mixed[2], 1.0]
    };
    for (x, expected, what) in [
        (2, half(bottom), "bottom alone"),
        (8, half(top), "the overlap is the top child at the group's opacity"),
        (15, half(top), "top alone"),
    ] {
        assert!(
            close(pixel(&group, x, 3), expected, 2e-6),
            "{what}: {:?} vs {expected:?}",
            pixel(&group, x, 3)
        );
    }
    // the same two children each faded by half, on the base, differ in the overlap: the oracle can tell the two apart
    let each = scene(18, 8, [paper()?].into_iter().chain(children(0.5)?))?.render(18)?;
    let wrong: [f32; 3] =
        std::array::from_fn(|channel| 0.5 * top[channel] + 0.25 * bottom[channel] + 0.25 * base[channel]);
    assert!(close(pixel(&each, 8, 3), [wrong[0], wrong[1], wrong[2], 1.0], 2e-6));
    assert!(!close(pixel(&each, 8, 3), half(top), 0.05));
    Ok(())
}

/// A child blended `multiply` inside a group meets the group's own content and not what is below the group; a group blended
/// `multiply` meets what is below it.
#[test]
fn a_blend_inside_a_group_sees_the_group_and_a_blend_of_the_group_sees_the_base() -> Outcome {
    let (yellow, cyan, blue, orange) = ("#ffff00", "#00ffff", "#0000ff", "#ff8000");
    let child = || -> Result<Layer, Error> { Ok(Layer::fill(cyan)?.blend(Blend::Multiply)) };
    // under the multiply child the group holds blue on its left half and nothing on its right
    let inner = scene(
        8,
        4,
        [
            Layer::fill(blue)?.fit([0.0, 0.0, 4.0, 4.0], Fit::Stretch, [0.5, 0.5])?,
            child()?,
        ],
    )?;
    let got = scene(8, 4, [Layer::fill(yellow)?, Layer::scene(inner)])?.render(8)?;
    // over the blue: cyan times blue is blue; over nothing: the cyan itself, not cyan times the yellow below (green)
    assert!(close(pixel(&got, 1, 1), [0.0, 0.0, 1.0, 1.0], 1e-6));
    assert!(
        close(pixel(&got, 6, 1), [0.0, 1.0, 1.0, 1.0], 1e-6),
        "{:?}",
        pixel(&got, 6, 1)
    );
    // a group blended multiply: orange times yellow, channel by channel
    let group = scene(8, 4, [Layer::fill(orange)?])?;
    let got = scene(8, 4, [Layer::fill(yellow)?, Layer::scene(group).blend(Blend::Multiply)])?.render(8)?;
    assert!(close(pixel(&got, 3, 2), [1.0, 128.0 / 255.0, 0.0, 1.0], 2e-6));
    Ok(())
}

// --- the pixel phase -----------------------------------------------------------------------------------------------

/// A group put at a fraction of a pixel is its children put there: a picture is resampled once (not once inside and once more
/// outside), a drawing is drawn at the fraction (not drawn on whole pixels and then moved), and a fill keeps the coverage of
/// its edge. Children of a normal blend at full opacity on a transparent backdrop are the same as laid on the canvas (over is
/// associative), within the rounding of one trip through straight alpha.
#[test]
fn a_group_at_a_fraction_of_a_pixel_is_not_resampled_again() -> Outcome {
    let moved = Place::new().dx(2.4).dy(1.7);
    let rows: Vec<(&str, Layer)> = vec![
        ("a picture", Layer::picture(crate::common::numbered(12, 8)?)),
        (
            "a drawing",
            Layer::drawing(&rectangle((16, 12), (3.25, 2.5), (12.75, 9.5), "#cc3355"))?,
        ),
        (
            "a fill",
            Layer::fill("#22aa66")?.fit([1.0, 1.0, 14.0, 10.0], Fit::Stretch, [0.5, 0.5])?,
        ),
    ];
    for (what, child) in rows {
        for size in [32, 48] {
            let by_group =
                scene(32, 24, [Layer::scene(scene(16, 12, [child.clone()])?).place(moved)?])?.render(size)?;
            let direct = scene(32, 24, [child.clone().place(moved)?])?.render(size)?;
            assert!(
                most(&by_group, &direct) <= 2e-6,
                "{what} at {size}: most {}",
                most(&by_group, &direct)
            );
        }
    }
    Ok(())
}

// --- depth and count -----------------------------------------------------------------------------------------------

/// The message of the refusal of `result`.
fn refusal<T>(result: Result<T, Error>) -> Result<String, String> {
    match result {
        Err(error @ Error::Invalid { .. }) => Ok(error.to_string()),
        Err(other) => Err(format!("not an Invalid error: {other:?}")),
        Ok(_) => Err("accepted".to_owned()),
    }
}

/// A scene of one translucent fill, wrapped in `levels` scenes, each holding the one before as its only layer.
fn nest(levels: usize) -> Result<Scene, Error> {
    let mut nested = scene(4, 4, [Layer::fill("#ffffff80")?])?;
    for _ in 0..levels {
        nested = scene(4, 4, [Layer::scene(nested)])?;
    }
    Ok(nested)
}

/// A chain of `links` references on a fill, each naming the one before.
fn chain(links: usize) -> Result<Vec<Layer>, Error> {
    let mut layers = vec![Layer::fill("#ffffff80")?.named("l0")?];
    for link in 1..=links {
        layers.push(Layer::of(&format!("l{}", link - 1)).named(&format!("l{link}"))?);
    }
    Ok(layers)
}

/// A scene that leans `links + plain + 2 x masked` deep: at the bottom a chain of `links` references, then `plain` scenes that
/// each hold the one before as a layer (the first also a reference to that layer, which draws the group again: more would
/// double the layers drawn at each level), then `masked` scenes that each hold the one before as the mask layer of a fill (a
/// group used as a mask is two levels).
fn mixed(links: usize, plain: usize, masked: usize) -> Result<Scene, Error> {
    let mut leaning = scene(4, 4, chain(links)?)?;
    for level in 0..plain {
        let group = Layer::scene(leaning).named("group")?;
        let layers = if level == 0 {
            vec![group, Layer::of("group").opacity(0.5)?]
        } else {
            vec![group]
        };
        leaning = scene(4, 4, layers)?;
    }
    for _ in 0..masked {
        leaning = scene(4, 4, [Layer::fill("#ffffff")?.mask(Layer::scene(leaning))?])?;
    }
    Ok(leaning)
}

/// What a thread of 256 KB can do with a scene: draw it, print it, copy it and drop it. The pixels drawn come back.
fn on_a_small_stack(build: fn() -> Result<Scene, Error>) -> Result<usize, Box<dyn std::error::Error>> {
    let worked = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || -> Result<usize, String> {
            let nested = build().map_err(|error| error.to_string())?;
            let shown = format!("{nested:?}");
            let copy = nested.clone();
            let raster = copy.render(4).map_err(|error| error.to_string())?;
            drop(nested);
            Ok(raster.pixels().len() + shown.len().min(1))
        })?;
    Ok(worked.join().map_err(|_| "the thread panicked")??)
}

/// A nest of `MAX_DEPTH` scenes is drawn, printed, copied and dropped on a thread of 256 KB, and one scene more is refused
/// when it is added: nothing recurses past the bound, and the bound is on the nest.
#[test]
fn a_nest_of_sixteen_scenes_is_drawn_on_a_small_stack_and_seventeen_are_refused() -> Outcome {
    assert_eq!(MAX_DEPTH, 16);
    assert_eq!(on_a_small_stack(|| nest(MAX_DEPTH))?, 64 + 1);
    let message = refusal(scene(4, 4, [Layer::scene(nest(MAX_DEPTH)?)]))?;
    assert!(message.contains("16") && message.contains("17"), "{message}");
    // a nest used as a mask layer is a level more than a scene as a layer
    let message = refusal(Layer::fill("#ffffff")?.mask(Layer::scene(nest(MAX_DEPTH)?)))?;
    assert!(message.contains("17"), "{message}");
    Ok(())
}

/// Masks, scenes and references share the one depth: the worst mix at exactly 16 is drawn on a thread of 256 KB, and the limit
/// is exact for other mixes.
#[test]
fn masks_scenes_and_references_share_one_depth_and_the_limit_is_exact() -> Outcome {
    assert_eq!(on_a_small_stack(|| mixed(6, 4, 3))?, 64 + 1);
    assert_eq!(on_a_small_stack(|| mixed(0, 8, 4))?, 64 + 1);
    assert_eq!(on_a_small_stack(|| mixed(0, 0, 8))?, 64 + 1);
    for (links, plain, masked) in [
        (6, 4, 3),
        (6, 4, 4),
        (7, 4, 3),
        (6, 5, 3),
        (16, 0, 0),
        (0, 16, 0),
        (0, 15, 0),
        (0, 0, 8),
    ] {
        let accepted = mixed(links, plain, masked).is_ok();
        assert_eq!(
            accepted,
            links + plain + 2 * masked <= MAX_DEPTH,
            "{links} links, {plain} scenes, {masked} masks"
        );
    }
    Ok(())
}

/// The layers a scene draws are counted at each use of an inner scene: a group is one layer and the layers of its scene.
#[test]
fn the_layers_of_an_inner_scene_are_counted_at_each_use() -> Outcome {
    let of = |count: usize| -> Result<Scene, Error> {
        scene(
            2,
            2,
            (0..count)
                .map(|_| Layer::fill("#10203040"))
                .collect::<Result<Vec<_>, _>>()?,
        )
    };
    assert_eq!(MAX_LAYERS, 1_000);
    // two uses of a scene of 499 layers are 2 x (1 + 499)
    assert!(scene(2, 2, [Layer::scene(of(499)?), Layer::scene(of(499)?)]).is_ok());
    let message = refusal(scene(2, 2, [Layer::scene(of(499)?), Layer::scene(of(500)?)]))?;
    assert!(message.contains("1001") || message.contains("1002"), "{message}");
    // a scene of 600 layers used twice is 1,202, though it holds only 600
    let message = refusal(scene(2, 2, [Layer::scene(of(600)?), Layer::scene(of(600)?)]))?;
    assert!(message.contains("1202") && message.contains("1000"), "{message}");
    // a scene holds a thousand layers and is one layer more as a layer
    assert!(of(MAX_LAYERS).is_ok());
    assert!(refusal(scene(2, 2, [Layer::scene(of(MAX_LAYERS)?)])).is_ok());
    // a reference to a group draws it again, and counts again
    let group = || Layer::scene(of(499).unwrap_or_else(|_| unreachable!())).named("g");
    assert!(scene(2, 2, [group()?, Layer::of("g")]).is_ok());
    assert!(refusal(scene(2, 2, [group()?, Layer::of("g"), Layer::of("g")])).is_ok());
    // as the mask layer of a layer it counts at each use too
    let mask = Layer::scene(of(300)?);
    let masked = || -> Result<Layer, Error> { Layer::fill("#ffffff")?.mask(mask.clone()) };
    assert!(scene(2, 2, [masked()?, masked()?]).is_ok());
    assert!(refusal(scene(2, 2, [masked()?, masked()?, masked()?, masked()?])).is_ok());
    Ok(())
}

/// A reference to a layer that is a group draws the group again: it is a second copy of the group.
#[test]
fn a_reference_to_a_group_is_a_second_copy_of_it() -> Outcome {
    for size in [24, 40] {
        let group =
            || Layer::scene(inner().unwrap_or_else(|_| unreachable!())).place(Place::new().dx(3.5).rotate(Deg(9.0)));
        let by_reference = scene(
            40,
            30,
            [group()?.named("g")?, Layer::of("g").place(Place::new().dx(6.0))?],
        )?
        .render(size)?;
        let by_copy = scene(
            40,
            30,
            [
                group()?.named("g")?,
                group()?.place(Place::new().dx(9.5).rotate(Deg(9.0)))?,
            ],
        )?
        .render(size)?;
        assert!(most(&by_reference, &by_copy) <= 1e-6, "at {size}");
    }
    Ok(())
}
