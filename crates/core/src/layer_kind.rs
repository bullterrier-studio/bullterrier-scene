//! A kind of layer the caller supplies: the plugin seam. The scene asks it one question, "draw this rectangle of the canvas at
//! this scale", and joins, places, masks and caches what it answers as it does for any layer.

use serde_json::Value;

use crate::{Error, Raster};

/// A kind of layer that something outside the scene draws (a generator, a simulation, a decoder): give one to
/// [`Layer::make`](crate::Layer::make).
///
/// **Identity.** What a kind draws depends on its [`name`](LayerKind::name), its [`options`](LayerKind::options), the rectangle and the
/// scale, and on nothing else: the same four give the same raster, whatever else has happened (the clock, a counter, a file
/// that changed). That is the promise that lets the scene keep what a kind drew and ask it again only for a rectangle or a
/// scale it has not seen, so a kind with a hidden input must put it in its options. Two kinds with one name and equal options
/// are the same layer: `==` on scenes says so, and so does the cache.
pub trait LayerKind: Send + Sync {
    /// What this kind is called: letters, digits and `_` by convention. Part of a layer's identity.
    fn name(&self) -> &str;

    /// The options of this layer as plain data (an object by convention): everything besides the name that decides what
    /// [`draw`](LayerKind::draw) answers. Part of a layer's identity, and what a saved scene will write.
    fn options(&self) -> Value;

    /// The rectangle `area` = `[x, y, width, height]` of the canvas, in design pixels, drawn at `scale` = `[across, down]`
    /// output pixels per design pixel (the two differ by less than one part in the canvas's long side on its short axis): a
    /// raster of `ceil(width * across)` columns by `ceil(height * down)` rows, straight alpha, values from 0 to 1, whose
    /// pixels divide the rectangle evenly. A product within 1e-12 (relative) of a whole number counts as that number.
    ///
    /// # Errors
    /// What the kind cannot draw; [`Raster::rgba`] refuses a value outside 0 to 1. A raster of another size than the one
    /// asked for is refused by the scene, naming both sizes.
    fn draw(&self, area: [f64; 4], scale: [f64; 2]) -> Result<Raster, Error>;
}
