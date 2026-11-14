//! What a layer does to the raster its source drew, before it is placed: its looks (stage 3 of "How a layer is drawn"), then
//! its masks by its own maps (stage 4). The layer owns them whatever its source; the source only draws, at the size the
//! looks run at, and hands over the maps they use.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::label;
use crate::feather;
use crate::geometry::whole_up;
use crate::source::picture::Look;
use crate::source::{Drawn, Request};
use crate::{Error, Raster};

/// A mask by one of a picture's maps, as the layer's `mask` call gave it.
#[derive(Debug, Clone)]
pub(crate) struct MapMask {
    /// The map's name.
    pub(crate) name: String,
    /// The mask's place among the layer's `mask` calls, from 1: `mask`, `mask#2`...
    pub(crate) number: usize,
    /// The blur of the map's edge, in design pixels as the layer is fitted; 0 is none.
    pub(crate) feather: f64,
    /// Whether the mask is 1 minus the map (after the blur).
    pub(crate) invert: bool,
}

/// The looks of a layer in call order and its masks by its own maps.
#[derive(Debug, Clone, Default)]
pub(crate) struct Finish {
    looks: Vec<Look>,
    masks: Vec<MapMask>,
    /// The layer is light (stage 7): its looks see its colour over black, and what they make of the black is light.
    light: bool,
}

impl Finish {
    /// This finish for a light layer (`true`) or not.
    pub(crate) fn lit(mut self, light: bool) -> Self {
        self.light = light;
        self
    }

    /// The masks by a map, in call order.
    pub(crate) fn masks(&self) -> &[MapMask] {
        &self.masks
    }

    /// The looks, in call order.
    pub(crate) fn looks(&self) -> &[Look] {
        &self.looks
    }

    /// The look at `index` with each of `set` (an address of the look, a number) set, checked by image together.
    ///
    /// # Errors
    /// What image refuses of the set.
    pub(crate) fn change_look(&mut self, index: usize, set: &[(&str, f64)]) -> Result<(), Error> {
        self.looks[index] = self.looks[index].with_values(set)?;
        Ok(())
    }

    /// This finish with `look` run after the looks it has.
    pub(crate) fn add_look(&mut self, look: Look) {
        self.looks.push(look);
    }

    /// This finish with `mask` multiplied in after the looks and the masks it has.
    pub(crate) fn add_mask(&mut self, mask: MapMask) {
        self.masks.push(mask);
    }

    /// Whether a look runs: the source then draws at the size the look runs at and hands over the maps in `uses_map`. With no
    /// look, a source that draws its own pixels (a picture) multiplies its masks into them itself, through
    /// [`Finish::masking`], so that the one warp resamples the picture and its map once.
    pub(crate) fn is_looked(&self) -> bool {
        !self.looks.is_empty()
    }

    /// Whether a look or a mask reads the map `name`.
    pub(crate) fn uses_map(&self, name: &str) -> bool {
        self.masks.iter().any(|mask| mask.name == name)
            || self.looks.iter().any(|look| look.known_maps().contains(&name))
    }

    /// `raster` with its alpha multiplied by each mask: the map's `values` (of `raster`'s size), blurred by the mask's
    /// `feather` design pixels of the fitted layer, `density` raster pixels each along x and y, then inverted if it says so.
    ///
    /// # Errors
    /// A blur that does not fit the raster.
    pub(crate) fn masking(
        &self,
        raster: &Arc<Raster>,
        density: (f64, f64),
        values: &BTreeMap<&str, &[f32]>,
    ) -> Result<Arc<Raster>, Error> {
        if self.masks.is_empty() {
            return Ok(Arc::clone(raster));
        }
        let size = (raster.width(), raster.height());
        let mut pixels = raster.pixels().to_vec();
        for mask in &self.masks {
            let sigma = (mask.feather * density.0, mask.feather * density.1);
            let reach = (feather::reach(sigma.0), feather::reach(sigma.1));
            let field = format!("{}.feather", label(mask.number));
            feather::fits_line(&field, size.0, reach.0)?;
            feather::fits_line(&field, size.1, reach.1)?;
            let mut factor = values[mask.name.as_str()].to_vec();
            feather::blur(&mut factor, size, sigma);
            if mask.invert {
                for value in &mut factor {
                    *value = 1.0 - *value;
                }
            }
            for (pixel, value) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(&factor) {
                pixel[3] *= value;
            }
        }
        Ok(Arc::new(Raster::made(size.0, size.1, pixels)))
    }

    /// `drawn` with each look run on it, in order, then each mask by a map multiplied into its alpha. A layer with no look
    /// is as drawn: its source did the masks on its own pixels. The look sees the raster at the size the layer draws it, keeps
    /// its alpha, and ends where the raster ends.
    ///
    /// # Errors
    /// What image refuses of a look at this size or of a map it misses, and a blur that does not fit.
    pub(crate) fn run(&self, drawn: Drawn, request: &Request) -> Result<Drawn, Error> {
        if !self.is_looked() {
            return Ok(drawn);
        }
        let Drawn {
            mut raster,
            covers,
            ends_at_window,
            maps,
        } = drawn;
        // drawn pixels per design pixel of the fitted layer: what the layer is drawn at, less the fit
        let density = (
            request.scale.x.pixels(1.0) / request.factor.0,
            request.scale.y.pixels(1.0) / request.factor.1,
        );
        // the scene's canvas as this layer's `place` scale draws it: what a look's `px` count in, whatever the layer's size
        // (a side beyond u32 is as large as image can be told)
        let side = |length: f64| u32::try_from(whole_up(length)).unwrap_or(u32::MAX);
        let canvas = (side(request.canvas.0 * density.0), side(request.canvas.1 * density.1));
        for look in &self.looks {
            raster = Arc::new(look.apply(&raster, &maps, canvas, self.light)?);
        }
        let values = maps
            .iter()
            .map(|(name, values)| (name.as_str(), values.as_slice()))
            .collect();
        Ok(Drawn {
            raster: self.masking(&raster, density, &values)?,
            covers,
            ends_at_window,
            maps: BTreeMap::new(),
        })
    }
}
