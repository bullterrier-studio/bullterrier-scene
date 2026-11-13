//! A vector drawing as a layer, and the one place that knows how the vector crate reads and draws a document.

use bullterrier_vector::{Inputs, Size, Values};

use super::Source;
use crate::geometry::{Rect, Scale};
use crate::{Error, Raster};

/// A drawing with a frame. Its box is the frame at the canvas origin, one design pixel to a pixel of the frame.
#[derive(Debug)]
pub(crate) struct Drawing {
    document: bullterrier_vector::Drawing,
    width: f64,
    height: f64,
}

/// `error` of the vector crate as scene's: a document it cannot read keeps its pointer under `/drawing`, anything else is a
/// refused value at `drawing`.
fn refused(error: &bullterrier_vector::Error) -> Error {
    match error {
        bullterrier_vector::Error::Format { pointer, message } => Error::Format {
            pointer: format!("/drawing{pointer}"),
            message: format!("vector document: {message}"),
        },
        other => Error::invalid("drawing", other.to_string()),
    }
}

impl Drawing {
    /// The drawing the JSON document `json` describes, checked now by the vector crate.
    pub(crate) fn parse(json: &str) -> Result<Self, Error> {
        let document = bullterrier_vector::Drawing::from_json(json, &Inputs::new()).map_err(|error| refused(&error))?;
        let (width, height) = document.size().ok_or_else(|| {
            Error::invalid(
                "drawing",
                "a drawing layer needs a frame: build the drawing with a `size`",
            )
        })?;
        Ok(Self {
            document,
            width,
            height,
        })
    }
}

impl Source for Drawing {
    fn draw(&self, rect: Rect, scale: Scale) -> Result<Raster, Error> {
        let (width, height) = scale.layer_size(self.width, self.height)?;
        #[expect(clippy::cast_precision_loss, reason = "a side of at most 65,000 pixels")]
        let size = Size::px(width as f64, height as f64);
        let bytes = self
            .document
            .to_rgba(size, &Values::new())
            .map_err(|error| Error::invalid("drawing", error.to_string()))?;
        let pixels = bytes.iter().map(|&byte| f32::from(byte) / 255.0).collect();
        let placed = Raster::made(width, height, pixels);
        Ok(placed.window(
            scale.at(rect.x),
            scale.at(rect.y),
            scale.len(rect.width),
            scale.len(rect.height),
        ))
    }
}
