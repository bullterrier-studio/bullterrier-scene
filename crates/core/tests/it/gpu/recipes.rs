//! Whole scenes against the CPU's, in the budget of the determinism note (interim, until the owner's side-by-side look): after
//! rounding to 8 bits, at most one level apart on 99.9% of values and two on all. Three scenes built after demos 06 (a two-ink
//! poster), 07 (a composed world with a shadow, a reflection and a glow) and 09 (the song poster's burst, keyline and group), with
//! seeded pictures where the demos have photos, and kernel looks (`grade`, `levels`, `curves`) where they have halftones: a look
//! that thresholds (a dither, a halftone) turns a difference of 1e-4 before it into a whole level after it, so its budget is image's.

use bullterrier_scene::{Blend, Deg, Error, Fit, Layer, Mask, Place, Raster, Scene};

use super::{gpu, rendered};
use crate::common::{Outcome, scene};

/// A smooth "photo": gradients and a few blobs, so a look has tones to grade, with a seeded fine grain.
fn photo(width: usize, height: usize, seed: u64) -> Result<Raster, Error> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut grain = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        #[expect(clippy::cast_precision_loss, reason = "the top 24 bits of a random word")]
        {
            (state >> 40) as f32 / (1_u64 << 24) as f32
        }
    };
    let pixels = (0..width * height)
        .flat_map(|at| {
            #[expect(clippy::cast_precision_loss, reason = "small counts")]
            let (x, y) = ((at % width) as f32 / width as f32, (at / width) as f32 / height as f32);
            let blob = (-(((x - 0.5).powi(2) + (y - 0.45).powi(2)) * 18.0)).exp();
            let noise = 0.04 * grain();
            [
                (0.2 + 0.6 * x * blob + noise).clamp(0.0, 1.0),
                (0.15 + 0.5 * y + 0.3 * blob + noise).clamp(0.0, 1.0),
                (0.6 - 0.4 * x + 0.2 * blob + noise).clamp(0.0, 1.0),
                1.0,
            ]
        })
        .collect();
    Raster::rgba(width, height, pixels)
}

/// A disc of `radius` at `centre` on a frame, as a drawing.
fn disc(frame: (u32, u32), centre: (f64, f64), radius: f64, colour: &str) -> String {
    let path: String = (0..48)
        .map(|step| {
            let angle = f64::from(step) * std::f64::consts::TAU / 48.0;
            let (x, y) = (centre.0 + radius * angle.cos(), centre.1 + radius * angle.sin());
            format!("{}{x:.3} {y:.3}", if step == 0 { "M" } else { " L" })
        })
        .collect();
    format!(
        r#"{{"root":[{{"stack":{{"items":[[{{"path":"{path} Z"}}]],"size":[{},{}]}}}},{{"fill":{{"color":"{colour}"}}}}],"version":1}}"#,
        frame.0, frame.1
    )
}

/// `bars` thin spokes round a centre (the song poster's burst), as one drawing.
fn burst(frame: (u32, u32), centre: (f64, f64), inner: f64, outer: f64, bars: u32, colour: &str) -> String {
    let items: Vec<String> = (0..bars)
        .map(|bar| {
            let angle = f64::from(bar) * std::f64::consts::TAU / f64::from(bars);
            let reach = inner + (outer - inner) * (0.4 + 0.6 * ((f64::from(bar) * 0.7).sin().abs()));
            let (sin, cos) = angle.sin_cos();
            let corner = |radius: f64, side: f64| {
                (
                    centre.0 + radius * cos - side * sin,
                    centre.1 + radius * sin + side * cos,
                )
            };
            let points = [
                corner(inner, -3.0),
                corner(reach, -3.0),
                corner(reach, 3.0),
                corner(inner, 3.0),
            ];
            format!(
                r#"[{{"path":"M{:.3} {:.3} L{:.3} {:.3} L{:.3} {:.3} L{:.3} {:.3} Z"}}]"#,
                points[0].0, points[0].1, points[1].0, points[1].1, points[2].0, points[2].1, points[3].0, points[3].1
            )
        })
        .collect();
    format!(
        r#"{{"root":[{{"stack":{{"items":[{}],"size":[{},{}]}}}},{{"fill":{{"color":"{colour}"}}}}],"version":1}}"#,
        items.join(","),
        frame.0,
        frame.1
    )
}

const GRADE: &str = r#"{"steps": [{"effect": "grade", "params": {"contrast": 1.45, "saturation": 1.2}}, {"effect": "levels", "params": {"gamma": 1.1}}]}"#;

/// Demo 06: paper, a disc, the graded photo in a window, rings, a band in front, a sticker turned as a group.
fn two_ink_poster() -> Result<Scene, Error> {
    let frame = (180, 270);
    let window = crate::common::rectangle(frame, (12.0, 70.0), (168.0, 225.0), "#ffffff");
    let sticker = scene(
        60,
        13,
        [
            Layer::fill("#ff3d86")?,
            Layer::fill("#16123f")?
                .place(Place::new().dx(3.0).dy(3.0).scale(0.9))?
                .opacity(0.7)?,
        ],
    )?;
    scene(
        frame.0,
        frame.1,
        [
            Layer::fill("#f3e9d6")?,
            Layer::drawing(&disc(frame, (140.0, 40.0), 30.0, "#16123f"))?,
            Layer::picture(photo(120, 180, 1)?)
                .fit([-5.0, 45.0, 185.0, 225.0], Fit::Cover, [0.5, 0.5])?
                .look(GRADE)?
                .mask(Mask::new(Layer::drawing(&window)?))?,
            Layer::drawing(&disc(frame, (19.0, 206.0), 41.0, "#ff3d86"))?
                .opacity(0.8)?
                .mask(Mask::new(Layer::drawing(&window)?).invert())?,
            Layer::drawing(&crate::common::rectangle(
                frame,
                (0.0, 230.0),
                (180.0, 270.0),
                "#16123f",
            ))?,
            Layer::scene(sticker).place(Place::new().dx(100.0).dy(210.0).rotate(Deg(-6.0)))?,
            Layer::fill("#808080")?.blend(Blend::Overlay).opacity(0.25)?,
        ],
    )
}

/// Demo 07: a sky, a grid in perspective, a sun, the dog with a shadow, a reflection and a glow, all references to one photo.
fn composed_world() -> Result<Scene, Error> {
    let frame = (160, 200);
    let area = [0.0, 0.0, 160.0, 200.0];
    scene(
        frame.0,
        frame.1,
        [
            Layer::picture(photo(80, 100, 2)?)
                .fit(area, Fit::Stretch, [0.5, 0.5])?
                .look(GRADE)?
                .opacity(0.9)?,
            Layer::drawing(&disc(frame, (80.0, 70.0), 40.0, "#ff7a30"))?.blend(Blend::Screen),
            Layer::picture(photo(64, 40, 3)?)
                .fit([0.0, 120.0, 160.0, 80.0], Fit::Stretch, [0.5, 0.5])?
                .perspective([[40.0, 120.0], [120.0, 120.0], [170.0, 200.0], [-10.0, 200.0]])?,
            Layer::picture(photo(60, 80, 4)?)
                .fit([40.0, 40.0, 80.0, 110.0], Fit::Cover, [0.5, 0.5])?
                .named("dog")?,
            Layer::of("dog")
                .place(Place::new().dx(10.0).dy(70.0).scale(0.9).flip_x(false))?
                .blend(Blend::Multiply)
                .opacity(0.5)?,
            Layer::of("dog")
                .place(Place::new().dy(105.0).anchor(0.5, 1.0).aspect(1.0).scale(1.0))?
                .opacity(0.35)?
                .mask(
                    Mask::new(Layer::picture(photo(40, 40, 5)?).fit(
                        [40.0, 110.0, 80.0, 80.0],
                        Fit::Stretch,
                        [0.5, 0.5],
                    )?)
                    .feather(6.0),
                )?,
            Layer::fill("#40a0ff")?
                .as_light(true)
                .blend(Blend::Screen)
                .opacity(0.4)?,
        ],
    )
}

/// Demo 09: the burst of bars, the graded photo, a keyline from a feathered mask, a group at another scale and angle, an overlay.
fn wow_poster() -> Result<Scene, Error> {
    let frame = (180, 270);
    let group = scene(
        80,
        80,
        [
            Layer::fill("#0d0b14")?,
            Layer::drawing(&burst((80, 80), (40.0, 40.0), 10.0, 36.0, 40, "#ff4a14"))?,
        ],
    )?;
    scene(
        frame.0,
        frame.1,
        [
            Layer::fill("#0d0b14")?,
            Layer::drawing(&burst(frame, (90.0, 105.0), 52.0, 85.0, 120, "#f5ebd3"))?.opacity(0.9)?,
            Layer::picture(photo(100, 140, 6)?)
                .fit([12.0, 30.0, 156.0, 200.0], Fit::Cover, [0.5, 0.5])?
                .look(GRADE)?
                .mask(Mask::new(Layer::drawing(&disc(frame, (90.0, 130.0), 70.0, "#ffffff"))?).feather(4.0))?,
            Layer::drawing(&disc(frame, (90.0, 130.0), 74.0, "#ff4a14"))?
                .opacity(0.8)?
                .mask(
                    Mask::new(Layer::drawing(&disc(frame, (90.0, 130.0), 70.0, "#ffffff"))?)
                        .feather(2.0)
                        .invert(),
                )?,
            Layer::scene(group)
                .place(Place::new().dx(100.0).dy(190.0).scale(0.9).rotate(Deg(14.0)))?
                .opacity(0.9)?,
            Layer::fill("#ff4a14")?
                .as_light(true)
                .blend(Blend::ColorDodge)
                .opacity(0.2)?,
            Layer::fill("#808080")?.blend(Blend::Overlay).opacity(0.2)?,
        ],
    )
}

/// The fraction of values more than one level apart and the largest gap, in 8-bit levels.
fn levels_apart(cpu: &Raster, gpu: &Raster) -> (f64, u32) {
    assert_eq!(cpu.pixels().len(), gpu.pixels().len());
    let level = |value: f32| (value * 255.0 + 0.5) as u32;
    let (mut over_one, mut worst) = (0_usize, 0_u32);
    for (left, right) in cpu.pixels().iter().zip(gpu.pixels()) {
        assert!(left.is_finite() && right.is_finite());
        let apart = level(*left).abs_diff(level(*right));
        over_one += usize::from(apart > 1);
        worst = worst.max(apart);
    }
    #[expect(clippy::cast_precision_loss, reason = "counts of values")]
    (over_one as f64 / cpu.pixels().len() as f64, worst)
}

#[test]
fn three_scenes_after_the_demos_hold_the_recipe_budget() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    for (name, scene) in [
        ("two-ink poster", two_ink_poster()?),
        ("composed world", composed_world()?),
        ("song poster", wow_poster()?),
    ] {
        let size = 270;
        let on_cpu = scene.render(size)?;
        let on_gpu = rendered(&gpu, &scene, size)?;
        let (share, worst) = levels_apart(&on_cpu, &on_gpu);
        assert!(
            share <= 0.001,
            "{name}: {share:.5} of values are more than one level apart"
        );
        assert!(worst <= 2, "{name}: a value is {worst} levels apart");
    }
    Ok(())
}
