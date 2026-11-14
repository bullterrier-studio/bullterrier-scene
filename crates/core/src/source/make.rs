//! A layer of a kind the caller supplies (`LayerKind`): drawn at the rectangle and scale the scene asks, like a fill that has detail.

use std::fmt;
use std::sync::Arc;

use serde_json::{Value, json};

use super::{Drawn, Opened, Source};
use crate::document::Form;
use crate::geometry::Rect;
use crate::{Error, LayerKind};

/// A kind as the source of a layer. Its box is the canvas, as a fill's is.
pub(crate) struct Make {
    kind: Arc<dyn LayerKind>,
}

impl Make {
    pub(crate) fn new(kind: Arc<dyn LayerKind>) -> Self {
        Self { kind }
    }
}

impl fmt::Debug for Make {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Make").field("kind", &self.kind.name()).finish()
    }
}

impl Source for Make {
    /// A kind has no JSON form yet: it is refused when saved, and written by name and options when compared.
    fn document(&self, form: Form) -> Result<Vec<(&'static str, Value)>, Error> {
        match form {
            Form::Save => Err(Error::invalid(
                "make",
                format!(
                    "a layer of kind {:?} has no JSON form: kinds are not registered yet, so it cannot be named in a saved scene",
                    self.kind.name()
                ),
            )),
            Form::Compare => Ok(vec![(
                "make",
                json!({"kind": self.kind.name(), "options": self.kind.options()}),
            )]),
        }
    }

    /// The part of the canvas the layer shows, at the asked scale.
    fn open(&self, canvas: Rect) -> Result<Opened<'_>, Error> {
        Ok(Opened::new(canvas, move |request| {
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
                maps: std::collections::BTreeMap::new(),
            })
        }))
    }
}
