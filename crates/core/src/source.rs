//! What a layer is made of: a source says how big its box is, then answers one question, "draw this part of it at this scale".
//! `picture` and `drawing` are the two adapters that know a sibling crate; the renderer knows neither. The looks and masks of a
//! layer are not the source's: they run on what it draws (`finish`).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::budget::Budget;
use crate::cache::{Cache, Keyed};
use crate::document::Form;
use crate::files::Located;
use crate::finish::Finish;
use crate::geometry::{Rect, Scale, whole_up};
use crate::parameters::{Parameter, Value};
use crate::{Error, Map, Raster};

pub(crate) mod drawing;
pub(crate) mod fill;
pub(crate) mod make;
pub(crate) mod picture;
pub(crate) mod scene;

/// A map a picture layer is given, as it was given: held, a file read when the scene renders, or a hole (a map the caller
/// has not given yet, which a render refuses until `with_values` fills it).
#[derive(Debug, Clone)]
pub(crate) enum Given {
    Held(Arc<Map>),
    File(Located),
    Hole,
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
    /// The scene's canvas in design pixels, width and height: what a look's design scale is measured on (the renderer sets it,
    /// `Layout::request` leaves it 0).
    pub(crate) canvas: (f64, f64),
    /// The pixels the scenes this one is drawn in (as a group, one inside another) hold in their own rasters, which stay alive
    /// while this is drawn: the renderer sets it, `Layout::request` leaves it 0.
    pub(crate) held: usize,
}

impl Request {
    /// The part of the box `own` that a source drawing the whole of it draws: the box, from `phase` before its top left to the
    /// whole pixels that hold it, at the density of `scale`. The warp then copies the raster onto whole pixels.
    pub(crate) fn whole(&self, own: Rect) -> Rect {
        let (across, down) = (self.scale.x.pixels(1.0), self.scale.y.pixels(1.0));
        // whole pixels, a side of at most 2^27: exact in a float
        #[expect(clippy::cast_precision_loss, reason = "a raster side of at most 2^27 pixels")]
        let (columns, rows) = (
            whole_up(self.phase.0 + own.width * across) as f64,
            whole_up(self.phase.1 + own.height * down) as f64,
        );
        Rect {
            x: own.x - self.phase.0 / across,
            y: own.y - self.phase.1 / down,
            width: columns / across,
            height: rows / down,
        }
    }
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
    /// The maps the layer's looks and masks use, each at the raster's size, resampled as the raster was: only from a source
    /// that has maps and was asked to draw at the size a look runs at (`Finish::is_looked`). The finish consumes them.
    pub(crate) maps: BTreeMap<String, Vec<f32>>,
}

/// How a source answers: told the request and what the layer's finish needs.
type Draw<'a> = dyn FnOnce(Request, &Finish, &Cache) -> Result<Drawn, Error> + 'a;

/// A source that has said how big its box is and waits to be told what to draw. The renderer decides the request (and
/// refuses one that is too large) between the two, so no source has to remember to.
pub(crate) struct Opened<'a> {
    /// The source's whole box, in design pixels, its top left at `(0, 0)`.
    pub(crate) own: Rect,
    /// Whether it draws the whole box whatever part is asked for (a vector drawing), so that what it allocates is the box at
    /// the scale, not the part.
    pub(crate) whole: bool,
    draw: Box<Draw<'a>>,
}

impl<'a> Opened<'a> {
    /// A source of box `own` that answers a request by `draw`, at the size it is asked for: the layer's looks run on the
    /// raster it gives back. `draw` is given the cache of the render, for a source that draws layers of its own.
    pub(crate) fn new(own: Rect, draw: impl FnOnce(Request, &Cache) -> Result<Drawn, Error> + 'a) -> Self {
        Self::finishing(own, move |request, _, cache| draw(request, cache))
    }

    /// A source of box `own` that answers a request by `draw`, and that has more to hand over when the layer runs looks on
    /// it (its own pixels, a map): `draw` is told what the layer's finish needs.
    pub(crate) fn finishing(
        own: Rect,
        draw: impl FnOnce(Request, &Finish, &Cache) -> Result<Drawn, Error> + 'a,
    ) -> Self {
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

    /// The part of the box that `request` asks for, with the layer's looks and masks by its own maps (stages 3 and 4) run on
    /// it: the one place they run, whatever the source.
    ///
    /// # Errors
    /// What the source cannot draw, and a look the sibling refuses at this size.
    pub(crate) fn draw(self, request: Request, finish: &Finish, light: bool, cache: &Cache) -> Result<Drawn, Error> {
        let drawn = (self.draw)(request, finish, cache)?;
        finish.run(drawn, &request, light)
    }
}

/// Something a layer draws.
pub(crate) trait Source: Send + Sync + fmt::Debug {
    /// What drawing a layer of this source costs: one layer, unless it draws layers of its own (a scene).
    fn budget(&self) -> Budget {
        Budget::LAYER
    }

    /// Opens this source: reads what gives its box its size (a file is read here, once, or taken from `cache` if an earlier
    /// render read it) and says how big the box is. `canvas` is the scene's canvas, the box of a source that has none of its
    /// own.
    ///
    /// # Errors
    /// A file that cannot be read.
    fn open(&self, canvas: Rect, cache: &Cache) -> Result<Opened<'_>, Error>;

    /// Writes into `key` everything this source draws from, besides the request it is asked to answer: what two sources with the
    /// same key and the same request draw alike. The one place each source says what its picture depends on: a source that
    /// leaves an input out is caught by the oracle of `tests/it/cache.rs`.
    ///
    /// # Errors
    /// A source whose document cannot be written.
    fn keyed(&self, key: &mut Keyed) -> Result<(), Error>;

    /// Whether this source draws on the grid it is put on: at the pixel phase of `Request::phase` (a layer that moves by a
    /// fraction of a pixel is drawn again) and, for a scene, within the pixels `Request::held` of the scenes round it. A source
    /// that does not (a picture, a fill, a kind) is asked once for a layer that only moves, and again only for another window or
    /// scale.
    fn follows_grid(&self) -> bool {
        false
    }

    /// Whether what this source draws is worth keeping even when its layer has no look and no mask by a map: it takes work
    /// (a drawing, a group, a kind of the caller). A picture without a look hands over its own pixels and a fill draws one
    /// pixel: nothing to keep.
    fn is_costly(&self) -> bool {
        false
    }

    /// The keys this source adds to its layer's document, in order (`picture` and `maps`, `drawing`, `text`...). The document
    /// is "Saved scene" in `docs/api.md`; `form` says whether it is written to be saved or to be compared.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a picture or a map held in memory when the document is to be saved, [`Error::Read`] for a file
    /// whose digest cannot be made.
    fn document(&self, form: Form) -> Result<Vec<(&'static str, serde_json::Value)>, Error>;

    /// The names of the maps this source holds, or `None` if it has no picture to hold them.
    fn map_names(&self) -> Option<Vec<&str>> {
        None
    }

    /// The parameters of this source, at addresses relative to the layer: `picture`, `map.<name>`, `text`, `color`,
    /// `drawing.<param>` or `scene.<address>`, in order.
    fn parameters(&self) -> Vec<Parameter> {
        Vec::new()
    }

    /// This source with each of `values` set: an address `parameters` lists, and a value. Nothing is set unless every one is
    /// accepted.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming the address, for a value of the wrong kind or outside its range.
    fn with_values(&self, values: &[(&str, &Value)]) -> Result<Arc<dyn Source>, Error> {
        let address = values.first().map_or("values", |(address, _)| address);
        Err(Error::invalid(address, "this source has no parameter of that address"))
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
