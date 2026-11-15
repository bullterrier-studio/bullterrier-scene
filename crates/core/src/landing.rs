//! Where a layer lands: the output grid it is drawn for (the canvas, or the canvas and a margin for a mask whose blur reaches
//! past it), and the one warp that puts the layer's drawn raster on it. The renderer lays layers over one another through it,
//! and a mask draws its layer through it onto a plane of its own.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::cache::{Cache, Keyed};
use crate::finish::Chain;
use crate::finish::Finish;
use crate::geometry::{MAX_LOOKED_PIXELS, MAX_PIXELS, Rect, Scale};
use crate::layer::Body;
use crate::placement::{Layout, Placement};
use crate::source::{Drawn, Opened, Request, Rows, Shape};
use crate::warp::{Map, Output, View, is_identity, reduce, reduced_map, reduction, rows_read, warp_into};
use crate::{Error, Raster};

/// Where a layer is drawn: the output grid of the canvas, with `margin` more pixels on each side (along x and y) for a mask
/// whose blur reaches past the canvas.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Target<'a> {
    /// The scene's canvas, in design pixels.
    pub(crate) canvas: Rect,
    pub(crate) scale: Scale,
    /// The canvas's size in output pixels.
    pub(crate) columns: usize,
    pub(crate) rows: usize,
    pub(crate) margin: (usize, usize),
    /// The rows of the plane (margin included) that are drawn: `(first, count)`. The whole plane for the whole canvas, a band
    /// of it for a band: a pixel is the same whichever rows are drawn with it.
    pub(crate) window: (usize, usize),
    /// How far, in output pixels, the canvas's top left lies from the grid's (margin not counted): 0 for the whole canvas, and
    /// the part of a pixel a scene used as a layer starts at when its box is not on a pixel.
    pub(crate) offset: (f64, f64),
    /// The pixels the scenes drawn round this one hold in their own rasters while it is drawn: 0 for the whole canvas.
    pub(crate) held: usize,
    /// What the renders of the scene kept: shared by every layer drawn on this target and by the scenes inside it.
    pub(crate) cache: &'a Cache,
}

impl Target<'_> {
    /// The size in pixels, margin included: `(columns, rows)`.
    pub(crate) fn size(&self) -> (usize, usize) {
        (self.columns + 2 * self.margin.0, self.rows + 2 * self.margin.1)
    }

    /// The pixels that are drawn: the plane's columns, and the rows of the window.
    pub(crate) fn drawn_size(&self) -> (usize, usize) {
        (self.size().0, self.window.1)
    }

    /// Whether only some rows of the plane are drawn.
    pub(crate) fn is_band(&self) -> bool {
        self.window != (0, self.size().1)
    }

    /// The plane and the rows of it that are drawn, as a warp is told them.
    pub(crate) fn output(&self) -> Output {
        let (columns, rows) = self.size();
        Output {
            columns,
            rows,
            first: self.window.0,
            count: self.window.1,
        }
    }

    /// This target with `reach` more pixels of margin on each side, along x and y, and the rows a blur of that reach (whose
    /// pass along y is made of boxes `width` pixels wide, 1 for none) needs to give this target's rows: the rows of this
    /// window and `reach.1` more on each side, the first moved up to a multiple of `width` so that the boxes of the blur
    /// fall where they fall on the whole plane (a sum of floats is the same only if it is added in the same order).
    pub(crate) fn grown(&self, reach: (usize, usize), width: usize) -> Self {
        let first = if reach.1 == 0 {
            self.window.0
        } else {
            self.window.0 / width * width
        };
        let end = self.window.0 + self.window.1 + 2 * reach.1;
        Self {
            margin: (self.margin.0 + reach.0, self.margin.1 + reach.1),
            window: (first, end - first),
            ..*self
        }
    }

    /// Writes into `key` what a plane or a raster drawn for this target depends on: the grid it is drawn on and where the canvas
    /// lies on it. Not the cache.
    pub(crate) fn keyed(&self, key: &mut Keyed) {
        key.debug(&(
            self.canvas,
            self.scale,
            self.columns,
            self.rows,
            self.margin,
            self.window,
            self.offset,
            self.held,
        ));
    }

    /// The part of the canvas plane the target holds, in design pixels: the canvas grown by the margin.
    fn rect(&self) -> Rect {
        // a margin of at most 2^27 pixels is exact in a float
        #[expect(clippy::cast_precision_loss, reason = "a margin of at most 2^27 pixels")]
        let (across, down) = (
            self.margin.0 as f64 / self.scale.x.pixels(1.0),
            self.margin.1 as f64 / self.scale.y.pixels(1.0),
        );
        Rect {
            x: self.canvas.x - across,
            y: self.canvas.y - down,
            width: self.canvas.width + 2.0 * across,
            height: self.canvas.height + 2.0 * down,
        }
    }
}

/// What the cache keeps of a layer drawn: the raster its source drew and its looks and masks by its own maps finished, before
/// the one warp puts it on a target. Where the raster is held in the cache, it is shared (it is read, never changed).
pub(crate) struct Kept {
    raster: Arc<Raster>,
    covers: Rect,
    ends_at_window: bool,
    room: [usize; 4],
}

impl Kept {
    fn of(drawn: Drawn) -> Self {
        Self {
            raster: drawn.raster,
            covers: drawn.covers,
            ends_at_window: drawn.ends_at_window,
            room: drawn.room,
        }
    }

    fn drawn(&self) -> Drawn {
        Drawn {
            raster: Arc::clone(&self.raster),
            covers: self.covers,
            ends_at_window: self.ends_at_window,
            room: self.room,
            maps: BTreeMap::new(),
        }
    }
}

/// A layer drawn at the scale it will have and ready to be put where it belongs: the raster, and the one warp that puts it on
/// the target.
pub(crate) struct Landed {
    pub(crate) drawn: Drawn,
    pub(crate) map: Map,
    pub(crate) clip: Rect,
    pub(crate) output: Output,
    /// The first row of the picture the raster holds and the rows the picture has: a layer drawn by window holds some of them.
    first: usize,
    total: usize,
}

impl Landed {
    /// Whether the raster is the target as it is (an unmoved layer the size of the output).
    pub(crate) fn is_unmoved(&self) -> bool {
        self.view().is_whole()
            && matches!(self.map, Map::Affine(affine) if is_identity(&self.drawn.raster, affine, self.clip, self.output.columns, self.output.rows))
    }

    /// The raster as the rows of the picture it holds.
    pub(crate) fn view(&self) -> View<'_> {
        View::part(&self.drawn.raster, self.first, self.total)
    }

    /// The pixels of the raster that are the output's wanted rows, where the layer is unmoved.
    pub(crate) fn unmoved_pixels(&self) -> &[f32] {
        let Output {
            columns, first, count, ..
        } = self.output;
        &self.drawn.raster.pixels()[first * columns * 4..(first + count) * columns * 4]
    }

    /// Gives `put(index, pixel)` each pixel (straight RGBA) the layer lands on, `index` being its place in the target, row
    /// after row. A pixel the layer does not reach is not given.
    pub(crate) fn each(&self, put: &mut impl FnMut(usize, [f32; 4])) {
        if self.is_unmoved() {
            for (at, pixel) in self.unmoved_pixels().as_chunks::<4>().0.iter().enumerate() {
                put(at, *pixel);
            }
        } else {
            warp_into(self.view(), self.map, self.clip, self.output, put);
        }
    }
}

/// What a layer drawn by window is drawn for, where it is: on a band, a source that draws its whole box and can draw some rows of it
/// alone, a warp that reads only some of those rows, and looks that image can run on a band (or none). The map and clip of the whole
/// raster, the rows to draw, and the rows of margin the looks need each side; `None` for any other layer, which is drawn whole.
///
/// # Errors
/// What image refuses of a look.
fn rows_to_draw(
    opened: &Opened,
    request: &Request,
    target: &Target,
    finish: &Finish,
    held: &[&str],
    light: bool,
    placed: impl Fn(Shape) -> (Map, Rect),
) -> Result<Option<(Map, Rect, Rows, usize)>, Error> {
    // a source with no look to run on it hands over its own pixels (`own_pixels`) and draws no raster to window
    if !(target.is_band() && opened.by_window && (finish.is_looked() || !opened.own_pixels)) {
        return Ok(None);
    }
    let shape = if opened.whole {
        request.drawn_shape(opened.own)
    } else {
        request.window_shape()
    };
    // looks run on the rows of the band and the margin they read round them (none: no look)
    let margin = if finish.is_looked() {
        let Some(margin) = finish.band_margin(request, shape.size, held, light)? else {
            return Ok(None);
        };
        margin
    } else {
        0
    };
    let (map, clip) = placed(shape);
    let Some((first, end)) = rows_read(map, target.output(), shape.size) else {
        return Ok(None);
    };
    let total = shape.size.1;
    Ok(((first, end) != (0, total)).then_some((map, clip, Rows { first, end, total }, margin)))
}

/// A layer drawn for the rows `read` of its raster: the source draws them and `margin` rows more each side, the looks run on all of
/// them as the rows of the whole raster, and the margin is cut. No chain of looks is kept: a band's looks are not another band's.
///
/// # Errors
/// What the source and the looks refuse.
fn draw_rows(
    opened: Opened,
    request: Request,
    finish: &Finish,
    light: bool,
    cache: &Cache,
    (read, margin): (Rows, usize),
) -> Result<Drawn, Error> {
    let wide = Rows {
        first: read.first.saturating_sub(margin),
        end: read.end.saturating_add(margin).min(read.total),
        total: read.total,
    };
    let mut drawn = opened.draw(
        Request {
            rows: Some(wide),
            ..request
        },
        finish,
        light,
        cache,
        None,
    )?;
    if (wide.first, wide.end) != (read.first, read.end) {
        drawn.raster = Arc::new(cut(&drawn.raster, read.first - wide.first..read.end - wide.first));
    }
    Ok(drawn)
}

/// A layer whose map shrinks its raster past 2x has the raster averaged down first (`warp`): on a band, that is the same average
/// for each band, so it is made once, kept in the cache and found by the next band. The layer lands from the averaged raster, as
/// the warp would have made it, so a pixel is the same.
///
/// # Errors
/// A source or look that cannot be written into a key.
fn reduced_for_bands(
    body: &Body,
    request: &Request,
    target: &Target,
    light: bool,
    (mut drawn, map, clip): (Drawn, Map, Rect),
) -> Result<(Drawn, Map, Rect), Error> {
    let Map::Affine(affine) = map else {
        return Ok((drawn, map, clip));
    };
    let from = (drawn.raster.width(), drawn.raster.height());
    let Some(to) = target.is_band().then(|| reduction(from, affine)).flatten() else {
        return Ok((drawn, map, clip));
    };
    // what the reduction is of: the source and its looks and masks (they make the raster) at this request, and the size
    let mut key = Keyed::new("reduced");
    body.source.keyed(&mut key)?;
    body.finish.keyed(&mut key)?;
    key.debug(&(light, request, to));
    let raster = target.cache.fetch(&key.finish(), || {
        let reduced = reduce(&drawn.raster, to.0, to.1);
        let bytes = size_of_val(reduced.pixels());
        Ok((reduced, bytes))
    })?;
    let (affine, clip) = reduced_map(from, to, affine, clip);
    drawn.raster = raster;
    Ok((drawn, Map::Affine(affine), clip))
}

/// The rows `rows` of `raster`.
fn cut(raster: &Raster, rows: std::ops::Range<usize>) -> Raster {
    let line = raster.width() * 4;
    Raster::made(
        raster.width(),
        rows.len(),
        raster.pixels()[rows.start * line..rows.end * line].to_vec(),
    )
}

/// `body` drawn at the scale it will have, and put on `target`: the source asked for the part of its box that shows, then the
/// map of the one warp worked out.
///
/// On a band of the plane, a source that draws its whole box and can draw some rows of it alone (a drawing, a group), with no
/// look to run on it, is asked only for the rows the band reads (`warp::rows_read`): the layer is never held whole, and the
/// limit of 2^27 pixels is on those rows. Every other layer is drawn whole (and kept, where it is worth keeping), and the band
/// is cut from it by the warp.
pub(crate) fn land(body: &Body, target: &Target, light: bool) -> Result<Landed, Error> {
    // a source with no box of its own (a fill) takes the part of the canvas the target holds, margin included, so that a mask
    // that fills the canvas goes on into the margin; but only where nothing acts on the box, for `fit` and `place` work on
    // it and a layer must be the same shape whatever its blur adds
    let unplaced = body.fit.is_none() && body.placement == Placement::default() && body.extras.is_empty();
    let opened = body
        .source
        .open(if unplaced { target.rect() } else { target.canvas }, target.cache)?;
    let layout = Layout::new(body.fit, body.placement, &body.extras, opened.own, target.scale).offset(target.offset);
    let request = Request {
        held: target.held,
        canvas: (target.canvas.width, target.canvas.height),
        ..layout.request(
            opened.own,
            opened.whole,
            if body.finish.is_looked() {
                MAX_LOOKED_PIXELS
            } else {
                MAX_PIXELS
            },
        )
    };
    // the output starts `margin` pixels, and the offset, up and left of the canvas
    #[expect(clippy::cast_precision_loss, reason = "a margin of at most 2^27 pixels")]
    let (across, down) = (
        target.margin.0 as f64 + target.offset.0,
        target.margin.1 as f64 + target.offset.1,
    );
    let placed = |shape: Shape| {
        let (map, clip) = layout.warp(shape);
        (
            if (across, down) == (0.0, 0.0) {
                map
            } else {
                map.shifted(across, down)
            },
            clip,
        )
    };
    if let Some((map, clip, read, margin)) = rows_to_draw(
        &opened,
        &request,
        target,
        &body.finish,
        &body.source.map_names().unwrap_or_default(),
        light,
        placed,
    )? {
        return Ok(Landed {
            drawn: draw_rows(opened, request, &body.finish, light, target.cache, (read, margin))?,
            map,
            clip,
            output: target.output(),
            first: read.first,
            total: read.total,
        });
    }
    // the one place the drawn-size limit is checked, before any source draws: a source that draws its own pixels allocates
    // nothing of the size it is drawn at, so a large scale on it is no refusal
    if opened.own_pixels && !body.finish.is_looked() {
        request.scale.sides_within_limit(request.window)?;
    } else {
        request.scale.size(request.window)?;
    }
    let drawn = if body.keeps_drawn() {
        // the phase and the pixels held round the scene are inputs only of a source that draws on the grid: a picture that
        // moves, or sits in a group drawn at another phase, keeps its raster
        let grid = body.source.follows_grid();
        let (phase, held) = if grid {
            (request.phase, request.held)
        } else {
            ((0.0, 0.0), 0)
        };
        // everything below the looks: what a stage of the chain of looks and the finished raster both depend on
        let mut base = Keyed::new("drawn");
        body.source.keyed(&mut base)?;
        base.debug(&(light, Request { phase, held, ..request }));
        let mut key = base.clone();
        key.text("finished");
        body.finish.keyed(&mut key)?;
        let kept = target.cache.fetch(&key.finish(), || {
            let chain = Chain {
                base: &base,
                maps: body.source.map_names().unwrap_or_default(),
            };
            let drawn = opened.draw(request, &body.finish, light, target.cache, Some(&chain))?;
            let bytes = size_of_val(drawn.raster.pixels());
            Ok((Kept::of(drawn), bytes))
        })?;
        kept.drawn()
    } else {
        opened.draw(request, &body.finish, light, target.cache, None)?
    };
    let (map, clip) = placed(drawn.shape());
    let (drawn, map, clip) = reduced_for_bands(body, &request, target, light, (drawn, map, clip))?;
    let total = drawn.raster.height();
    Ok(Landed {
        drawn,
        map,
        clip,
        output: target.output(),
        first: 0,
        total,
    })
}

// Inline, because no public route counts how often a raster is averaged down: `warp::reductions_of` is the one seam, and the
// pictures are compared with the whole render's by the oracle of `tests/it/bands.rs`.
#[cfg(test)]
mod tests {
    use crate::warp::reductions_of;
    use crate::{Error, Fit, Layer, Raster, Scene};

    /// A picture whose size no other test makes, shrunk to a tenth of its width.
    fn shrunk() -> Result<Scene, Error> {
        let (columns, rows) = (397, 293);
        let pixels =
            (0..columns * rows).flat_map(|at| [f32::from(u8::try_from(at % 251).unwrap_or(0)) / 251.0, 0.5, 0.25, 1.0]);
        Scene::new(40, 30)?.add([Layer::picture(Raster::rgba(columns, rows, pixels.collect())?).fit(
            [0.0, 0.0, 40.0, 30.0],
            Fit::Cover,
            [0.5, 0.5],
        )?])
    }

    #[test]
    fn a_picture_shrunk_past_2x_is_averaged_down_once_however_many_bands_there_are() -> Result<(), Error> {
        let scene = shrunk()?;
        let before = reductions_of((397, 293));
        scene.render_bands(40, 3, |_, _| Ok(()))?;
        assert_eq!(reductions_of((397, 293)) - before, 1, "ten bands of three rows");
        // and the next render takes it from the cache
        scene.render_bands(40, 7, |_, _| Ok(()))?;
        assert_eq!(reductions_of((397, 293)) - before, 1);
        Ok(())
    }
}
