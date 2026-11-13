//! The canvas and its stack of layers.

use std::collections::HashMap;

use crate::{Error, Layer, Raster, render};

/// The most layers a scene holds.
pub const MAX_LAYERS: usize = 1_000;

/// The longest a canvas side is, in design pixels.
const MAX_CANVAS: u32 = 100_000;

/// A canvas and a stack of layers, bottom first, drawn into one picture at any size. A scene is an immutable value: every
/// call returns a new one, and the old one can be used again.
#[derive(Debug, Clone)]
pub struct Scene {
    width: u32,
    height: u32,
    layers: Vec<Layer>,
}

impl Scene {
    /// An empty canvas of `width` by `height` design pixels: whole numbers, 1 to 100,000. A design pixel is what a length
    /// in the scene means.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `canvas` for a side outside 1 to 100,000.
    pub fn new(width: u32, height: u32) -> Result<Self, Error> {
        if !(1..=MAX_CANVAS).contains(&width) || !(1..=MAX_CANVAS).contains(&height) {
            return Err(Error::invalid(
                "canvas",
                format!("a canvas side is a whole number from 1 to {MAX_CANVAS}, got {width} x {height}"),
            ));
        }
        Ok(Self {
            width,
            height,
            layers: Vec::new(),
        })
    }

    /// This scene with `layers` on top of its stack, the first lowest.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `layers` for more than [`MAX_LAYERS`] layers in all, and naming `name` for a name used
    /// twice (the message names both layers) or a `layer<n>` name at any place but the `n`th.
    pub fn add(&self, layers: impl IntoIterator<Item = Layer>) -> Result<Self, Error> {
        let mut stack = self.layers.clone();
        stack.extend(layers);
        if stack.len() > MAX_LAYERS {
            return Err(Error::invalid(
                "layers",
                format!("a scene holds at most {MAX_LAYERS} layers, got {}", stack.len()),
            ));
        }
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
            if let Some(first) = places.insert(name.clone(), place) {
                return Err(Error::invalid(
                    "name",
                    format!("{name} is the name of layer {first} and of layer {place}: a name is used once in a scene"),
                ));
            }
        }
        Ok(Self {
            width: self.width,
            height: self.height,
            layers: stack,
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
    pub fn render(&self, size: u32) -> Result<Raster, Error> {
        render::render(self.width, self.height, &self.layers, size)
    }
}
