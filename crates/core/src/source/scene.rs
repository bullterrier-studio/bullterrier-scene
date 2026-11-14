//! A scene used as a layer, a group: its canvas is the layer's box, and it is drawn on its own, then joined as one picture.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use super::{Drawn, Opened, Source};
use crate::budget::Budget;
use crate::cache::{Cache, Keyed};
use crate::document::{Form, scene_document};
use crate::geometry::{MAX_PIXELS, Rect};
use crate::landing::Target;
use crate::parameters::{Parameter, Value, Values};
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
    fn document(&self, form: Form) -> Result<Vec<(&'static str, serde_json::Value)>, Error> {
        Ok(vec![("scene", scene_document(&self.scene, form)?)])
    }

    /// The parameters of the scene, each under `scene.`.
    fn parameters(&self) -> Vec<Parameter> {
        self.scene
            .parameters()
            .into_iter()
            .map(|parameter| parameter.under("scene"))
            .collect()
    }

    /// The scene with the values set (see [`Scene::with_values`]): a new scene, the old one unchanged.
    fn with_values(&self, values: &[(&str, &Value)]) -> Result<Arc<dyn Source>, Error> {
        let inner = values.iter().fold(Values::new(), |inner, &(address, value)| {
            inner.set(address.strip_prefix("scene.").unwrap_or(address), value.clone())
        });
        let scene = self
            .scene
            .with_values(&inner)
            .map_err(|error| error.in_layer("scene"))?;
        Ok(Arc::new(Self::new(scene)))
    }

    /// What the group draws is its scene as a value: the canvas and every layer it draws, each by what it is made of.
    fn keyed(&self, key: &mut Keyed) -> Result<(), Error> {
        key.text("group")
            .debug(&(self.scene.width(), self.scene.height()))
            .number(self.scene.drawn.len());
        for layer in &self.scene.drawn {
            layer.keyed(key)?;
        }
        Ok(())
    }

    /// A group is drawn at the phase of its place, as a drawing is.
    fn follows_grid(&self) -> bool {
        true
    }

    /// A group takes the drawing of its layers.
    fn is_costly(&self) -> bool {
        true
    }

    /// This layer, and the layers its scene draws (each time the scene is used), one level deeper than the scene leans.
    fn budget(&self) -> Budget {
        Budget::LAYER.needing(self.scene.budget)
    }

    /// The whole box at the asked scale: the scene is drawn on a transparent backdrop of its own, its canvas starting
    /// `Request::phase` into the grid the outer layer will have, so that a group put at a fraction of a pixel is not resampled.
    /// A layer that ends at the box ends there by its own coverage (as in a scene drawn whole), and the outer warp does not cut
    /// the box again; a layer that overflows the box shows in the pixel its edge falls in, less than a pixel past it.
    fn open(&self, _canvas: Rect, _cache: &Cache) -> Result<Opened<'_>, Error> {
        let own = Rect {
            x: 0.0,
            y: 0.0,
            width: f64::from(self.scene.width()),
            height: f64::from(self.scene.height()),
        };
        let opened = Opened::new(own, move |request, cache| {
            let (shape, (first, end)) = request.drawn_rows(own)?;
            let (columns, rows) = shape.size;
            // the pictures of the groups drawn inside one another are alive together: their sum is the bound, checked before
            // this one is allocated
            let held = request.held + columns * (end - first);
            if held > MAX_PIXELS {
                return Err(Error::invalid(
                    "size",
                    format!(
                        "the scenes drawn inside one another hold {held} pixels at once ({columns} x {} here, {} outside); at most {MAX_PIXELS} pixels (2^27): lower `size` or make the scenes smaller",
                        end - first,
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
                window: (first, end - first),
                offset: request.phase,
                held,
                cache,
            };
            let joined = draw(&self.scene.drawn, &target).map_err(|error| error.in_layer("scene"))?;
            Ok(Drawn {
                raster: Arc::new(Raster::made(columns, end - first, straight(joined))),
                covers: shape.covers,
                ends_at_window: shape.ends_at_window,
                room: [0; 4],
                maps: BTreeMap::new(),
            })
        });
        Ok(opened.drawing_whole().by_window())
    }
}
