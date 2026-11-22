//! A scene rendered on the GPU is the CPU's picture within a named bound: flat colours and their join first.

use bullterrier_scene::{Blend, Layer};

use super::{gap_to_cpu, gpu};
use crate::common::{Outcome, scene};

/// Per channel, f32 against f32 on one pipeline of the same maths: Metal measured 0 to 1e-6 here; 1e-4 is image's bound for a
/// per-pixel kernel.
const BOUND: f32 = 1e-4;

#[test]
fn fills_over_one_another_are_the_cpu_picture() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let scene = scene(
        40,
        30,
        [
            Layer::fill("#204080")?,
            Layer::fill("#ff8000")?.opacity(0.5)?,
            Layer::fill("#00ff80")?.opacity(0.25)?,
        ],
    )?;
    let (gap, at) = gap_to_cpu(&gpu, &scene, 40)?;
    assert!(gap <= BOUND, "gap {gap} at value {at}");
    Ok(())
}

#[test]
fn a_scene_of_nothing_is_transparent_and_a_blend_of_fills_is_the_cpu_picture() -> Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    let empty = scene(20, 10, [])?;
    let (gap, _) = gap_to_cpu(&gpu, &empty, 20)?;
    assert_eq!(gap, 0.0);
    let multiplied = scene(20, 10, [Layer::fill("#806040")?, Layer::fill("#40a0ff")?.blend(Blend::Multiply).opacity(0.7)?])?;
    let (gap, at) = gap_to_cpu(&gpu, &multiplied, 20)?;
    assert!(gap <= BOUND, "gap {gap} at value {at}");
    Ok(())
}
