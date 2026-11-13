//! What a layer is made of: a source says how big its box is, then answers one question, "draw this part of it at this scale".
//! `picture` and `drawing` are the two adapters that know a sibling crate; the renderer knows neither.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use crate::geometry::{Rect, Scale};
use crate::{Error, Map, Raster};

pub(crate) mod drawing;
pub(crate) mod fill;
pub(crate) mod picture;

/// A map a picture layer is given, as it was given: held, a file read when the scene renders, or a hole (a map the caller
/// has not given yet, which a render refuses until parameters fill holes).
#[derive(Debug, Clone)]
pub(crate) enum Given {
    Held(Arc<Map>),
    File(PathBuf),
    Hole,
}

/// A mask by one of a picture's maps, as the layer's `mask` call gave it.
#[derive(Debug, Clone)]
pub(crate) struct MapMask {
    /// The map's name.
    pub(crate) name: String,
    /// The mask's place among the layer's `mask` calls, from 1: `mask`, `mask#2`...
    pub(crate) number: usize,
    /// The blur of the map's edge, in design pixels as the layer is fitted; 0 is none.
    pub(crate) feather: f64,
    /// Whether the mask is 1 minus the map (after the blur).
    pub(crate) invert: bool,
}

/// What the renderer asks of a source once it knows the size of the source's box.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Request {
    /// The part of the box wanted, in design pixels of the box.
    pub(crate) window: Rect,
    /// The output pixels a design pixel of the box should have.
    pub(crate) scale: Scale,
    /// The design pixels of the canvas a design pixel of the box is once the layer is fitted, along x and y: what a length
    /// in the layer's own space (a feather) is measured in.
    pub(crate) factor: (f64, f64),
    /// Where the top left of the source's whole box falls between two output pixels, as the fractions (from 0 to 1) of a pixel
    /// along x and y, when the layer's map keeps the raster's pixels (a move and a zoom, nothing turned, skewed, flipped
    /// or put in perspective); `(0, 0)` otherwise. A source that draws a raster with its content moved by these fractions,
    /// and says so in `covers`, has the warp put the raster on whole pixels: no resample, no soft edge.
    pub(crate) phase: (f64, f64),
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
    /// Whether the raster already ends, antialiased, where the window it was asked for ends, so that the warp does not cut it
    /// there again by coverage (a drawing's edge is drawn by vector: cutting it too would count the same pixel's cover twice).
    pub(crate) ends_at_window: bool,
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

    /// This source with the picture's map `mask.name` multiplied into its alpha, after its looks (stage 4 of a layer).
    ///
    /// # Errors
    /// The refusal of a source that has no such map.
    fn masked(&self, mask: &MapMask) -> Result<Arc<dyn Source>, Error> {
        Err(Error::invalid(
            "mask",
            format!(
                "{:?} names a map of this layer's picture, and this layer has no picture",
                mask.name
            ),
        ))
    }

    /// This source with the map `name` (of the picture's size when the scene renders), replacing a map of that name.
    ///
    /// # Errors
    /// The refusal of a source that has no picture.
    fn with_map(&self, _name: &str, _given: Given) -> Result<Arc<dyn Source>, Error> {
        Err(Error::invalid(
            "map",
            "a map belongs to a picture layer, and this layer has no picture",
        ))
    }
}
