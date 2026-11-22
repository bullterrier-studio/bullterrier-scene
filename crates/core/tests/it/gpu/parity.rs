//! Each piece of the compositor against the CPU's: a warp of each kind, each blend mode on noise, opacity, masks (alpha,
//! luminance, feathered, inverted, two at once), light, a group and an average past 2x. Seeded noise pictures, small canvases;
//! the CPU render is the oracle and each bound is named beside its case, measured on Metal and kept with room for lavapipe.

use bullterrier_scene::{Blend, Deg, Fit, Layer, Mask, Place, Raster, Scene, Use};

use super::{gap_to_cpu, gpu};
use crate::common::{Outcome, scene};

/// The canvas of every case, in design pixels, drawn 1:1.
const CANVAS: (u32, u32) = (60, 40);

/// A picture of seeded noise: every channel an independent value, alpha from 0.25 to 1.
fn noise(width: usize, height: usize, seed: u64) -> Result<Raster, bullterrier_scene::Error> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        #[expect(clippy::cast_precision_loss, reason = "the top 24 bits of a random word")]
        {
            (state >> 40) as f32 / (1_u64 << 24) as f32
        }
    };
    let pixels = (0..width * height)
        .flat_map(|_| [next(), next(), next(), 0.25 + 0.75 * next()])
        .collect();
    Raster::rgba(width, height, pixels)
}

/// The canvas's box in design pixels.
const BOX: [f64; 4] = [0.0, 0.0, 60.0, 40.0];

/// A picture of `size` stretched over the canvas.
fn stretched(size: (usize, usize), seed: u64) -> Result<Layer, bullterrier_scene::Error> {
    Layer::picture(noise(size.0, size.1, seed)?).fit(BOX, Fit::Stretch, [0.5, 0.5])
}

/// A ground of noise under what a case puts on it, so alpha and every blend have a backdrop to meet.
fn ground() -> Result<Layer, bullterrier_scene::Error> {
    stretched((60, 40), 1)
}

fn over(layer: Layer) -> Result<Scene, bullterrier_scene::Error> {
    scene(CANVAS.0, CANVAS.1, [ground()?, layer])
}

fn within(scene: &Scene, bound: f32, what: &str) -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let (gap, at) = gap_to_cpu(&gpu, scene, CANVAS.0.max(CANVAS.1))?;
    assert!(gap <= bound, "{what}: gap {gap} at value {at} (bound {bound})");
    Ok(())
}

/// A bilinear or an area average over the same maths in 32-bit floats: the positions are 32-bit on the device.
const EXACT_CLASS: f32 = 1e-4;

#[test]
fn a_picture_the_size_of_the_canvas_is_the_cpu_picture() -> Outcome {
    within(&over(stretched((60, 40), 2)?)?, EXACT_CLASS, "identity")
}

#[test]
fn a_move_by_whole_pixels_copies_the_pixels() -> Outcome {
    within(
        &over(stretched((60, 40), 2)?.place(Place::new().dx(7.0).dy(-3.0))?)?,
        EXACT_CLASS,
        "move",
    )
}

#[test]
fn a_picture_scaled_up_or_down_or_flipped_is_the_cpu_picture() -> Outcome {
    within(&over(stretched((37, 29), 3)?)?, EXACT_CLASS, "scaled up")?;
    within(
        &over(stretched((60, 40), 3)?.place(Place::new().scale(0.6))?)?,
        EXACT_CLASS,
        "scaled down",
    )?;
    within(
        &over(stretched((60, 40), 3)?.place(Place::new().flip_x(true).flip_y(true))?)?,
        EXACT_CLASS,
        "flipped",
    )
}

#[test]
fn a_picture_shrunk_past_2x_is_averaged_down_as_on_the_cpu() -> Outcome {
    within(&over(stretched((300, 200), 4)?)?, EXACT_CLASS, "reduced")?;
    within(
        &over(stretched((300, 200), 4)?.place(Place::new().scale(0.7).dx(5.0))?)?,
        EXACT_CLASS,
        "reduced and moved",
    )
}

/// The footprint filter in 32-bit floats: the lengths of the footprint's axes and the angle are worked out in 64 on the CPU for an
/// affine map (and passed), so the taps are the same; the positions are 32-bit.
const FOOTPRINT: f32 = 2e-3;

#[test]
fn a_turn_a_skew_and_a_perspective_are_the_cpu_picture_within_the_footprints_bound() -> Outcome {
    within(
        &over(stretched((60, 40), 5)?.place(Place::new().rotate(Deg(17.0)))?)?,
        FOOTPRINT,
        "rotate 17",
    )?;
    within(
        &over(stretched((60, 40), 5)?.place(Place::new().skew_x(Deg(35.0)))?)?,
        FOOTPRINT,
        "skew",
    )?;
    within(
        &over(stretched((60, 40), 5)?.place(Place::new().rotate(Deg(90.0)))?)?,
        EXACT_CLASS,
        "quarter turn",
    )?;
    // the footprint counts its taps by the length of its axes: where the CPU's 64-bit length is just over a whole number and the
    // device's 32-bit one under it, a pixel takes one tap fewer; a perspective is looser than the affine maps
    let layer = stretched((60, 40), 5)?.perspective([[5.0, 4.0], [55.0, 8.0], [58.0, 36.0], [8.0, 32.0]])?;
    within(&over(layer)?, 1e-2, "perspective")
}

/// A blend's formulas in 32-bit floats against 32-bit floats: a division (colour-dodge, burn, the non-separable modes) amplifies.
const BLEND: f32 = 1e-3;

#[test]
fn each_of_the_sixteen_blend_modes_on_noise_is_the_cpu_picture() -> Outcome {
    let names = [
        "normal",
        "multiply",
        "screen",
        "overlay",
        "darken",
        "lighten",
        "color-dodge",
        "color-burn",
        "hard-light",
        "soft-light",
        "difference",
        "exclusion",
        "hue",
        "saturation",
        "color",
        "luminosity",
    ];
    for name in names {
        let blend: Blend = name.parse()?;
        within(&over(stretched((60, 40), 6)?.blend(blend).opacity(0.8)?)?, BLEND, name)?;
    }
    Ok(())
}

#[test]
fn opacity_and_light_are_the_cpu_picture() -> Outcome {
    for opacity in [0.0, 0.3, 1.0] {
        within(
            &over(stretched((60, 40), 7)?.opacity(opacity)?)?,
            EXACT_CLASS,
            "opacity",
        )?;
    }
    within(
        &over(stretched((60, 40), 7)?.as_light(true).blend("screen".parse()?))?,
        BLEND,
        "light",
    )
}

#[test]
fn masks_by_alpha_and_luminance_feathered_and_inverted_are_the_cpu_picture() -> Outcome {
    let mask = || stretched((30, 20), 8);
    within(
        &over(stretched((60, 40), 9)?.mask(Mask::new(mask()?))?)?,
        EXACT_CLASS,
        "alpha mask",
    )?;
    within(
        &over(stretched((60, 40), 9)?.mask(Mask::new(mask()?).using(Use::Luminance))?)?,
        EXACT_CLASS,
        "luminance mask",
    )?;
    within(
        &over(stretched((60, 40), 9)?.mask(Mask::new(mask()?).invert())?)?,
        EXACT_CLASS,
        "inverted mask",
    )?;
    // the feather: three box passes in 32-bit floats against 64-bit sums
    within(
        &over(stretched((60, 40), 9)?.mask(Mask::new(mask()?).feather(3.0))?)?,
        5e-4,
        "feathered mask",
    )?;
    within(
        &over(stretched((60, 40), 9)?.mask(Mask::new(mask()?).feather(1.5).invert())?)?,
        5e-4,
        "feathered and inverted",
    )?;
    // two masks at once: their product
    let two = stretched((60, 40), 9)?
        .mask(Mask::new(mask()?))?
        .mask(Mask::new(stretched((20, 30), 10)?).feather(2.0))?;
    within(&over(two)?, 5e-4, "two masks")
}

#[test]
fn a_group_is_the_cpu_picture() -> Outcome {
    let inner = scene(
        60,
        40,
        [stretched((60, 40), 11)?, stretched((20, 20), 12)?.opacity(0.5)?],
    )?;
    within(&over(Layer::scene(inner).opacity(0.7)?)?, EXACT_CLASS, "group")
}
