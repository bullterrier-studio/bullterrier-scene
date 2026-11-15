//! What a layer does to the raster its source drew, before it is placed: its looks (stage 3 of "How a layer is drawn"), then
//! its masks by its own maps (stage 4). The layer owns them whatever its source; the source only draws, at the size the
//! looks run at, and hands over the maps they use.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::cache::{Cache, Key, Keyed};
use crate::error::label;
use crate::feather;
use crate::geometry::{MAX_LOOKED_PIXELS, Rect};
use crate::source::picture::Look;
use crate::source::{Drawn, Request};
use crate::{Error, Raster};

/// The reach, in pixels, from which image sums a blur by a Fourier transform (`FFT_FROM_REACH` in its `ops/blur.rs`). A band of
/// that blur differs from the whole one in the last bits, and a banded render equals the whole one bit for bit: looks whose
/// margins add up to this are run whole. Revisit when image offers an exact-only `Recipe::band_margin`.
const FOURIER_REACH: usize = 16;

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

    /// The rows of margin each side of a band of a raster of `size` (columns, rows) that this finish's looks need to run on the band
    /// as image runs them on the whole raster, or `None` where they run on the whole raster: a `light` layer (its looks run on a raster
    /// grown by room), a look or a mask that reads one of the maps `held` by the source, a recipe image cannot band (`Recipe::band_margin`), a raster whose longest side is not the
    /// canvas's (the window's canvas is the raster, and sets the look's design scale: a band of it would look at another scale), and
    /// margins that reach `FOURIER_REACH` (a blur of that reach is summed by a Fourier transform whose rounding depends on the rows
    /// it is given: banded within 1e-4, not to the bit).
    ///
    /// # Errors
    /// What image refuses of a recipe.
    pub(crate) fn band_margin(
        &self,
        request: &Request,
        size: (usize, usize),
        held: &[&str],
        light: bool,
    ) -> Result<Option<usize>, Error> {
        let canvas = request.looks_canvas();
        // a map is the picture's, not the band's: a look or a mask that reads one runs whole
        if light || held.iter().any(|name| self.uses_map(name)) || size.0.max(size.1) != canvas.0.max(canvas.1) as usize
        {
            return Ok(None);
        }
        let mut total = 0_usize;
        for look in &self.looks {
            match look.band_margin(canvas.0.max(canvas.1))? {
                Some(margin) => total = total.saturating_add(margin),
                None => return Ok(None),
            }
        }
        Ok((total < FOURIER_REACH).then_some(total))
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

    /// What the source drew (`draw`, run only if the cache holds no stage to resume from) with each look run on it, in order,
    /// then each mask by a map multiplied into its alpha. A layer with no look is as drawn: its source did the masks on its
    /// own pixels. The look sees the raster at the size the layer draws it, plus and keeps its alpha. A `light` layer
    /// (stage 7) is looked at over black, and what the look makes of the black is light: the raster is grown by the room its
    /// looks spread into (`Look::room`) before the look and kept grown, so the halo lands past the box. Any other layer is
    /// looked at its own size.
    ///
    /// **Stages:** with `chain` and two or more looks, the raster after each look but the last is kept in `cache` under the
    /// key of the looks up to it, so that a change to look `k` runs the looks from `k` only (and a reference that adds looks
    /// to the named layer's chain resumes from the named layer's stages). The finished raster is kept by the caller.
    ///
    /// # Errors
    /// What the source or image refuses of a look at this size or of a map it misses, and a blur that does not fit.
    pub(crate) fn run(
        &self,
        draw: impl FnOnce() -> Result<Drawn, Error>,
        request: &Request,
        light: bool,
        cache: &Cache,
        chain: Option<&Chain>,
    ) -> Result<Drawn, Error> {
        if !self.is_looked() {
            return draw();
        }
        let density = request.density();
        let canvas = request.looks_canvas();
        // only light shows the room: a layer that is not light keeps its source's alpha, so the halo would be cut, and a look
        // that blurs its edge would mix black in where image's own edge rule would have kept the picture's colour
        let reach = if light { Look::room(&self.looks, canvas)? } else { None };
        let kept = chain
            .filter(|_| self.looks.len() > 1)
            .map(|chain| self.stages(chain, reach))
            .transpose()?;
        let wanted: Vec<&str> = chain.map_or_else(Vec::new, |chain| {
            chain.maps.iter().copied().filter(|name| self.uses_map(name)).collect()
        });
        let resumed = kept.as_ref().and_then(|stages| {
            stages.iter().enumerate().rev().find_map(|(at, key)| {
                let stage = cache.get::<Stage>(key)?;
                wanted
                    .iter()
                    .all(|name| stage.maps.contains_key(*name))
                    .then_some((at, stage))
            })
        });
        let (mut looked, covers, ends_at_window, room, maps, first) = if let Some((at, stage)) = resumed {
            let stage = &*stage;
            (
                Arc::clone(&stage.raster),
                stage.covers,
                stage.ends_at_window,
                stage.room,
                Arc::clone(&stage.maps),
                at + 1,
            )
        } else {
            let Drawn {
                raster,
                mut covers,
                mut ends_at_window,
                mut maps,
                ..
            } = draw()?;
            let drawn_size = (raster.width(), raster.height());
            let room = reach.filter(|room| fits(drawn_size, *room)).unwrap_or_default();
            if room == [0; 4] {
                (
                    raster,
                    covers,
                    ends_at_window,
                    room,
                    Arc::new(std::mem::take(&mut maps)),
                    0,
                )
            } else {
                let replicas = maps
                    .iter()
                    .map(|(name, values)| (name.clone(), replicated(values, drawn_size, room)))
                    .collect();
                // the raster now reaches past the window by the room, so the warp lets all of it through
                ends_at_window |= covers == request.window;
                covers = grow(covers, drawn_size, room);
                (
                    Arc::new(padded(&raster, room)),
                    covers,
                    ends_at_window,
                    room,
                    Arc::new(replicas),
                    0,
                )
            }
        };
        for (at, look) in self.looks.iter().enumerate().skip(first) {
            #[cfg(test)]
            RAN.with(|ran| ran.set(ran.get() + 1));
            looked = Arc::new(look.apply(&looked, &maps, canvas, light, request.rows, room)?);
            if let Some(key) = kept.as_ref().and_then(|stages| stages.get(at)) {
                let bytes =
                    size_of_val(looked.pixels()) + maps.values().map(|values| size_of_val(&values[..])).sum::<usize>();
                let stage = Stage {
                    raster: Arc::clone(&looked),
                    maps: Arc::clone(&maps),
                    covers,
                    ends_at_window,
                    room,
                };
                cache.put(*key, stage, bytes, true);
            }
        }
        let values = maps
            .iter()
            .map(|(name, values)| (name.as_str(), values.as_slice()))
            .collect();
        Ok(Drawn {
            raster: self.masking(&looked, density, &values)?,
            covers,
            ends_at_window,
            room,
            maps: BTreeMap::new(),
        })
    }

    /// The keys of the stages kept for this chain, one for each look but the last: everything below the looks (`chain.base`),
    /// the room the whole chain grows the raster by, and the looks up to and including the stage's.
    fn stages(&self, chain: &Chain, reach: Option<[usize; 4]>) -> Result<Vec<Key>, Error> {
        (1..self.looks.len())
            .map(|count| {
                let mut key = chain.base.clone();
                key.text("stage").debug(&reach).number(count);
                for look in &self.looks[..count] {
                    look.keyed(&mut key)?;
                }
                Ok(key.finish())
            })
            .collect()
    }
}

/// Everything below a layer's looks that its stages depend on, written to a key by the caller (the source, the light and the
/// request), and the maps the source holds.
pub(crate) struct Chain<'a> {
    pub(crate) base: &'a Keyed,
    pub(crate) maps: Vec<&'a str>,
}

/// The raster after a look, as the next look takes it up: with the maps at its size (shared by every stage of one run, and
/// holding at least the maps the chain that kept it needed), the part of the box it covers and the room it was grown by.
struct Stage {
    raster: Arc<Raster>,
    maps: Arc<BTreeMap<String, Vec<f32>>>,
    covers: Rect,
    ends_at_window: bool,
    room: [usize; 4],
}

#[cfg(test)]
thread_local! {
    /// How many looks ran on this thread: what the tests below count.
    static RAN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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

// Inline, because no public route reaches the limit cheaply (a layer of 2^26 pixels is a gigabyte of floats), and none counts the
// looks that ran: `RAN` is the one seam, and the pictures themselves are compared with a cold scene's in `tests/it/cache.rs`.
#[cfg(test)]
mod tests {
    use super::{RAN, fits};
    use crate::geometry::MAX_LOOKED_PIXELS;
    use crate::{Error, Fit, Layer, Raster, Scene, Values};

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

    fn grade(exposure: f64) -> String {
        format!(r#"{{"steps": [{{"effect": "grade", "params": {{"exposure": {exposure}}}}}]}}"#)
    }

    fn picture() -> Result<Layer, Error> {
        let pixels = (0..12 * 8).flat_map(|at| [f32::from(u8::try_from(at).unwrap_or(0)) / 96.0, 0.5, 0.25, 1.0]);
        Ok(Layer::picture(Raster::rgba(12, 8, pixels.collect())?))
    }

    /// How many looks ran while `scene` (with `values` set) was rendered.
    fn looks_run(scene: &Scene, values: &[(&str, f64)]) -> Result<usize, Error> {
        let mut set = Values::new();
        for (address, value) in values {
            set = set.set(*address, *value);
        }
        let before = RAN.with(std::cell::Cell::get);
        scene.with_values(&set)?.render(24)?;
        Ok(RAN.with(std::cell::Cell::get) - before)
    }

    #[test]
    fn changing_one_look_of_a_chain_runs_that_look_and_those_after_it() -> Result<(), Error> {
        let chain = Scene::new(24, 16)?.add([picture()?
            .named("photo")?
            .look(&grade(0.1))?
            .look(&grade(0.2))?
            .look(&grade(0.3))?])?;
        assert_eq!(looks_run(&chain, &[])?, 3, "a first render runs the chain");
        assert_eq!(looks_run(&chain, &[])?, 0, "and the finished raster is kept");
        assert_eq!(looks_run(&chain, &[("photo.look#3.grade.exposure", 0.9)])?, 1);
        assert_eq!(looks_run(&chain, &[("photo.look#2.grade.exposure", 0.8)])?, 2);
        assert_eq!(looks_run(&chain, &[("photo.look.grade.exposure", 0.7)])?, 3);
        assert_eq!(looks_run(&chain, &[("photo.look#3.grade.exposure", 0.6)])?, 1);
        assert_eq!(
            looks_run(&chain, &[("photo.look#3.grade.exposure", 0.9)])?,
            0,
            "a finished raster seen before comes back"
        );
        assert_eq!(looks_run(&chain, &[("photo.look#2.grade.exposure", 0.8)])?, 0);
        Ok(())
    }

    /// How many looks ran while `scene` was rendered in bands of `band_rows` rows at `size`.
    fn looks_run_by_bands(scene: &Scene, size: u32, band_rows: usize) -> Result<usize, Error> {
        let before = RAN.with(std::cell::Cell::get);
        scene.render_bands(size, band_rows, |_, _| Ok(()))?;
        Ok(RAN.with(std::cell::Cell::get) - before)
    }

    fn looked_drawing(recipe: &str) -> Result<Scene, Error> {
        let frame = r##"{"root":[{"stack":{"items":[[{"path":"M12 22 L71 22 L71 93 L12 93 Z"}]],"size":[100,100]}},{"fill":{"color":"#c04020"}}],"version":1}"##;
        Scene::new(100, 100)?.add([Layer::drawing(frame)?.look(recipe)?])
    }

    fn looked_picture(recipe: &str) -> Result<Scene, Error> {
        Scene::new(100, 100)?.add([picture()?
            .fit([0.0, 0.0, 100.0, 100.0], Fit::Cover, [0.5, 0.5])?
            .look(recipe)?])
    }

    #[test]
    fn a_looked_drawing_is_looked_for_each_band_where_image_can_band_it_and_once_where_it_cannot() -> Result<(), Error>
    {
        let step =
            |effect: &str, params: &str| format!(r#"{{"steps": [{{"effect": "{effect}", "params": {params}}}]}}"#);
        // ten bands of ten rows: a look for each; a look that cannot be banded runs on the whole once and is cut
        let bandable = [
            step("vignette", r#"{"amount": 0.9}"#),
            step("gaussian_blur", r#"{"radius": 8.0}"#),
        ];
        for recipe in &bandable {
            assert_eq!(looks_run_by_bands(&looked_drawing(recipe)?, 100, 10)?, 10, "{recipe}");
            assert_eq!(
                looks_run_by_bands(&looked_picture(recipe)?, 100, 10)?,
                10,
                "{recipe} on a picture"
            );
        }
        let whole = [
            step("gaussian_blur", r#"{"radius": 300.0}"#),
            step("gaussian_blur", r#"{"radius": 24.0}"#),
            step("auto_levels", "{}"),
        ];
        for recipe in &whole {
            assert_eq!(looks_run_by_bands(&looked_drawing(recipe)?, 100, 10)?, 1, "{recipe}");
            assert_eq!(
                looks_run_by_bands(&looked_picture(recipe)?, 100, 10)?,
                1,
                "{recipe} on a picture"
            );
        }
        Ok(())
    }

    /// The planes of a banded render are used by their own band alone, so they never cost the cache a finished layer: a looked layer
    /// that fills the cache is looked once however many bands a masked layer over it takes.
    #[test]
    fn the_planes_of_the_bands_do_not_push_a_finished_layer_out_of_a_full_cache() -> Result<(), Error> {
        let looked = picture()?
            .fit([0.0, 0.0, 100.0, 100.0], Fit::Cover, [0.5, 0.5])?
            .look(r#"{"steps": [{"effect": "auto_levels", "params": {}}]}"#)?
            .named("photo")?;
        // the finished raster is 100 x 100 x 16 bytes; the room left is less than one plane of a band
        let scene = Scene::new(100, 100)?.with_cache_capacity(160_000 + 1_000).add([
            looked,
            Layer::fill("#c04020")?.mask(Layer::fill("#ffffff")?.fit(
                [10.0, 10.0, 80.0, 80.0],
                Fit::Stretch,
                [0.5, 0.5],
            )?)?,
        ])?;
        let before = RAN.with(std::cell::Cell::get);
        scene.render_bands(100, 10, |_, _| Ok(()))?;
        assert_eq!(RAN.with(std::cell::Cell::get) - before, 1);
        Ok(())
    }

    #[test]
    fn references_that_add_looks_to_one_chain_run_its_looks_once() -> Result<(), Error> {
        let scene = Scene::new(24, 16)?.add([
            picture()?.named("night")?.look(&grade(0.1))?,
            Layer::of("night").named("a")?.look(&grade(0.2))?,
            Layer::of("night").named("b")?.look(&grade(0.3))?,
        ])?;
        // night runs its look; `a` runs night's and its own, keeping the stage between; `b` takes the stage, runs its own
        assert_eq!(looks_run(&scene, &[])?, 1 + 2 + 1);
        assert_eq!(looks_run(&scene, &[("a.look.grade.exposure", 0.5)])?, 1);
        assert_eq!(
            looks_run(
                &scene,
                &[("a.look.grade.exposure", 0.6), ("b.look.grade.exposure", 0.7)]
            )?,
            2
        );
        assert_eq!(
            looks_run(&scene, &[("night.look.grade.exposure", 0.4)])?,
            1 + 2 + 1,
            "the named layer's look runs again for night and for `a`, and `b` takes the new stage"
        );
        Ok(())
    }
}
