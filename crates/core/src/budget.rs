//! How much drawing a scene may ask: the one rule of [`MAX_DEPTH`] and [`MAX_LAYERS`], stated once. A mask layer, a reference
//! and (batch C4c) a group each ask for more layers to be drawn, and the same count and depth serve all of them.

use crate::Error;

/// The deepest drawing leans on other layers: a layer's mask layer, that layer's own mask layers, and the references each
/// leads through (a reference to a reference to a layer is two deep). A layer that is drawn on its own has depth 0. Scenes
/// used as layers (batch C4c) count in the same depth.
pub const MAX_DEPTH: usize = 16;

/// The most layers a scene draws in one render: its layers, the mask layers they are masked by, and the references among
/// them, a layer used twice counted twice.
pub const MAX_LAYERS: usize = 1_000;

/// What drawing something costs: how deep it leans on other layers and how many layers it draws in all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Budget {
    pub(crate) depth: usize,
    pub(crate) layers: usize,
}

impl Budget {
    /// Nothing drawn: a scene with no layers.
    pub(crate) const NONE: Self = Self { depth: 0, layers: 0 };

    /// One layer, drawn on its own.
    pub(crate) const LAYER: Self = Self { depth: 0, layers: 1 };

    /// This and `other` drawn in the same render: as deep as the deeper, and their layers together.
    pub(crate) fn beside(self, other: Self) -> Self {
        Self {
            depth: self.depth.max(other.depth),
            layers: self.layers + other.layers,
        }
    }

    /// This, drawn through `inner`, which it needs drawn first (a mask layer for the layer it masks): one deeper than `inner`
    /// and its layers added.
    pub(crate) fn needing(self, inner: Self) -> Self {
        Self {
            depth: self.depth.max(inner.depth + 1),
            layers: self.layers + inner.layers,
        }
    }

    /// This, drawn through a chain of `links` references: at least that deep.
    pub(crate) fn through(self, links: usize) -> Self {
        Self {
            depth: self.depth.max(links),
            ..self
        }
    }

    /// This budget if it is within [`MAX_DEPTH`] and [`MAX_LAYERS`].
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `field`, with the bound and the count.
    pub(crate) fn checked(self, field: &str) -> Result<Self, Error> {
        if self.depth > MAX_DEPTH {
            return Err(Error::invalid(
                field,
                format!(
                    "drawing leans on other layers at most {MAX_DEPTH} deep (mask layers, and the references each leads through), got {}",
                    self.depth
                ),
            ));
        }
        if self.layers > MAX_LAYERS {
            return Err(Error::invalid(
                field,
                format!(
                    "at most {MAX_LAYERS} layers are drawn (the layers, their mask layers and references, a layer used twice counted twice), got {}",
                    self.layers
                ),
            ));
        }
        Ok(self)
    }
}
