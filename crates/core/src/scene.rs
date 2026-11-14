//! The canvas and its stack of layers.

use std::collections::HashMap;
use std::sync::Arc;

use crate::budget::Budget;
use crate::cache::Cache;
use crate::{Error, Layer, Raster, reference, render};

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

    /// A scene of the same canvas with no layer, sharing this scene's cache: the start of a scene made from this one.
    pub(crate) fn emptied(&self) -> Self {
        Self {
            layers: Vec::new(),
            drawn: Vec::new(),
            budget: Budget::NONE,
            ..self.clone()
        }
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
}
