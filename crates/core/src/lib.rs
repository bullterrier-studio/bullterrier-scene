//! The core of bullterrier-scene: its logic, in pure Rust. No Python, no numpy, safe code: the `bullterrier-scene-python`
//! crate binds it to the Python package.
//!
//! A [`Scene`] is a canvas and a stack of [`Layer`]s; [`Scene::render`] draws the whole picture at any size and gives a
//! [`Raster`]. Everything a render does to answer (sizes, resampling, premultiplication, the sibling crates) stays behind
//! that call.

#![forbid(unsafe_code)]

mod blend;
mod budget;
mod cache;
mod document;
mod error;
mod feather;
mod files;
mod finish;
mod geometry;
mod landing;
mod layer;
mod layer_kind;
mod layer_kinds;
mod map;
mod mask;
mod parameters;
mod perspective;
mod placement;
mod ranges;
mod raster;
mod reference;
mod render;
mod scene;
mod source;
mod suggest;
mod text;
mod warp;

pub use blend::Blend;
pub use budget::{MAX_DEPTH, MAX_LAYERS};
pub use error::Error;
pub use geometry::MAX_PIXELS;
pub use layer::Layer;
pub use layer_kind::LayerKind;
pub use layer_kinds::{LayerKindFactory, LayerKinds};
pub use map::Map;
pub use mask::{Mask, MaskBy, Use};
pub use parameters::{Kind, Parameter, Value, Values};
pub use placement::{Deg, Fit, Place};
pub use raster::Raster;
pub use scene::Scene;
pub use text::Text;
