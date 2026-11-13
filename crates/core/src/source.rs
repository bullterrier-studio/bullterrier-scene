//! What a layer is made of: a source answers one question, "draw this rectangle of the canvas at this scale".
//! `picture` and `drawing` are the two adapters that know a sibling crate; the renderer knows neither.

use std::fmt;
use std::sync::Arc;

use serde_json::Value;

use crate::geometry::{Rect, Scale};
use crate::{Error, Raster};

pub(crate) mod drawing;
pub(crate) mod fill;
pub(crate) mod picture;

/// Something a layer draws.
pub(crate) trait Source: Send + Sync + fmt::Debug {
    /// The part of the layer that lies in `rect` (design pixels of the canvas), drawn at `scale`: a picture of
    /// `scale.len(rect.width)` by `scale.len(rect.height)` pixels, straight RGBA, transparent where the layer draws
    /// nothing. Content outside the canvas is cut.
    ///
    /// # Errors
    /// What the source cannot draw: a file that cannot be read, a look the sibling refuses at this size.
    fn draw(&self, rect: Rect, scale: Scale) -> Result<Raster, Error>;

    /// This source with the look written as the recipe document `recipe` run on it. Only a picture takes a look in this
    /// release (looks on every layer arrive with batch C5).
    ///
    /// # Errors
    /// What the sibling crate refuses in the recipe, and the refusal of a source that takes no look.
    fn looked(&self, _recipe: &Value) -> Result<Arc<dyn Source>, Error> {
        Err(Error::invalid(
            "look",
            "a look runs on a picture layer; looks on other layers arrive with batch C5",
        ))
    }
}
