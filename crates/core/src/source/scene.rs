//! A scene used as a layer, a group: its canvas is the layer's box, and it is drawn on its own, then joined as one picture.

use std::fmt;
use std::sync::Arc;

use super::{Drawn, Opened, Source};
use crate::budget::Budget;
use crate::geometry::{MAX_PIXELS, Rect};
use crate::landing::Target;
use crate::render::{draw, straight};
use crate::{Error, Raster, Scene};

/// A scene as the source of a layer. The scene is held, not copied, so a scene used as a layer many times is held once.
pub(crate) struct Group {
    scene: Arc<Scene>,
}

impl Group {
    pub(crate) fn new(scene: Scene) -> Self {
        Self { scene: Arc::new(scene) }
    }
}

// not derived: a derived `Debug` would print the nest of scenes inside, as deep as they go
impl fmt::Debug for Group {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let budget = self.scene.budget;
        formatter
            .debug_struct("Group")
            .field("canvas", &(self.scene.width(), self.scene.height()))
            .field("layers", &budget.layers)
            .field("depth", &budget.depth)
            .finish()
    }
}

impl Source for Group {
    /// This layer, and the layers its scene draws (each time the scene is used), one level deeper than the scene leans.
    fn budget(&self) -> Budget {
        Budget::LAYER.needing(self.scene.budget)
    }

    /// The whole box at the asked scale: the scene is drawn on a transparent backdrop of its own, its canvas starting
    /// `Request::phase` into the grid the outer layer will have, so that a group put at a fraction of a pixel is not resampled.
    /// A layer that ends at the box ends there by its own coverage (as in a scene drawn whole), and the outer warp does not cut
    /// the box again; a layer that overflows the box shows in the pixel its edge falls in, less than a pixel past it.
    fn open(&self, _canvas: Rect) -> Result<Opened<'_>, Error> {
        let own = Rect {
            x: 0.0,
            y: 0.0,
            width: f64::from(self.scene.width()),
            height: f64::from(self.scene.height()),
        };
        let opened = Opened::new(own, move |request| {
            let covers = request.whole(own);
            let (columns, rows) = request.scale.size(covers)?;
            // the pictures of the groups drawn inside one another are alive together: their sum is the bound, checked before
            // this one is allocated
            let held = request.held + columns * rows;
            if held > MAX_PIXELS {
                return Err(Error::invalid(
                    "size",
                    format!(
                        "the scenes drawn inside one another hold {held} pixels at once ({columns} x {rows} here, {} outside); at most {MAX_PIXELS} pixels (2^27): lower `size` or make the scenes smaller",
                        request.held
                    ),
                ));
            }
            let target = Target {
                canvas: own,
                scale: request.scale,
                columns,
                rows,
                margin: (0, 0),
                offset: request.phase,
                held,
            };
            let joined = draw(&self.scene.drawn, &target).map_err(|error| error.in_layer("scene"))?;
            Ok(Drawn {
                raster: Arc::new(Raster::made(columns, rows, straight(joined))),
                covers,
                ends_at_window: request.window == own,
            })
        });
        Ok(opened.drawing_whole())
    }
}
