//! A layer: a source and how it joins what is below it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::blend::Blend;
use crate::mask::{By, CanvasMask, Mask, Use};
use crate::perspective::Quad;
use crate::placement::{Fit, Fitting, Place, Placement};
use crate::scene::MAX_LAYERS;
use crate::source::drawing::Drawing;
use crate::source::fill::Fill;
use crate::source::picture::{Picture, recipe_file};
use crate::source::{Given, MapMask, Source};
use crate::text::Text;
use crate::{Error, Map, Raster};

/// The deepest masks nest: a layer's mask layer, that layer's own mask layers, and so on. A layer with no masks has depth 0
/// and a mask layer's depth counts from the layer it masks; a `mask` call that would make a nest deeper is refused. Scenes
/// used as layers (batch C4) count in the same depth.
pub const MAX_DEPTH: usize = 16;

/// One layer of a scene: a source, and how it joins the layers below it. A layer is an immutable value: every call
/// returns a new one, and a layer can be used in several scenes, or twice in one.
#[derive(Debug, Clone)]
pub struct Layer {
    pub(crate) name: Option<String>,
    pub(crate) opacity: f32,
    pub(crate) blend: Blend,
    /// Whether the layer's alpha is made from the brightness of its colour (stage 7).
    pub(crate) light: bool,
    pub(crate) source: Arc<dyn Source>,
    pub(crate) fit: Option<Fitting>,
    pub(crate) placement: Placement,
    /// Masks by layers drawn on the canvas (stage 6), in call order; those by a map are in the source.
    pub(crate) masks: Vec<CanvasMask>,
    /// How many `mask` calls this layer has had: the number of the next is one more.
    mask_calls: usize,
    /// How deep its masks nest, and how many mask layers there are in all in its tree (a shared layer counted each time).
    pub(crate) depth: usize,
    mask_layers: usize,
}

/// `name` if it is letters, digits and `_`, starting with a letter: the one rule of a layer's name and a map's.
fn check_name(field: &str, name: &str) -> Result<(), Error> {
    let mut characters = name.chars();
    let starts = characters.next().is_some_and(|first| first.is_ascii_alphabetic());
    if starts && characters.all(|next| next.is_ascii_alphanumeric() || next == '_') {
        Ok(())
    } else {
        Err(Error::invalid(
            field,
            format!("a name is letters, digits and `_`, starting with a letter, got {name:?}"),
        ))
    }
}

impl Layer {
    fn of(source: impl Source + 'static) -> Self {
        Self {
            name: None,
            opacity: 1.0,
            blend: Blend::Normal,
            light: false,
            source: Arc::new(source),
            fit: None,
            placement: Placement::default(),
            masks: Vec::new(),
            mask_calls: 0,
            depth: 0,
            mask_layers: 0,
        }
    }

    /// A picture: its box is its pixels, one design pixel each, at the canvas origin; what lies outside the canvas is cut.
    /// It is resampled once to the output size, by bilinear interpolation of premultiplied colour, or by an area average
    /// where it is reduced.
    #[must_use]
    pub fn picture(raster: Raster) -> Self {
        Self::of(Picture::held(raster))
    }

    /// A picture read from a file when the scene renders: as [`Layer::picture`] draws it, upright, in sRGB values, with its
    /// alpha if it has one. A file that cannot be read is an [`Error::Read`] at `render`, naming the layer.
    #[must_use]
    pub fn picture_file(path: impl Into<PathBuf>) -> Self {
        Self::of(Picture::file(path.into()))
    }

    /// A vector drawing, given as its JSON document (what the vector package's `Drawing.to_json()` writes): its box is its
    /// frame at the canvas origin, one design pixel to a pixel of the frame, rasterised by vector at the output size; what
    /// lies outside the canvas is cut. The document is checked now.
    ///
    /// # Errors
    /// [`Error::Format`] (pointer under `/drawing`) for a document the linked vector crate cannot read, a version it does
    /// not read included; [`Error::Invalid`] naming `drawing` for a drawing without a frame.
    pub fn drawing(json: &str) -> Result<Self, Error> {
        Ok(Self::of(Drawing::parse(json)?))
    }

    /// `content` set in type: `text` says the font, the colour and vector's text options (`size`, `axes`, `tracking`, `leading`,
    /// `align`, `width`, `fit`, `features`), checked now by the linked vector crate. The layer's origin is the text's anchor,
    /// the left end, the middle or the right end of the first baseline by `align`, so `place(dx, dy)` puts it there; its box is
    /// the glyphs' bounds. It is drawn by vector at the size it will have, so it is sharp at
    /// any `scale`.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `font` for a bundled font that does not exist (the message lists them) or a file that is no
    /// font, `content` for a text with no glyph to draw, the option for one vector refuses (in its words) and for `anchor`,
    /// `inside`, `runs`, expressions and Params; [`Error::Read`] for a font file that cannot be read.
    pub fn text(content: &str, text: &Text) -> Result<Self, Error> {
        Ok(Self::of(Drawing::text(content, text)?))
    }

    /// A flat colour over the whole canvas: `#rrggbb` or `#rrggbbaa`, in sRGB values; the alpha is straight.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `fill` for any other text.
    pub fn fill(color: &str) -> Result<Self, Error> {
        Ok(Self::of(Fill::parse(color)?))
    }

    /// This layer with an image look run on it, after the looks it has: `json` is the recipe document that the image package's
    /// `Recipe.to_dict()` writes, checked now by the linked image crate. The look runs on the picture at the output size, on
    /// its colour with the alpha left as it is, and its `px` values are image's own (a picture whose long side is 480); a
    /// look that sets `size` or `frame` is refused. Only a picture layer takes a look in this release.
    ///
    /// # Errors
    /// [`Error::Format`] if `json` is not a JSON object or carries a recipe `format` the linked image crate does not read;
    /// [`Error::Invalid`] naming `look` for anything image refuses, in its words, and for a layer that is no picture.
    pub fn look(mut self, json: &str) -> Result<Self, Error> {
        let recipe = serde_json::from_str(json).map_err(|error| Error::Format {
            pointer: "/look".to_owned(),
            message: format!("not JSON: {error}"),
        })?;
        self.source = self.source.looked(&recipe)?;
        Ok(self)
    }

    /// This layer with the look of the recipe file `path` (TOML, as the image package writes it), read now.
    ///
    /// # Errors
    /// [`Error::Read`] if the file cannot be read; otherwise as [`Layer::look`].
    pub fn look_file(mut self, path: impl AsRef<Path>) -> Result<Self, Error> {
        self.source = self.source.looked(&recipe_file(path.as_ref())?)?;
        Ok(self)
    }

    /// This layer called `name`: letters, digits and `_`, starting with a letter; unique in its scene. A layer without a
    /// name is `layer1`, `layer2`... by its place in the scene.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `name` for any other text.
    pub fn named(mut self, name: &str) -> Result<Self, Error> {
        check_name("name", name)?;
        self.name = Some(name.to_owned());
        Ok(self)
    }

    /// This picture layer with `map` under `name`: a value from 0 to 1 for each pixel of the picture (white = the part a
    /// matte keeps), cut and fitted with the picture. A look sees the maps its recipe uses (`subject`, `depth`, a `given`
    /// region), and `mask(name)` multiplies by it. A map of the same name given before is replaced. The map must be
    /// the picture's size when the scene renders (`Error::Invalid` naming the layer and `map.<name>`, with both sizes,
    /// then).
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `map` for a name that is not letters, digits and `_` starting with a letter, and for a
    /// layer that is no picture.
    pub fn map(self, name: &str, map: Map) -> Result<Self, Error> {
        self.given(name, Given::Held(Arc::new(map)))
    }

    /// This picture layer with the grey picture in the file `path` as its map `name`, read when the scene renders (as
    /// [`Layer::picture_file`] is): upright, 8 or 16 bits, white = 1. A file that cannot be read is an [`Error::Read`] at
    /// `render`, naming the layer.
    ///
    /// # Errors
    /// As [`Layer::map`].
    pub fn map_file(self, name: &str, path: impl Into<PathBuf>) -> Result<Self, Error> {
        self.given(name, Given::File(path.into()))
    }

    /// This picture layer with a hole under the map `name`: a map to be given later. Until parameters fill holes (batch
    /// C6) a render refuses it, naming the layer and `map.<name>`.
    ///
    /// # Errors
    /// As [`Layer::map`].
    pub fn map_hole(self, name: &str) -> Result<Self, Error> {
        self.given(name, Given::Hole)
    }

    fn given(mut self, name: &str, given: Given) -> Result<Self, Error> {
        check_name("map", name)?;
        self.source = self.source.with_map(name, given)?;
        Ok(self)
    }

    /// This layer's box fitted into `area` = `[x, y, width, height]` (design pixels of the canvas); the last `fit` wins.
    /// `Fit::Cover` scales it, its aspect kept, to fill `area`, and what overflows is cut off at `area`'s edge;
    /// `Fit::Contain` scales it to lie wholly inside; `Fit::Stretch` fills `area` exactly, its aspect not kept. `align` is
    /// two fractions from 0 to 1 of the layer's box: which part stays when cover cuts, and where a contained layer sits in
    /// the slack. It is resolved when the scene renders, from the source's real size, so a picture of any size fills the
    /// same area; `place` then acts on the fitted box (the cut `area` for cover, the contained rectangle for contain).
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `box` for a size not above 0 or a number that is not finite, and `align` for a fraction
    /// outside 0 to 1.
    pub fn fit(mut self, area: [f64; 4], how: Fit, align: [f64; 2]) -> Result<Self, Error> {
        self.fit = Some(Fitting::new(area, how, align)?);
        Ok(self)
    }

    /// This layer with the fields `place` sets changed and the others kept: see [`Place`] for what each does and the order
    /// they apply in. The result is one resample of the finished raster; the source is drawn at the size it will have, so
    /// a drawing at `scale` 3 is sharp and a look grows with it.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming the field for a `scale` or `aspect` outside 0.001 to 1000, a skew outside -89 to 89, a `dx`
    /// or `dy` outside -1e6 to 1e6, an `anchor` outside 0 to 1, or a number that is not finite.
    pub fn place(mut self, place: Place) -> Result<Self, Error> {
        self.placement = self.placement.placed(place)?;
        Ok(self)
    }

    /// This layer with the corners of its fitted box put on `corners`, the places its top left, top right, bottom right and
    /// bottom left land on, in design pixels of the canvas. It replaces what `place` says (the last of `place` and
    /// `perspective` that is called wins); `fit` still comes first, so the corners are those of the fitted box. The
    /// corners must make a strictly convex quadrilateral, which a perspective maps a rectangle to and nothing else; one
    /// that turns counter-clockwise is accepted and is the box seen from behind, a mirror. The layer is drawn at the
    /// largest scale the quadrilateral gives it, so the near side is sharp, cut down if that would pass the 2^27 pixels
    /// of a layer; the far side is filtered, not aliased.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `corners` for a number that is not finite or is beyond -1e6 to 1e6, and for corners that
    /// are not a strictly convex quadrilateral (a dart, a bowtie, three on a line, a corner twice).
    pub fn perspective(mut self, corners: [[f64; 2]; 4]) -> Result<Self, Error> {
        self.placement = Placement::Perspective(Quad::new(corners)?);
        Ok(self)
    }

    /// This layer with its alpha multiplied by `mask`: a layer drawn on the canvas as its own `fit` and `place` say (stage 6),
    /// or the name of one of this picture layer's maps, cut and fitted with it (stage 4). Calling it again multiplies
    /// another mask in. A mask layer's own alpha includes its masks, its opacity multiplies the mask, and it has no name;
    /// a layer used as a mask is drawn on the canvas grid only (with a margin where its blur reaches past it).
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `feather` for a number outside 0 to 1,000,000; `use` for a map read as luminance; `mask`
    /// for a name that is no map of this layer (it has no picture, or no map of that name yet), for a mask layer that
    /// has a name, for masks nested more than [`MAX_DEPTH`] deep, and for more than 1,000 mask layers in all.
    pub fn mask(mut self, mask: impl Into<Mask>) -> Result<Self, Error> {
        let mask = mask.into();
        mask.checked()?;
        let number = self.mask_calls + 1;
        match &mask.by.0 {
            By::Map(name) => {
                if mask.uses == Use::Luminance {
                    return Err(Error::invalid(
                        "use",
                        "a map is its own values: read it as \"alpha\", not \"luminance\"",
                    ));
                }
                self.source = self.source.masked(&MapMask {
                    name: name.clone(),
                    number,
                    feather: mask.feather,
                    invert: mask.invert,
                })?;
            }
            By::Layer(by) => {
                if let Some(name) = &by.name {
                    return Err(Error::invalid(
                        "mask",
                        format!("a mask layer has no name (it is owned by the layer it masks), got {name:?}"),
                    ));
                }
                if by.depth + 1 > MAX_DEPTH {
                    return Err(Error::invalid(
                        "mask",
                        format!(
                            "masks nest at most {MAX_DEPTH} deep, and this mask layer has masks {} deep",
                            by.depth
                        ),
                    ));
                }
                let layers = self.mask_layers + 1 + by.mask_layers;
                if layers > MAX_LAYERS {
                    return Err(Error::invalid(
                        "mask",
                        format!(
                            "a layer holds at most {MAX_LAYERS} mask layers in all, nested ones counted each time, got {layers}"
                        ),
                    ));
                }
                self.depth = self.depth.max(by.depth + 1);
                self.mask_layers = layers;
                self.masks.push(CanvasMask::new((**by).clone(), &mask, number));
            }
        }
        self.mask_calls = number;
        Ok(self)
    }

    /// This layer at `value` of its own alpha: 0 hides it, 1 (the default) leaves it as it is.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `opacity` unless `value` is a number from 0 to 1.
    pub fn opacity(mut self, value: f64) -> Result<Self, Error> {
        if !(0.0..=1.0).contains(&value) {
            return Err(Error::invalid(
                "opacity",
                format!("opacity is a number from 0 to 1, got {value}"),
            ));
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a value from 0 to 1 is held as a 32-bit float"
        )]
        {
            self.opacity = value as f32;
        }
        Ok(self)
    }

    /// This layer joined to what is below by `blend` (default [`Blend::Normal`]), in sRGB values, after its masks and its
    /// opacity: where the layer lands it is blended with the colour below by the W3C compositing equation.
    #[must_use]
    pub fn blend(mut self, blend: Blend) -> Self {
        self.blend = blend;
        self
    }

    /// This layer made light (`true`), or not (`false`, the default). A light layer's premultiplied colour, after its looks
    /// and masks, is its colour over black, and its alpha is the largest of the red, green and blue of that colour, so what
    /// is black adds nothing, a `screen` of the layer over black gives that colour exactly, and white is opaque white. A
    /// layer that is not light keeps the alpha of its source.
    #[must_use]
    pub fn as_light(mut self, on: bool) -> Self {
        self.light = on;
        self
    }
}
