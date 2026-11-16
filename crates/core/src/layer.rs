//! A layer: a source and how it joins what is below it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::blend::Blend;
use crate::budget::{Budget, MAX_DEPTH};
use crate::cache::Keyed;
use crate::files::Located;
use crate::finish::{Finish, MapMask};
use crate::mask::{By, CanvasMask, Mask, Use};
use crate::perspective::Quad;
use crate::placement::{Fit, Fitting, Place, Placed, Placement, Px};
use crate::reference::Reference;
use crate::source::drawing::Drawing;
use crate::source::fill::Fill;
use crate::source::make::Make;
use crate::source::picture::{Look, Picture, recipe_file};
use crate::source::scene::Group;
use crate::source::{Given, Source};
use crate::text::Text;
use crate::{Error, LayerKind, Map, Raster, Scene};

/// One layer of a scene: a source, and how it joins the layers below it. A layer is an immutable value: every call
/// returns a new one, and a layer can be used in several scenes, or twice in one.
#[derive(Debug, Clone)]
pub struct Layer {
    pub(crate) name: Option<String>,
    pub(crate) base: Base,
    pub(crate) join: Join,
}

/// What a layer draws: a body of its own, or a reference to the body of another layer of the scene, which `Scene::add`
/// resolves into a body of its own for drawing.
#[derive(Debug, Clone)]
pub(crate) enum Base {
    Body(Body),
    Of(Reference),
}

/// What a layer draws before it meets the scene: its source, its looks and own-map masks (whatever the source), and where
/// that goes, stages 1 to 5 of "How a layer is drawn". A reference inherits it.
#[derive(Debug, Clone)]
pub(crate) struct Body {
    pub(crate) source: Arc<dyn Source>,
    pub(crate) finish: Finish,
    pub(crate) fit: Option<Fitting>,
    pub(crate) placement: Placement,
    /// The extra places of the references that lead to this body, in order, each after the one before: empty for a layer
    /// that is not a reference.
    pub(crate) extras: Vec<Placed>,
    /// How many references this body came through: 0 for a layer that is not a reference.
    pub(crate) chain: usize,
}

/// How a layer joins the scene, stages 6 to 9: its masks by layers drawn on the canvas, light, opacity and blend. A
/// reference owns its own.
#[derive(Debug, Clone)]
pub(crate) struct Join {
    pub(crate) opacity: f32,
    pub(crate) blend: Blend,
    /// Whether the layer's alpha is made from the brightness of its colour (stage 7).
    pub(crate) light: bool,
    /// Masks by layers drawn on the canvas (stage 6), in call order; those by a map are in the source.
    pub(crate) masks: Vec<CanvasMask>,
    /// How many `mask` calls this layer has had: the number of the next is one more.
    mask_calls: usize,
    /// What drawing the layer costs with its mask layers (a shared layer counted each time): masks only, until `Scene::add`
    /// resolves its references.
    pub(crate) budget: Budget,
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

impl Body {
    /// Whether what this body draws before it is placed is worth keeping: a look or a mask by a map ran on it, or its source
    /// takes work to draw. A picture that is only moved hands over its own pixels, and a fill is one pixel.
    pub(crate) fn keeps_drawn(&self) -> bool {
        self.finish.is_looked() || !self.finish.masks().is_empty() || self.source.is_costly()
    }

    /// This body with its own-map masks and no look.
    pub(crate) fn without_looks(&self) -> Self {
        Self {
            finish: self.finish.without_looks(),
            ..self.clone()
        }
    }

    /// Writes into `key` everything this body draws: its source, its looks and masks, and where it goes.
    ///
    /// # Errors
    /// What the source or a look cannot write.
    fn keyed(&self, key: &mut Keyed) -> Result<(), Error> {
        self.source.keyed(key)?;
        self.finish.keyed(key)?;
        key.debug(&(self.fit, &self.placement, &self.extras));
        Ok(())
    }

    /// This body with the mask `mask` by a map of its picture, after its looks and masks.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `mask` for a body with no picture, or with no map of that name.
    pub(crate) fn mask_by_map(&mut self, mask: MapMask) -> Result<(), Error> {
        let Some(names) = self.source.map_names() else {
            return Err(Error::invalid(
                "mask",
                format!(
                    "{:?} names a map of this layer's picture, and this layer has no picture",
                    mask.name
                ),
            ));
        };
        if !names.contains(&mask.name.as_str()) {
            return Err(Error::invalid(
                "mask",
                format!(
                    "{:?} is no map of this layer; its maps: {}",
                    mask.name,
                    if names.is_empty() {
                        "none".to_owned()
                    } else {
                        names.join(", ")
                    }
                ),
            ));
        }
        self.finish.add_mask(mask);
        Ok(())
    }
}

impl Layer {
    fn from_source(source: impl Source + 'static) -> Self {
        let budget = source.budget();
        let mut layer = Self::fresh(Base::Body(Body {
            source: Arc::new(source),
            finish: Finish::default(),
            fit: None,
            placement: Placement::default(),
            extras: Vec::new(),
            chain: 0,
        }));
        layer.join.budget = budget;
        layer
    }

    /// A layer that draws `base` and joins as a plain, opaque, normal layer.
    fn fresh(base: Base) -> Self {
        Self {
            name: None,
            base,
            join: Join {
                opacity: 1.0,
                blend: Blend::Normal,
                light: false,
                masks: Vec::new(),
                mask_calls: 0,
                budget: Budget::LAYER,
            },
        }
    }

    /// Writes into `key` everything the layer draws and how it joins, bar its name (which only addresses an error) and the
    /// numbers of its masks: its body, opacity, blend, light and mask layers, each by what it draws.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a reference no scene has resolved, and what a source or a look cannot write.
    pub(crate) fn keyed(&self, key: &mut Keyed) -> Result<(), Error> {
        self.drawable()?.keyed(key)?;
        let join = &self.join;
        key.debug(&(join.opacity, join.blend, join.light))
            .number(join.masks.len());
        for mask in &join.masks {
            mask.layer().keyed(key)?;
            key.debug(&(mask.uses, mask.feather, mask.invert));
        }
        Ok(())
    }

    /// What drawing this layer costs: itself, its mask layers (and, once `Scene::add` has resolved a reference, the
    /// references it leads through).
    pub(crate) fn budget(&self) -> Budget {
        self.join.budget
    }

    /// The body this layer is drawn from.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a reference no scene has resolved: a layer is drawn only from a scene, which resolves it.
    pub(crate) fn drawable(&self) -> Result<&Body, Error> {
        match &self.base {
            Base::Body(body) => Ok(body),
            Base::Of(reference) => Err(Error::invalid(
                "of",
                format!("the reference to {:?} was not resolved by a scene", reference.name),
            )),
        }
    }

    /// A reference to the layer called `name` of the scene this layer is added to: it draws that layer again. It inherits
    /// the layer's source, `fit`, looks, own-map masks and `place` (stages 1 to 5 of "How a layer is drawn") with that
    /// layer's own values, so it follows it; it does not inherit its layer masks, `as_light`, opacity or blend, which
    /// belong to the scene's composition: the reference has its own, as any layer has. It adds its own looks after the named
    /// layer's, its own-map masks, and an extra `place` applied after the named layer's, about its own `anchor` as
    /// fractions of the bounding rectangle of the box as the named layer places it. It has no source, no `fit` and no
    /// `perspective`: each of those calls is refused.
    ///
    /// `name` is looked up when the layer is added to a scene (`Scene::add`): the layer must be in the scene already or in
    /// the same call, and a chain of references that leads back to itself, also through masks, is refused there.
    #[must_use]
    pub fn of(name: &str) -> Self {
        Self::fresh(Base::Of(Reference::new(name)))
    }

    /// A picture: its box is its pixels, one design pixel each, at the canvas origin; what lies outside the canvas is cut.
    /// It is resampled once to the output size, by bilinear interpolation of premultiplied colour, or by an area average
    /// where it is reduced.
    #[must_use]
    pub fn picture(raster: Raster) -> Self {
        Self::from_source(Picture::held(raster))
    }

    /// A picture to be given later, a hole: `with_values` fills it (`<layer>.picture`) with a [`Raster`] or a path. A render
    /// refuses a layer whose picture is still a hole, naming it.
    #[must_use]
    pub fn picture_hole() -> Self {
        Self::from_source(Picture::hole())
    }

    /// A picture read from a file when the scene renders: as [`Layer::picture`] draws it, upright, in sRGB values, with its
    /// alpha if it has one. A file that cannot be read is an [`Error::Read`] at `render`, naming the layer.
    #[must_use]
    pub fn picture_file(path: impl Into<PathBuf>) -> Self {
        Self::from_source(Picture::file(Located::new(path)))
    }

    /// A picture of a saved scene: the file as the document names it.
    pub(crate) fn picture_saved(file: Located) -> Self {
        Self::from_source(Picture::file(file))
    }

    /// A vector drawing, given as its JSON document (what the vector package's `Drawing.to_json()` writes): its box is its
    /// frame at the canvas origin, one design pixel to a pixel of the frame, rasterised by vector at the output size; what
    /// lies outside the canvas is cut. The document is checked now.
    ///
    /// # Errors
    /// [`Error::Format`] (pointer under `/drawing`) for a document the linked vector crate cannot read, a version it does
    /// not read included; [`Error::Invalid`] naming `drawing` for a drawing without a frame.
    pub fn drawing(json: &str) -> Result<Self, Error> {
        Ok(Self::from_source(Drawing::parse(json)?))
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
        Ok(Self::from_source(Drawing::text(content, text)?))
    }

    /// A scene as a layer, a group: its canvas is the layer's box, at the canvas origin like a picture, and its layers are
    /// drawn on their own, each joined as it says over the group's own content, before the finished picture is joined as one
    /// layer (`fit`, `place`, `perspective`, masks, `blend`, `opacity` and `as_light` act on it as they act on any layer). The
    /// scene is drawn at the density the layer has, so it is as sharp at any size; a look runs on its finished picture.
    #[must_use]
    pub fn scene(inner: Scene) -> Self {
        Self::from_source(Group::new(inner))
    }

    /// A layer that `kind` draws (a plugin: [`LayerKind`]): its box is the canvas, like a fill's, and the scene asks it for the
    /// rectangle of the canvas that shows, at the scale it is drawn at. `fit`, `place`, looks, masks, `blend` and `opacity`
    /// act on what it answers as they act on any layer; what it drew is kept and asked for again only for a rectangle or a
    /// scale it has not drawn. A scene with such a layer has no JSON form (`to_json` refuses it, as it does an array): make
    /// it through [`LayerKinds::make`](crate::LayerKinds::make) for one that a saved scene can name.
    #[must_use]
    pub fn make(kind: Arc<dyn LayerKind>) -> Self {
        Self::from_source(Make::new(kind, false))
    }

    /// A layer of a kind a registry made: as [`Layer::make`], and a saved scene names it.
    pub(crate) fn made_by_name(kind: Arc<dyn LayerKind>) -> Self {
        Self::from_source(Make::new(kind, true))
    }

    /// A flat colour over the whole canvas: `#rrggbb` or `#rrggbbaa`, in sRGB values; the alpha is straight.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `fill` for any other text.
    pub fn fill(color: &str) -> Result<Self, Error> {
        Ok(Self::from_source(Fill::parse(color)?))
    }

    /// This layer with an image look run on it, after the looks it has: `json` is the recipe document that the image package's
    /// `Recipe.to_dict()` writes, checked now by the linked image crate. Any layer takes one: the look runs on what its source
    /// drew at the size the layer has (a picture resampled, a drawing, a text, a fill or a group's finished picture), on its
    /// colour with the alpha left as it is, and its `px` values are image's design pixels (1/480 of the canvas's long side); a look
    /// that sets `size` or `frame` is refused.
    ///
    /// # Errors
    /// [`Error::Format`] if `json` is not a JSON object or carries a recipe `format` the linked image crate does not read;
    /// [`Error::Invalid`] naming `look` for anything image refuses, in its words.
    pub fn look(self, json: &str) -> Result<Self, Error> {
        self.look_in(Px::Design, json)
    }

    /// [`Layer::look`] with its `px` in the unit `px`: [`Px::Canvas`] counts them in the scene's canvas pixels (inside a group,
    /// the inner scene's), so a value tuned on one canvas size means the same on another.
    ///
    /// # Errors
    /// As [`Layer::look`].
    pub fn look_in(self, px: Px, json: &str) -> Result<Self, Error> {
        let recipe = serde_json::from_str(json).map_err(|error| Error::Format {
            pointer: "/look".to_owned(),
            message: format!("not JSON: {error}"),
        })?;
        self.looked(&recipe, px)
    }

    /// This layer with the look of the recipe file `path` (TOML, as the image package writes it), read now.
    ///
    /// # Errors
    /// [`Error::Read`] if the file cannot be read; otherwise as [`Layer::look`].
    pub fn look_file(self, path: impl AsRef<Path>) -> Result<Self, Error> {
        self.look_file_in(Px::Design, path)
    }

    /// [`Layer::look_file`] with its `px` in the unit `px` (see [`Layer::look_in`]).
    ///
    /// # Errors
    /// As [`Layer::look_file`].
    pub fn look_file_in(self, px: Px, path: impl AsRef<Path>) -> Result<Self, Error> {
        self.looked(&recipe_file(path.as_ref())?, px)
    }

    /// This layer with the look `recipe`: run on what its source drew, or, for a reference, kept to be run on that of the layer
    /// it names after that layer's looks.
    fn looked(mut self, recipe: &serde_json::Value, px: Px) -> Result<Self, Error> {
        match &mut self.base {
            Base::Body(body) => body.finish.add_look(Look::new(recipe, px)?),
            Base::Of(reference) => reference.looks.push(Look::new(recipe, px)?),
        }
        Ok(self)
    }

    /// The body of this layer, for a call that a reference refuses: it has none of its own.
    fn own_body(&mut self, call: &str) -> Result<&mut Body, Error> {
        match &mut self.base {
            Base::Body(body) => Ok(body),
            Base::Of(reference) => Err(Error::invalid(
                call,
                format!(
                    "a reference has no {call} of its own: it inherits that of {:?}; place it with `place`",
                    reference.name
                ),
            )),
        }
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
        self.given(name, Given::File(Located::new(path)))
    }

    /// This picture layer with the map `name` in the file of a saved scene.
    pub(crate) fn map_saved(self, name: &str, file: Located) -> Result<Self, Error> {
        self.given(name, Given::File(file))
    }

    /// This picture layer with a hole under the map `name`: a map to be given later, which `with_values` fills
    /// (`<layer>.map.<name>`). A render refuses a hole, naming the layer and `map.<name>`.
    ///
    /// # Errors
    /// As [`Layer::map`].
    pub fn map_hole(self, name: &str) -> Result<Self, Error> {
        self.given(name, Given::Hole)
    }

    fn given(mut self, name: &str, given: Given) -> Result<Self, Error> {
        check_name("map", name)?;
        let body = self.own_body("map")?;
        body.source = body.source.with_map(name, given)?;
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
        self.own_body("fit")?.fit = Some(Fitting::new(area, how, align)?);
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
        match &mut self.base {
            Base::Body(body) => body.placement = body.placement.placed(place)?,
            Base::Of(reference) => reference.place = reference.place.with(place)?,
        }
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
        self.own_body("perspective")?.placement = Placement::Perspective(Quad::new(corners)?);
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
    /// has a name, a blend other than normal or is light, for masks nested more than [`MAX_DEPTH`] deep, and for a layer
    /// that with its mask layers would draw more than [`MAX_LAYERS`](crate::MAX_LAYERS) layers.
    pub fn mask(mut self, mask: impl Into<Mask>) -> Result<Self, Error> {
        let mask = mask.into();
        mask.checked()?;
        let number = self.join.mask_calls + 1;
        match &mask.by.0 {
            By::Map(name) => {
                if mask.uses == Use::Luminance {
                    return Err(Error::invalid(
                        "use",
                        "a map is its own values: read it as \"alpha\", not \"luminance\"",
                    ));
                }
                let map_mask = MapMask {
                    name: name.clone(),
                    number,
                    feather: mask.feather,
                    invert: mask.invert,
                };
                match &mut self.base {
                    Base::Body(body) => body.mask_by_map(map_mask)?,
                    // a reference reads the maps of the layer it names, which `Scene::add` finds
                    Base::Of(reference) => reference.masks.push(map_mask),
                }
            }
            By::Layer(by) => {
                if let Some(name) = &by.name {
                    return Err(Error::invalid(
                        "mask",
                        format!("a mask layer has no name (it is owned by the layer it masks), got {name:?}"),
                    ));
                }
                if by.join.blend != Blend::Normal {
                    return Err(Error::invalid(
                        "mask",
                        format!(
                            "a mask layer has no blend (a mask is not joined to what is below), got {}",
                            by.join.blend
                        ),
                    ));
                }
                if by.join.light {
                    return Err(Error::invalid(
                        "mask",
                        "a mask layer is not a light (its alpha is its own), got as_light(true)",
                    ));
                }
                let budget = self.join.budget.needing(by.budget());
                if budget.depth > MAX_DEPTH {
                    return Err(Error::invalid(
                        "mask",
                        format!(
                            "masks nest at most {MAX_DEPTH} deep, and this mask layer has masks {} deep",
                            by.join.budget.depth
                        ),
                    ));
                }
                self.join.budget = budget.checked("mask")?;
                self.join.masks.push(CanvasMask::new((**by).clone(), &mask, number));
            }
        }
        self.join.mask_calls = number;
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
            self.join.opacity = value as f32;
        }
        Ok(self)
    }

    /// This layer joined to what is below by `blend` (default [`Blend::Normal`]), in sRGB values, after its masks and its
    /// opacity: where the layer lands it is blended with the colour below by the W3C compositing equation.
    #[must_use]
    pub fn blend(mut self, blend: Blend) -> Self {
        self.join.blend = blend;
        self
    }

    /// This layer made light (`true`), or not (`false`, the default). A light layer's premultiplied colour, after its looks
    /// and masks, is its colour over black, and its alpha is the largest of the red, green and blue of that colour, so what
    /// is black adds nothing, a `screen` of the layer over black gives that colour exactly, and white is opaque white. A
    /// layer that is not light keeps the alpha of its source.
    #[must_use]
    pub fn as_light(mut self, on: bool) -> Self {
        self.join.light = on;
        self
    }
}
