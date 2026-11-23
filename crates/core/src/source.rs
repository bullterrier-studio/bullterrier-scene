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
use crate::finish::Px;
use crate::finish::{Chain, Finish};
use crate::geometry::{MAX_PIXELS, Rect, Scale, whole_up};
use crate::parameters::{Parameter, Value};
use crate::{Error, Map, Raster, Scene};

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
    /// The rows `first..end` of what the source would draw of its whole box (see [`Request::whole`]) that are wanted, for a source
    /// that can draw them alone (`Opened::by_window`); `None` for all of it. The renderer sets it when it knows which rows the
    /// output reads.
    pub(crate) rows: Option<Rows>,
}

/// The rows `first..end` of a source's whole raster that are wanted, of `total` rows in all: what a looked layer needs to run
/// its looks on part of the picture as the rows of the whole one (`Look::apply`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rows {
    pub(crate) first: usize,
    pub(crate) end: usize,
    pub(crate) total: usize,
}

/// What a source that draws its whole box answers with, before it draws: where the raster lies on the box and how big it is.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Shape {
    /// The part of the box the raster shows.
    pub(crate) covers: Rect,
    /// The raster's columns and rows, all of them (a part drawn by window is some rows of it).
    pub(crate) size: (usize, usize),
    /// Whether the raster ends where the window it was asked for ends (see [`Drawn::ends_at_window`]).
    pub(crate) ends_at_window: bool,
    /// The room past the window (see [`Drawn::room`]).
    pub(crate) room: [usize; 4],
}

impl Request {
    /// Drawn pixels per design pixel of the fitted layer, along x and y: what the layer is drawn at, less the fit.
    pub(crate) fn density(&self) -> (f64, f64) {
        (
            self.scale.x.pixels(1.0) / self.factor.0,
            self.scale.y.pixels(1.0) / self.factor.1,
        )
    }

    /// The scene's canvas as this layer's `place` scale draws it, in output pixels: what a look's `px` count in, whatever the
    /// layer's size (a side beyond u32 is as large as image can be told).
    pub(crate) fn looks_canvas(&self) -> (u32, u32) {
        let density = self.density();
        let side = |length: f64| u32::try_from(whole_up(length)).unwrap_or(u32::MAX);
        (side(self.canvas.0 * density.0), side(self.canvas.1 * density.1))
    }

    /// The canvas a look of unit `px` measures its `px` on: [`Request::looks_canvas`] for design pixels; for canvas pixels the same
    /// canvas shrunk by 480 over the scene canvas's long side (each side rounded, at least 1), so that image's design scale of it
    /// is one pixel of the scene's canvas as drawn.
    pub(crate) fn canvas_of(&self, px: Px) -> (u32, u32) {
        let (width, height) = self.looks_canvas();
        match px {
            Px::Design => (width, height),
            Px::Canvas => {
                let long = self.canvas.0.max(self.canvas.1).max(1.0);
                // clamped to 1 and to u32, so the cast is exact
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "rounded and clamped into 1 to u32::MAX"
                )]
                let shrink =
                    |side: u32| (f64::from(side) * 480.0 / long).round().clamp(1.0, f64::from(u32::MAX)) as u32;
                (shrink(width), shrink(height))
            }
        }
    }

    /// The shape of the whole box of `own` at this request, whatever its size.
    pub(crate) fn drawn_shape(&self, own: Rect) -> Shape {
        let covers = self.whole(own);
        Shape {
            covers,
            size: self.scale.sides(covers),
            ends_at_window: self.window == own,
            room: [0; 4],
        }
    }

    /// The shape of the whole box of `own` at this request, and the rows of it to draw: the rows asked for, or all. The limit of
    /// 2^27 pixels is on what is drawn, so a part of a large box is drawn where the whole of it is refused.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `size` for a part (or the whole) of more than 2^27 pixels.
    pub(crate) fn drawn_rows(&self, own: Rect) -> Result<(Shape, (usize, usize)), Error> {
        let shape = self.drawn_shape(own);
        Ok((shape, self.rows_within_limit(shape)?))
    }

    /// The shape of the window of a source that draws only the window (a picture), whatever its size.
    pub(crate) fn window_shape(&self) -> Shape {
        Shape {
            covers: self.window,
            size: self.scale.sides(self.window),
            ends_at_window: false,
            room: [0; 4],
        }
    }

    /// The rows of a raster of `shape` to draw: the rows asked for, or all, within the limit of 2^27 pixels.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `size` for a part (or the whole) of more than 2^27 pixels.
    pub(crate) fn rows_within_limit(&self, shape: Shape) -> Result<(usize, usize), Error> {
        let rows = match self.rows {
            None => {
                self.scale.size(shape.covers)?;
                (0, shape.size.1)
            }
            Some(Rows { first, end, .. }) => {
                if shape.size.0.saturating_mul(end - first) > MAX_PIXELS {
                    return Err(Error::invalid(
                        "size",
                        format!(
                            "the layer is drawn {} x {} pixels at a time; at most {MAX_PIXELS} pixels (2^27) are drawn: lower `size` or make the layer smaller",
                            shape.size.0,
                            end - first
                        ),
                    ));
                }
                (first, end)
            }
        };
        Ok(rows)
    }
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
    /// The pixels the raster reaches past the window on each side (top, right, bottom, left): the room a light layer's looks
    /// spread into, shown whole. `[0; 4]` for every other raster, which ends at the window or where `covers` says.
    pub(crate) room: [usize; 4],
    /// The maps the layer's looks and masks use, each at the raster's size, resampled as the raster was: only from a source
    /// that has maps and was asked to draw at the size a look runs at (`Finish::is_looked`). The finish consumes them.
    pub(crate) maps: BTreeMap<String, Vec<f32>>,
}

impl Drawn {
    /// The shape of this answer.
    pub(crate) fn shape(&self) -> Shape {
        Shape {
            covers: self.covers,
            size: (self.raster.width(), self.raster.height()),
            ends_at_window: self.ends_at_window,
            room: self.room,
        }
    }
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
    /// Whether it can draw some rows of its whole box alone (`Request::rows`), a pixel the same as in the whole.
    pub(crate) by_window: bool,
    /// Whether, with no look to run on it, it draws no more than its own pixels (a picture's, a fill's one), whatever the scale
    /// it is asked for: the warp resamples them, so nothing of the drawn size is allocated.
    pub(crate) own_pixels: bool,
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
            by_window: false,
            own_pixels: false,
            draw: Box::new(draw),
        }
    }

    /// This source, saying that it draws the whole of its box.
    pub(crate) fn drawing_whole(mut self) -> Self {
        self.whole = true;
        self
    }

    /// This source, saying that with no look on it draws no more than its own pixels.
    pub(crate) fn own_pixels(mut self) -> Self {
        self.own_pixels = true;
        self
    }

    /// This source, saying that it can draw some rows of its whole box alone, and each pixel of them is what it is in the whole.
    pub(crate) fn by_window(mut self) -> Self {
        self.by_window = true;
        self
    }

    /// The part of the box that `request` asks for, with the layer's looks and masks by its own maps (stages 3 and 4) run on
    /// it: the one place they run, whatever the source.
    ///
    /// # Errors
    /// What the source cannot draw, and a look the sibling refuses at this size. With `chain`, the stages of a chain of looks
    /// are kept in `cache` and the source is not asked at all when a stage of the chain is there to resume from.
    pub(crate) fn draw(
        self,
        request: Request,
        finish: &Finish,
        light: bool,
        cache: &Cache,
        chain: Option<&Chain>,
    ) -> Result<Drawn, Error> {
        finish.run(|| (self.draw)(request, finish, cache), &request, light, cache, chain)
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

    /// The scene this source draws, if it is a group: what a hit test looks inside.
    fn group(&self) -> Option<&Scene> {
        None
    }

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
