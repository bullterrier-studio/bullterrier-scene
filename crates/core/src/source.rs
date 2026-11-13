//! What a layer is made of: a source says how big its box is, then answers one question, "draw this part of it at this scale".
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
    /// The part of the box the raster shows: the requested window, or more when the source cannot draw a part, at the scale
    /// it can (a picture without a look hands over its own pixels, a fill one pixel). Its density is the raster's size over
    /// this.
    pub(crate) covers: Rect,
}

/// A source that has said how big its box is and waits to be told what to draw. The renderer decides the request (and
/// refuses one that is too large) between the two, so no source has to remember to.
pub(crate) struct Opened<'a> {
    /// The source's whole box, in design pixels, its top left at `(0, 0)`.
    pub(crate) own: Rect,
    /// Whether it draws the whole box whatever part is asked for (a vector drawing), so that what it allocates is the box at
    /// the scale, not the part.
    pub(crate) whole: bool,
    draw: Box<dyn FnOnce(Request) -> Result<Drawn, Error> + 'a>,
}

impl<'a> Opened<'a> {
    /// A source of box `own` that answers a request by `draw`.
    pub(crate) fn new(own: Rect, draw: impl FnOnce(Request) -> Result<Drawn, Error> + 'a) -> Self {
        Self {
            own,
            whole: false,
            draw: Box::new(draw),
        }
    }

    /// This source, saying that it draws the whole of its box.
    pub(crate) fn drawing_whole(mut self) -> Self {
        self.whole = true;
        self
    }

    /// The part of the box that `request` asks for.
    ///
    /// # Errors
    /// What the source cannot draw: a look the sibling refuses at this size.
    pub(crate) fn draw(self, request: Request) -> Result<Drawn, Error> {
        (self.draw)(request)
    }
}

/// Something a layer draws.
pub(crate) trait Source: Send + Sync + fmt::Debug {
    /// Opens this source: reads what gives its box its size (a file is read here, once) and says how big the box is.
    /// `canvas` is the scene's canvas, the box of a source that has none of its own.
    ///
    /// # Errors
    /// A file that cannot be read.
    fn open(&self, canvas: Rect) -> Result<Opened<'_>, Error>;

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
