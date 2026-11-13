//! A layer: a source and how it joins what is below it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::Error;
use crate::Raster;
use crate::source::Source;
use crate::source::drawing::Drawing;
use crate::source::fill::Fill;
use crate::source::picture::{Picture, recipe_file};

/// One layer of a scene: a source, and how it joins the layers below it. A layer is an immutable value: every call
/// returns a new one, and a layer can be used in several scenes, or twice in one.
#[derive(Debug, Clone)]
pub struct Layer {
    pub(crate) name: Option<String>,
    pub(crate) opacity: f32,
    pub(crate) source: Arc<dyn Source>,
}

impl Layer {
    fn of(source: impl Source + 'static) -> Self {
        Self {
            name: None,
            opacity: 1.0,
            source: Arc::new(source),
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
        let mut characters = name.chars();
        let starts = characters.next().is_some_and(|first| first.is_ascii_alphabetic());
        if !(starts && characters.all(|next| next.is_ascii_alphanumeric() || next == '_')) {
            return Err(Error::invalid(
                "name",
                format!("a name is letters, digits and `_`, starting with a letter, got {name:?}"),
            ));
        }
        self.name = Some(name.to_owned());
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
}
