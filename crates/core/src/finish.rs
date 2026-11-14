//! What a layer does to the raster its source drew, before it is placed: its looks (stage 3 of "How a layer is drawn"), then
//! its masks by its own maps (stage 4). The layer owns them whatever its source; the source only draws, at the size the
//! looks run at, and hands over the maps they use.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::cache::Keyed;
use crate::error::label;
use crate::feather;
use crate::geometry::{MAX_LOOKED_PIXELS, Rect, whole_up};
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
}

impl Finish {
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

    /// This finish with no look: the masks by a map only.
    pub(crate) fn without_looks(&self) -> Self {
        Self {
            looks: Vec::new(),
            masks: self.masks.clone(),
        }
    }

    /// This finish with `look` run after the looks it has.
    pub(crate) fn add_look(&mut self, look: Look) {
        self.looks.push(look);
    }

    /// This finish with `mask` multiplied in after the looks and the masks it has.
    pub(crate) fn add_mask(&mut self, mask: MapMask) {
        self.masks.push(mask);
    }

    /// Writes into `key` the looks in order and the masks by a map: what this finish does to what a source drew.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a look image cannot write as data.
    pub(crate) fn keyed(&self, key: &mut Keyed) -> Result<(), Error> {
        key.number(self.looks.len());
        for look in &self.looks {
            look.keyed(key)?;
        }
        key.number(self.masks.len());
        for mask in &self.masks {
            key.text(&mask.name).debug(&(mask.feather, mask.invert));
        }
        Ok(())
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
    /// is as drawn: its source did the masks on its own pixels. The look sees the raster at the size the layer draws it, plus
    /// and keeps its alpha. A `light` layer (stage 7) is looked at over black, and what the look makes of the black is light:
    /// the raster is grown by the room its looks spread into (`Look::room`) before the look and kept grown, so the halo lands
    /// past the box. Any other layer is looked at its own size.
    ///
    /// # Errors
    /// What image refuses of a look at this size or of a map it misses, and a blur that does not fit.
    pub(crate) fn run(&self, drawn: Drawn, request: &Request, light: bool) -> Result<Drawn, Error> {
        if !self.is_looked() {
            return Ok(drawn);
        }
        let Drawn {
            raster,
            mut covers,
            mut ends_at_window,
            mut maps,
            ..
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
        let drawn_size = (raster.width(), raster.height());
        // only light shows the room: a layer that is not light keeps its source's alpha, so the halo would be cut, and a look
        // that blurs its edge would mix black in where image's own edge rule would have kept the picture's colour
        let room = if light {
            Look::room(&self.looks, canvas)?
                .filter(|room| fits(drawn_size, *room))
                .unwrap_or_default()
        } else {
            [0; 4]
        };
        let grown = room != [0; 4];
        let (mut looked, look_maps) = if grown {
            let replicas = maps
                .iter()
                .map(|(name, values)| (name.clone(), replicated(values, drawn_size, room)))
                .collect();
            (Arc::new(padded(&raster, room)), replicas)
        } else {
            (raster, std::mem::take(&mut maps))
        };
        for look in &self.looks {
            looked = Arc::new(look.apply(&looked, &look_maps, canvas, light)?);
        }
        let mut shown = [0; 4];
        if grown {
            // the raster now reaches past the window by the room, so the warp lets all of it through
            ends_at_window |= covers == request.window;
            covers = grow(covers, drawn_size, room);
            shown = room;
        }
        let values = look_maps
            .iter()
            .map(|(name, values)| (name.as_str(), values.as_slice()))
            .collect();
        Ok(Drawn {
            raster: self.masking(&looked, density, &values)?,
            covers,
            ends_at_window,
            room: shown,
            maps: BTreeMap::new(),
        })
    }
}

/// Whether a raster of `size` (columns, rows) with `room` (top, right, bottom, left) added is one a look can run on: image's
/// limit. A layer that would pass it gets no room, so that room never makes a render fail that would have run.
fn fits((columns, rows): (usize, usize), [top, right, bottom, left]: [usize; 4]) -> bool {
    let wide = columns.saturating_add(left).saturating_add(right);
    wide.saturating_mul(rows.saturating_add(top).saturating_add(bottom)) <= MAX_LOOKED_PIXELS
}

/// `covers`, a part of the box drawn as `size` pixels, grown by `room` pixels of that density on each side.
fn grow(covers: Rect, (columns, rows): (usize, usize), [top, right, bottom, left]: [usize; 4]) -> Rect {
    // a side of at most 2^26 pixels: exact in a float
    #[expect(clippy::cast_precision_loss, reason = "a raster side of at most 2^26 pixels")]
    let (across, down, top, right, bottom, left) = (
        columns as f64 / covers.width,
        rows as f64 / covers.height,
        top as f64,
        right as f64,
        bottom as f64,
        left as f64,
    );
    Rect {
        x: covers.x - left / across,
        y: covers.y - top / down,
        width: covers.width + (left + right) / across,
        height: covers.height + (top + bottom) / down,
    }
}

/// `raster` with `room` pixels of nothing (transparent black) round it.
fn padded(raster: &Raster, [top, right, bottom, left]: [usize; 4]) -> Raster {
    let (columns, rows) = (raster.width(), raster.height());
    let wide = columns + left + right;
    let mut pixels = vec![0.0_f32; wide * (rows + top + bottom) * 4];
    for (row, line) in raster.pixels().chunks_exact(columns * 4).enumerate() {
        let start = ((row + top) * wide + left) * 4;
        pixels[start..start + line.len()].copy_from_slice(line);
    }
    Raster::made(wide, rows + top + bottom, pixels)
}

/// A map of `size` (columns, rows) with `room` pixels more on each side, each the nearest pixel of the map: a mask goes on past
/// the box as it is at the box's edge, as a blur's does.
fn replicated(values: &[f32], (columns, rows): (usize, usize), [top, right, bottom, left]: [usize; 4]) -> Vec<f32> {
    let (wide, high) = (columns + left + right, rows + top + bottom);
    (0..high)
        .flat_map(|y| {
            let row = y.saturating_sub(top).min(rows - 1) * columns;
            (0..wide).map(move |x| row + x.saturating_sub(left).min(columns - 1))
        })
        .map(|at| values[at])
        .collect()
}

// Inline, because no public route reaches the limit cheaply: a layer of 2^26 pixels is a gigabyte of floats.
#[cfg(test)]
mod tests {
    use super::fits;
    use crate::geometry::MAX_LOOKED_PIXELS;

    #[test]
    fn room_is_given_only_where_the_grown_raster_stays_within_what_a_look_runs_on() {
        let side = 1 << 13; // 8192 x 8192 is the limit exactly
        assert!(fits((side, side), [0; 4]));
        assert!(!fits((side, side), [1, 0, 0, 0]));
        assert!(!fits((side, side), [0, 0, 0, 1]));
        assert!(fits((side - 2, side), [0, 1, 0, 1]));
        assert!(!fits((1, 1), [usize::MAX, usize::MAX, 0, 0]));
        assert_eq!(side * side, MAX_LOOKED_PIXELS);
    }
}
