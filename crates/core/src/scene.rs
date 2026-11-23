//! The canvas and its stack of layers.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use crate::budget::Budget;
use crate::cache::Cache;
use crate::pdf::PdfStream;
use crate::stream::PngStream;
use crate::{Error, Layer, PdfOptions, Plates, Raster, WriteOptions, hit, plates, reference, render};

/// The longest a canvas side is, in design pixels.
const MAX_CANVAS: u32 = 100_000;

/// A canvas and a stack of layers, bottom first, drawn into one picture at any size. A scene is an immutable value: every
/// call returns a new one, and the old one can be used again.
///
/// A scene keeps what its renders drew (the raster of each layer before it is placed, the planes of its masks, the pictures it
/// read from files) in one bounded cache that every scene made from it by [`add`](Scene::add) and
/// [`with_values`](Scene::with_values) shares, so that a render redraws only what depends on what changed. The cache is
/// never part of the value: it changes no picture, and `==` and `to_json` do not see it. Scenes made apart (two calls of
/// [`Scene::new`], a scene read from JSON twice) share nothing.
#[derive(Debug, Clone)]
pub struct Scene {
    width: u32,
    height: u32,
    /// The layers as they were added.
    layers: Vec<Layer>,
    /// The same layers as they are drawn: each reference replaced by the body it draws.
    pub(crate) drawn: Vec<Layer>,
    /// What drawing the scene costs, kept as layers are added so that a check is a sum, not a walk.
    pub(crate) budget: Budget,
    /// What renders of this scene and the scenes made from it kept, shared by all of them.
    cache: Arc<Cache>,
}

impl Scene {
    /// An empty canvas of `width` by `height` design pixels: whole numbers, 1 to 100,000. A design pixel is what a length
    /// in the scene means.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `canvas` for a side outside 1 to 100,000.
    pub fn new(width: impl Into<i128>, height: impl Into<i128>) -> Result<Self, Error> {
        let (width, height) = (width.into(), height.into());
        let side = |value: i128| u32::try_from(value).ok().filter(|side| (1..=MAX_CANVAS).contains(side));
        let (Some(width), Some(height)) = (side(width), side(height)) else {
            return Err(Error::invalid(
                "canvas",
                format!("a canvas side is a whole number from 1 to {MAX_CANVAS}, got {width} x {height}"),
            ));
        };
        Ok(Self {
            width,
            height,
            layers: Vec::new(),
            drawn: Vec::new(),
            budget: Budget::NONE,
            cache: Arc::new(Cache::new()),
        })
    }

    /// This scene with a new, empty cache that keeps at most `bytes` bytes of pixels, planes and decoded files (2 GiB unless
    /// set), shared by every scene made from it: a browser or a phone sets less, `0` keeps nothing. The picture is the same
    /// whatever the budget; a budget below what a render reuses draws again what it would have found. The cache this scene
    /// shared is left to the scenes that hold it.
    #[must_use]
    pub fn with_cache_budget(&self, bytes: usize) -> Self {
        Self {
            cache: Arc::new(Cache::with_capacity(bytes)),
            ..self.clone()
        }
    }

    /// A scene of the same canvas with no layer, sharing this scene's cache: the start of a scene made from this one.
    pub(crate) fn emptied(&self) -> Self {
        Self {
            width: self.width,
            height: self.height,
            layers: Vec::new(),
            drawn: Vec::new(),
            budget: Budget::NONE,
            cache: Arc::clone(&self.cache),
        }
    }

    /// A scene of the same canvas that draws `drawn`, layers already resolved, with this scene's cache: what a plate draws.
    pub(crate) fn drawing(&self, drawn: Vec<Layer>) -> Self {
        Self {
            drawn,
            ..self.emptied()
        }
    }

    /// The layers as they are drawn, references resolved.
    pub(crate) fn drawn(&self) -> &[Layer] {
        &self.drawn
    }

    /// This scene with `layers` on top of its stack, the first lowest.
    ///
    /// A reference ([`Layer::of`]) names a layer of the scene that is there already or is in `layers`, whichever comes
    /// first in the stack: it is drawn where it is in the stack.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `layers` for more layers drawn than [`MAX_LAYERS`](crate::MAX_LAYERS) (the layers, the mask
    /// layers they are masked by and the references, a layer used twice counted twice) or leaning on other layers deeper
    /// than [`MAX_DEPTH`](crate::MAX_DEPTH); `name` for a name used twice (the message names both layers) or a `layer<n>`
    /// name at any place but the `n`th; `<layer>.of` for a reference that names no layer of the scene or whose chain leads
    /// back to itself, also through a mask layer; `<layer>.look` and `<layer>.mask` for a look or a mask by a map that the
    /// layer a reference names cannot take.
    pub fn add(&self, layers: impl IntoIterator<Item = Layer>) -> Result<Self, Error> {
        let mut stack = self.layers.clone();
        stack.extend(layers);
        // what the layers cost as they were written: enough to stop a stack far past the limit before it is walked
        stack
            .iter()
            .fold(Budget::NONE, |budget, layer| budget.beside(layer.budget()))
            .checked("layers")?;
        let mut places: HashMap<String, usize> = HashMap::new();
        for (index, layer) in stack.iter().enumerate() {
            let place = index + 1;
            let own = format!("layer{place}");
            let name = layer.name.clone().unwrap_or_else(|| own.clone());
            if name
                .strip_prefix("layer")
                .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
                && name != own
            {
                return Err(Error::invalid(
                    "name",
                    format!(
                        "{name} is a default name, and only the layer at place {} may have it; layer {place} is called {name}",
                        &name[5..]
                    ),
                ));
            }
            if let Some(first) = places.insert(name.clone(), index) {
                return Err(Error::invalid(
                    "name",
                    format!(
                        "{name} is the name of layer {} and of layer {place}: a name is used once in a scene",
                        first + 1
                    ),
                ));
            }
        }
        let mut drawn = self.drawn.clone();
        let resolved = reference::resolve(&stack, &self.drawn, &places)?;
        let budget = resolved
            .iter()
            .fold(self.budget, |budget, layer| budget.beside(layer.budget()))
            .checked("layers")?;
        drawn.extend(resolved);
        Ok(Self {
            width: self.width,
            height: self.height,
            layers: stack,
            drawn,
            budget,
            cache: Arc::clone(&self.cache),
        })
    }

    /// A scene of this canvas with `layers` as its stack, bottom first, sharing this scene's cache: an editor's change to the
    /// tree of layers (add, remove, reorder, regroup into a [`Layer::scene`]) is a new list, from [`layers`](Scene::layers),
    /// and a layer that did not change is not drawn again. A named layer keeps its addresses wherever it goes (`x.<field>`,
    /// or `g.scene.x.<field>` inside the group `g`); one without a name is `layer<n>` by its new place. This scene is left as
    /// it is.
    ///
    /// # Errors
    /// As [`add`](Scene::add) on an empty canvas: a name used twice, a reference that names no layer of the new stack, too
    /// many layers or too deep.
    pub fn with_layers(&self, layers: impl IntoIterator<Item = Layer>) -> Result<Self, Error> {
        self.emptied().add(layers)
    }

    /// The plates of the scene at `size`: one plane of ink coverage for each ink its layers name ([`Layer::inks`]), in the order
    /// the inks first appear, bottom layer first. Plate `i` is what the scene draws when only the layers that print in ink `i`
    /// are on it, each as the shape of its ink (the plane its look separates, or its alpha), placed, masked and faded as it is
    /// in the picture, and joined over the others of the same ink as an overprint: where two shapes of one ink meet, `a` over `b`
    /// is `a + b (1 - a)`. A layer's blend mode is not asked (ink on ink is an overprint whatever the blend); the plates
    /// say what each ink prints, and they make the picture where the layers of different inks are laid one over the other as
    /// inks are, by multiplying (`paper * prod (1 - coverage * (1 - ink / paper))`: an ink over its own paper is its colour). Layers that name no ink, the paper among them, are
    /// not on a plate. Inks inside a group (a layer that is a scene) are not handed on; the group is one shape.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `size` for a size outside 1 to 65,000, or planes of more than 2^27 values in all (the columns
    /// times the rows times the inks; said before anything is drawn); `inks` for a scene whose layers name no ink, a layer that
    /// is light, a layer with no look that names more than one ink, and a look that makes no ink planes or a number of them that
    /// is not the number of names; whatever a layer cannot draw, as for [`render`](Scene::render).
    pub fn plates(&self, size: impl Into<i128>) -> Result<Plates, Error> {
        plates::make(self, size.into())
    }

    /// The layers, bottom first.
    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// The canvas's width in design pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The canvas's height in design pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// What a render draws: the canvas, the layers as they are drawn, and the cache the scene shares.
    #[cfg(feature = "gpu")]
    pub(crate) fn parts(&self) -> (u32, u32, &[Layer], &Cache) {
        (self.width, self.height, &self.drawn, &self.cache)
    }

    /// The picture of the scene: straight alpha, transparent where nothing is drawn, 0 to 1, sRGB values. `size` is the long
    /// side in output pixels, a whole number from 1 to 65,000, and the picture has at most 2^27 pixels in all; the canvas
    /// keeps its aspect, so one design pixel is `size / max(width, height)` output pixels and the short side is rounded up.
    /// A smaller size shows the same picture in less detail. Reads the files the scene names and writes none.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `size` for a size outside its range or a picture of more than 2^27 pixels; whatever a
    /// layer cannot draw, naming the layer: [`Error::Read`] for a file, [`Error::Invalid`] for a look or a drawing the
    /// sibling crate refuses at this size.
    pub fn render(&self, size: impl Into<i128>) -> Result<Raster, Error> {
        render::render(self.width, self.height, &self.drawn, size.into(), &self.cache)
    }

    /// The address of the topmost layer that shows at the pixel (`x`, `y`) of [`render(size)`](Scene::render)'s picture (column
    /// and row from the top left), or `None` where none does: the start of its [`parameters`](Scene::parameters) addresses (its
    /// name, `layer<n>` for one without, `group.scene.inner` for a layer inside a group).
    ///
    /// A layer shows at a pixel where, drawn alone as the render draws it (placed, in perspective, looked, masked, at its
    /// opacity, light), it leaves alpha that an 8-bit picture keeps (at least 0.5 / 255): a blend mode changes its colour, not
    /// where it is, and a layer at opacity 0 or masked away is not there. The layers are asked top first. In a group, the
    /// answer is the topmost inner layer that shows there alone through the group's place, looks and masks, or the group if
    /// none does alone. A mask layer is never the answer; a reference to a group is the answer itself (it has no addresses
    /// inside). Each layer asked is drawn for the one row of the pixel, as a band of a render is: a look image can band runs
    /// on that row and the margin it reaches each time, one it cannot is drawn whole at `size` once and kept.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `size` for a size outside 1 to 65,000, `x` or `y` for a pixel outside the picture; whatever
    /// a layer cannot draw, as for [`render`](Scene::render).
    pub fn layer_at(
        &self,
        x: impl Into<i128>,
        y: impl Into<i128>,
        size: impl Into<i128>,
    ) -> Result<Option<String>, Error> {
        hit::layer_at(
            (self.width, self.height),
            (&self.layers, &self.drawn),
            (x.into(), y.into()),
            size.into(),
            &self.cache,
        )
    }

    /// The columns and rows of the picture at `size`, the long side in output pixels: what [`render`](Scene::render) and
    /// [`render_bands`](Scene::render_bands) make, whatever the number of pixels.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `size` for a size outside 1 to 65,000.
    pub fn picture_size(&self, size: impl Into<i128>) -> Result<(usize, usize), Error> {
        render::picture_size(self.width, self.height, size.into())
    }

    /// The picture of [`render`](Scene::render), drawn a band of rows at a time: `sink` is given each band in order, top to
    /// bottom, as the rows it is (`0..band_rows`, then the next `band_rows`, the last with what is left) and the band's picture
    /// (the whole width, those rows). The bands put together are the picture `render` makes, within the one tolerance, whatever
    /// `band_rows`. The picture is never held whole: what is held is a band, the scene's cache and what the layers need
    /// whole (below).
    ///
    /// **What is drawn by band and what whole.** A layer with no look is drawn for the rows of a band alone where its source
    /// can be: a picture is read from where the band falls, a flat colour, a mask layer and the blend of a pixel are per
    /// pixel. A layer with a look is drawn whole once, at the size it is drawn at, and cut per band (it stays held: the
    /// floor of a looked layer), and so is a source that cannot draw a part. The limit of 2^27 pixels is on what is held at
    /// once, so a picture past it can be drawn by bands; `render` keeps it.
    ///
    /// A refusal of `sink` ends the render and is returned as it is; nothing more is drawn.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `size` for a size outside its range, `band_rows` for a number of rows that is 0, more than the
    /// picture has or makes a band of more than 2^27 pixels; whatever a layer cannot draw, as for `render`; and what `sink`
    /// returns.
    pub fn render_bands(
        &self,
        size: impl Into<i128>,
        band_rows: usize,
        mut sink: impl FnMut(Range<usize>, &Raster) -> Result<(), Error>,
    ) -> Result<(), Error> {
        render::render_bands(
            (self.width, self.height),
            &self.drawn,
            size.into(),
            band_rows,
            &self.cache,
            &mut sink,
        )
    }

    /// The picture of [`render`](Scene::render) written to `path` as an 8-bit RGBA PNG (straight alpha, sRGB values, the PNG's
    /// `sRGB` chunk), drawn by bands of [`BAND_PIXELS`](crate::BAND_PIXELS) pixels and written as each is made: a picture past
    /// 2^27 pixels can be written, and what is held is a band (see [`render_bands`](Scene::render_bands) for what else). Each
    /// level is rounded as [`Raster::write`] rounds it. The file is made under a temporary name beside `path` and put in place
    /// when it is whole: a refusal leaves no file, and a file already there stays as it was.
    ///
    /// # Errors
    /// As [`render_bands`](Scene::render_bands) for the picture, [`Error::Write`] naming `path` for a path that cannot be
    /// written.
    pub fn render_to_png(&self, size: impl Into<i128>, path: &Path) -> Result<(), Error> {
        self.render_to_png_with(size, path, &WriteOptions::new())
    }

    /// [`render_to_png`](Scene::render_to_png) with the `options`: the resolution the file says it prints at, in the `pHYs` chunk
    /// as [`Raster::write_with`] writes it.
    ///
    /// # Errors
    /// As [`render_to_png`](Scene::render_to_png).
    pub fn render_to_png_with(&self, size: impl Into<i128>, path: &Path, options: &WriteOptions) -> Result<(), Error> {
        let size = size.into();
        let (columns, rows) = self.picture_size(size)?;
        let mut png = PngStream::create(path, columns, rows, options.dpi)?;
        self.render_bands(size, render::default_band_rows(columns, rows), |_, band| png.push(band))?;
        png.finish()
    }

    /// The picture of [`render`](Scene::render) written to `path` as a one-page PDF, as [`Raster::write_pdf`] writes a picture
    /// (the same page, boxes and image), drawn by bands and written as each is made, with no limit on the pixels of the
    /// image. What is held is a band and a few buffers, never the picture or the compressed image: that goes to two scratch
    /// files beside `path` (as large as the compressed image) which are removed when the PDF is in place or the render fails. The file is put in place when it is whole, as for [`render_to_png`](Scene::render_to_png).
    ///
    /// # Errors
    /// As [`render_bands`](Scene::render_bands) for the picture, and as [`Raster::write_pdf`] for the options and the page.
    pub fn render_to_pdf(&self, size: impl Into<i128>, path: &Path, options: &PdfOptions) -> Result<(), Error> {
        let size = size.into();
        let (columns, rows) = self.picture_size(size)?;
        let mut pdf = PdfStream::create(path, (columns, rows), options, usize::MAX)?;
        self.render_bands(size, render::default_band_rows(columns, rows), |_, band| pdf.push(band))?;
        pdf.finish()
    }
}
