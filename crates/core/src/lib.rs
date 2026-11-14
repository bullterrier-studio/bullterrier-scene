//! The core of bullterrier-scene: its logic, in pure Rust. No Python, no numpy, safe code: the `bullterrier-scene-python`
//! crate binds it to the Python package.
//!
//! A [`Scene`] is a canvas and a stack of [`Layer`]s; [`Scene::render`] draws the whole picture at any size and gives a
//! [`Raster`]. Everything a render does to answer (sizes, resampling, premultiplication, the sibling crates) stays behind
//! that call.

#![forbid(unsafe_code)]

mod error;
mod feather;
mod geometry;
mod landing;
mod layer;
mod map;
mod mask;
mod perspective;
mod placement;
mod ranges;
mod raster;
mod render;
mod scene;
mod source;
mod text;
mod warp;

pub use error::Error;
pub use layer::{Layer, MAX_DEPTH};
pub use map::Map;
pub use mask::{Mask, MaskBy, Use};
pub use placement::{Deg, Fit, Place};
pub use raster::Raster;
pub use scene::{MAX_LAYERS, Scene};
pub use text::Text;
