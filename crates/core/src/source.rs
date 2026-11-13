//! What a layer is made of: a source answers one question, "draw this rectangle of your own box at this scale".
//! `picture` and `drawing` are the two adapters that know a sibling crate; the renderer knows neither.

use std::fmt;
use std::sync::Arc;

use serde_json::Value;

use crate::geometry::{Rect, Scale};
use crate::{Error, Raster};

pub(crate) mod drawing;
pub(crate) mod fill;
pub(crate) mod picture;

/// What the renderer asks of a source once it knows the size of the source's box.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Request {
    /// The part of the box wanted, in design pixels of the box (its top left is `(0, 0)`).
    pub(crate) window: Rect,
    /// The output pixels a design pixel of the box should have.
    pub(crate) scale: Scale,
}

/// What a source answers.
#[derive(Debug)]
pub(crate) struct Drawn {
    /// Straight RGBA, `covers` divided evenly into its pixels. Held, so that a picture can hand over its own pixels.
    pub(crate) raster: Arc<Raster>,
    /// The source's whole box, in design pixels, its top left at `(0, 0)`.
    pub(crate) own: Rect,
    /// The part of the box the raster shows: the requested window, or more when the source cannot draw a part, at the scale
    /// it can (a picture without a look hands over its own pixels, a fill one pixel). Its density is the raster's size over
    /// this.
    pub(crate) covers: Rect,
}

/// Something a layer draws.
pub(crate) trait Source: Send + Sync + fmt::Debug {
    /// Draws the part of this source's box that `plan` asks for. `plan` is given the box (a source whose size is known only
    /// when it is read, a file, is read once) and gives the window and the scale; `canvas` is the scene's canvas, the box of
    /// a source that has none of its own.
    ///
    /// # Errors
    /// What `plan` refuses, and what the source cannot draw: a file that cannot be read, a look the sibling refuses at this
    /// size.
    fn draw(&self, canvas: Rect, plan: &dyn Fn(Rect) -> Result<Request, Error>) -> Result<Drawn, Error>;

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
