//! The plugin seam of C7: a layer of a kind the caller supplies, asked for the rectangle and scale the scene draws it at.

use std::sync::Arc;

use bullterrier_scene::{Error, Fit, Layer, LayerKind, Place, Raster};
use serde_json::{Value, json};

use crate::common::counting::Counting;
use crate::common::{Outcome, exactly, pixel, scene};

#[test]
fn a_kind_is_asked_for_the_canvas_at_the_scale_of_the_output_and_what_it_draws_is_the_picture() -> Outcome {
    let kind = Counting::new(0.25);
    let picture = scene(100, 50, [Layer::make(kind.clone())])?.render(200)?;
    assert_eq!(kind.draws(), 1);
    assert_eq!(kind.questions(), vec![([0.0, 0.0, 100.0, 50.0], [2.0, 2.0])]);
    assert_eq!((picture.width(), picture.height()), (200, 100));
    // the kind's own raster for that question, bit for bit
    let direct = Counting::new(0.25).draw([0.0, 0.0, 100.0, 50.0], [2.0, 2.0])?;
    assert!(
        picture
            .pixels()
            .iter()
            .zip(direct.pixels())
            .all(|(got, expected)| got.to_bits() == expected.to_bits())
    );
    Ok(())
}

#[test]
fn a_kind_is_asked_for_the_part_of_the_canvas_that_fit_shows_and_the_layer_is_placed() -> Outcome {
    let kind = Counting::new(0.5);
    let layer = Layer::make(kind.clone())
        .fit([10.0, 10.0, 40.0, 40.0], Fit::Cover, [0.5, 0.5])?
        .place(Place::new().dx(5.0))?;
    let picture = scene(100, 50, [layer])?.render(100)?;
    // the kind's box is the canvas, 100 by 50; cover into a square of 40 shows a 50 by 50 part of it, at the scale 0.8
    assert_eq!(kind.draws(), 1);
    let ([_, _, width, height], scale) = kind.questions()[0];
    assert_eq!(((width, height), scale), ((50.0, 50.0), [0.8, 0.8]));
    // moved by 5: the 40 by 40 box starts at x = 15
    assert!(exactly(
        pixel(&picture, 15, 10),
        [0.5, pixel(&picture, 15, 10)[1], pixel(&picture, 15, 10)[2], 1.0]
    ));
    assert_eq!(pixel(&picture, 14, 10)[3].to_bits(), 0.0_f32.to_bits());
    Ok(())
}

/// A kind that draws a raster of the wrong size.
struct Wrong;

impl LayerKind for Wrong {
    fn name(&self) -> &'static str {
        "wrong"
    }

    fn options(&self) -> Value {
        Value::Null
    }

    fn draw(&self, _area: [f64; 4], _scale: [f64; 2]) -> Result<Raster, Error> {
        Raster::rgba(1, 1, vec![0.0; 4])
    }
}

#[test]
fn a_raster_of_the_wrong_size_is_refused_naming_both_sizes() -> Outcome {
    let refused = scene(10, 10, [Layer::make(Arc::new(Wrong)).named("odd")?])?.render(20);
    let Err(Error::Invalid { address, message }) = refused else {
        return Err("a raster of one pixel for 20 by 20 was accepted".into());
    };
    assert_eq!(address, "odd.make");
    assert!(message.contains("1 x 1") && message.contains("20 x 20"), "{message}");
    Ok(())
}

#[test]
fn a_layer_of_a_kind_has_no_json_form_and_equals_one_of_the_same_name_and_options() -> Outcome {
    let one = scene(10, 10, [Layer::make(Counting::new(0.1))])?;
    let refused = one.to_json();
    assert!(
        matches!(&refused, Err(Error::Invalid { address, .. }) if address.ends_with("make")),
        "{refused:?}"
    );
    assert_eq!(one, scene(10, 10, [Layer::make(Counting::new(0.1))])?);
    assert_ne!(one, scene(10, 10, [Layer::make(Counting::new(0.2))])?);
    assert_eq!(json!({"tone": 0.1}), Counting::new(0.1).options());
    Ok(())
}
