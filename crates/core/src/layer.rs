//! A layer: a source and how it joins what is below it.

use std::path::PathBuf;
use std::sync::Arc;

use crate::Error;
use crate::Raster;
use crate::source::Source;
use crate::source::fill::Fill;
use crate::source::picture::Picture;

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
    /// It is resampled once to the output size, by bilinear interpolation of premultiplied colour.
    #[must_use]
    pub fn picture(raster: Raster) -> Self {
        Self::of(Picture::Held(raster))
    }

    /// A picture read from a file when the scene renders: as [`Layer::picture`] draws it, upright, in sRGB values, with its
    /// alpha if it has one. A file that cannot be read is an [`Error::Read`] at `render`, naming the layer.
    #[must_use]
    pub fn picture_file(path: impl Into<PathBuf>) -> Self {
        Self::of(Picture::File(path.into()))
    }

    /// A flat colour over the whole canvas: `#rrggbb` or `#rrggbbaa`, in sRGB values; the alpha is straight.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `fill` for any other text.
    pub fn fill(color: &str) -> Result<Self, Error> {
        Ok(Self::of(Fill::parse(color)?))
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
