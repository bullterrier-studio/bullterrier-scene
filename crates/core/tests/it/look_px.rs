//! A look's `px` in canvas pixels (`Layer::look_in(Px::Canvas, ..)`): the oracle is a stroke measured on the rendered pixels,
//! not the formula that sizes it.

use bullterrier_scene::{Error, Fit, Layer, Px, Raster, Scene};

use crate::common::{Outcome, pixel, scene};

/// Dark lines `spacing` px apart, half of each period dark.
fn lines(spacing: f64) -> String {
    format!(
        r#"{{"steps": [{{"effect": "scanlines", "params": {{"spacing": {spacing}, "thickness": 0.5, "darkness": 0.8}}}}]}}"#
    )
}

/// A white fill with the lines.
fn ruled(size: (u32, u32), px: Px, spacing: f64) -> Result<Scene, Error> {
    scene(size.0, size.1, [Layer::fill("#ffffff")?.look_in(px, &lines(spacing))?])
}

/// The distance between the lines in the middle column, in output pixels: the first dark row to the last, over the gaps.
fn period(raster: &Raster) -> f64 {
    let column = raster.width() / 2;
    let dark = |y: usize| pixel(raster, column, y)[0] < 0.5;
    let starts: Vec<usize> = (0..raster.height())
        .filter(|&y| dark(y) && (y == 0 || !dark(y - 1)))
        .collect();
    assert!(starts.len() >= 2, "lines are seen: {starts:?}");
    #[expect(clippy::cast_precision_loss, reason = "a pixel count of a test picture")]
    {
        (starts[starts.len() - 1] - starts[0]) as f64 / (starts.len() - 1) as f64
    }
}

fn near(found: f64, expected: f64) -> bool {
    (found - expected).abs() <= 0.1 * expected.max(1.0)
}

#[test]
fn lines_in_canvas_pixels_are_that_many_pixels_of_the_canvas_at_any_output_size() -> Outcome {
    let size = (300, 2700);
    let canvas = ruled(size, Px::Canvas, 25.0)?;
    let (whole, half) = (period(&canvas.render(2700)?), period(&canvas.render(1350)?));
    assert!(near(whole, 25.0), "25 canvas pixels at 2700: {whole}");
    assert!(near(half, 12.5), "12.5 at 1350: {half}");
    // a design pixel is 1/480 of the long side
    let found = period(&ruled(size, Px::Design, 25.0)?.render(2700)?);
    assert!(near(found, 25.0 * 2700.0 / 480.0), "design 25 at 2700: {found}");
    Ok(())
}

/// A group whose canvas grew (a poster's story version: the same width, taller) keeps the lines of a canvas-pixel look; a
/// design-pixel look grows with the canvas's long side.
#[test]
fn inside_a_group_canvas_pixels_are_the_inner_scenes_so_a_taller_inner_canvas_keeps_the_lines() -> Outcome {
    let measured = |inner_height: u32, px: Px| -> Result<f64, Error> {
        let inner = ruled((600, inner_height), px, 20.0)?;
        let group = Layer::scene(inner).fit([0.0, 0.0, 600.0, 900.0], Fit::Cover, [0.5, 0.5])?;
        Ok(period(&scene(600, 900, [group])?.render(900)?))
    };
    let (short, tall) = (measured(900, Px::Canvas)?, measured(1067, Px::Canvas)?);
    assert!(near(short, 20.0) && near(tall, 20.0), "canvas px: {short} and {tall}");
    let (short, tall) = (measured(900, Px::Design)?, measured(1067, Px::Design)?);
    assert!(near(short, 20.0 * 900.0 / 480.0), "design px, 900 tall: {short}");
    assert!(near(tall, 20.0 * 1067.0 / 480.0), "design px, 1067 tall: {tall}");
    Ok(())
}
