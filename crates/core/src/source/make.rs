//! A layer of a kind the caller supplies (`LayerKind`): drawn at the rectangle and scale the scene asks, like a fill that has detail.

use std::fmt;
use std::sync::Arc;

use serde_json::{Value, json};

use super::{Drawn, Opened, Source};
use crate::cache::{Cache, Keyed};
use crate::document::Form;
use crate::geometry::Rect;
use crate::{Error, LayerKind};

/// A kind as the source of a layer. Its box is the canvas, as a fill's is.
pub(crate) struct Make {
    kind: Arc<dyn LayerKind>,
    /// Whether the kind came from a registry, so that a saved scene can name it and read it back.
    registered: bool,
}

impl Make {
    pub(crate) fn new(kind: Arc<dyn LayerKind>, registered: bool) -> Self {
        Self { kind, registered }
    }
}

impl fmt::Debug for Make {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Make")
            .field("kind", &self.kind.name())
            .field("registered", &self.registered)
            .finish()
    }
}

impl Source for Make {
    /// A kind is written by name and options. A saved scene holds only one made through a registry (it can be read back
    /// by name); one given to `Layer::make` is refused when saved.
    fn document(&self, form: Form) -> Result<Vec<(&'static str, Value)>, Error> {
        if form == Form::Save && !self.registered {
            return Err(Error::invalid(
                "make",
                format!(
                    "a layer of kind {:?} was not made through a registry (`LayerKinds::make`), so a saved scene cannot name it",
                    self.kind.name()
                ),
            ));
        }
        Ok(vec![(
            "make",
            json!({"kind": self.kind.name(), "options": self.kind.options()}),
        )])
    }

    /// What a kind draws depends on its name and options (and the request): the promise of [`LayerKind`].
    fn keyed(&self, key: &mut Keyed) -> Result<(), Error> {
        key.text("make")
            .text(self.kind.name())
            .text(&self.kind.options().to_string());
        Ok(())
    }

    /// What the kind draws is kept: asking again is its own cost.
    fn is_costly(&self) -> bool {
        true
    }

    /// The part of the canvas the layer shows, at the asked scale.
    fn open(&self, canvas: Rect, _cache: &Cache) -> Result<Opened<'_>, Error> {
        Ok(Opened::new(canvas, move |request, _| {
            let window = request.window;
            let (columns, rows) = request.scale.size(window)?;
            let raster = self.kind.draw(
                [window.x, window.y, window.width, window.height],
                [request.scale.x.pixels(1.0), request.scale.y.pixels(1.0)],
            )?;
            if (raster.width(), raster.height()) != (columns, rows) {
                return Err(Error::invalid(
                    "make",
                    format!(
                        "the kind {:?} drew {} x {} pixels for a rectangle that is {columns} x {rows} pixels at this scale",
                        self.kind.name(),
                        raster.width(),
                        raster.height()
                    ),
                ));
            }
            Ok(Drawn {
                raster: Arc::new(raster),
                covers: window,
                ends_at_window: false,
                room: [0; 4],
                maps: std::collections::BTreeMap::new(),
            })
        }))
    }
}
