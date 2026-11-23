//! Drawing a scene: the size of the picture, each layer drawn at it, joined by its blend mode in premultiplied floats, and
//! given back as straight alpha.

use std::ops::Range;

use rayon::prelude::*;

use crate::blend::{Blend, join, lit};
use crate::cache::{Cache, Keyed};
use crate::geometry::{MAX_PIXELS, Rect, Scale};
use crate::landing::{Target, land};
use crate::layer::Join;
use crate::mask::{Plane, product};
use crate::{Error, Layer, Raster};

/// The longest side of a picture, in output pixels.
const MAX_SIZE: u32 = 65_000;

/// The pixels a band of a picture written straight to a file holds at most: 2^24 (a band of float32 RGBA is 256 MiB), an
/// eighth of what a whole render may draw ([`MAX_PIXELS`]). A picture of no more pixels than this is one band, the same as a
/// whole render: the command line writes it as one, and streams only a larger one.
pub const BAND_PIXELS: usize = MAX_PIXELS / 8;

/// The pixels of a chunk of rows that one thread lays: 2^14 (256 KiB of float32 RGBA), so a thumbnail is one chunk and a poster
/// is dozens.
const CHUNK_PIXELS: usize = 1 << 14;

/// The most pixels of a whole render whose joins are kept: 2^22 (64 MiB of float32 RGBA a join, a picture of 2,400 by 1,750), so a
/// stack of a dozen layers holds under a gigabyte. A larger render draws every layer each time.
const PREFIX_PIXELS: usize = 1 << 22;

/// The columns and rows of the picture a canvas of `width` by `height` design pixels makes at `size` on its long side.
///
/// # Errors
/// [`Error::Invalid`] naming `size` for a size outside 1 to 65,000.
pub(crate) fn picture_size(width: u32, height: u32, size: i128) -> Result<(usize, usize), Error> {
    let Some(size) = u32::try_from(size).ok().filter(|size| (1..=MAX_SIZE).contains(size)) else {
        return Err(Error::invalid(
            "size",
            format!("size is a whole number from 1 to {MAX_SIZE}, got {size}"),
        ));
    };
    let long = width.max(height);
    let output = |canvas: u32| {
        usize::try_from((u64::from(canvas) * u64::from(size)).div_ceil(u64::from(long))).unwrap_or(usize::MAX)
    };
    Ok((output(width), output(height)))
}

/// The columns and rows of a whole render at `size`: [`picture_size`], and at most [`MAX_PIXELS`] pixels in all.
///
/// # Errors
/// [`Error::Invalid`] naming `size`.
pub(crate) fn checked_size(width: u32, height: u32, size: i128) -> Result<(usize, usize), Error> {
    let (columns, rows) = picture_size(width, height, size)?;
    if columns.saturating_mul(rows) > MAX_PIXELS {
        return Err(Error::invalid(
            "size",
            format!(
                "size {size} gives a picture of {columns} x {rows} pixels; at most {MAX_PIXELS} pixels are drawn (2^27)"
            ),
        ));
    }
    Ok((columns, rows))
}

/// The rows a band of a picture of `columns` by `rows` has when the caller does not choose: as many as [`BAND_PIXELS`] holds,
/// and at least one.
pub(crate) fn default_band_rows(columns: usize, rows: usize) -> usize {
    (BAND_PIXELS / columns).clamp(1, rows)
}

/// `layers` drawn on a canvas of `width` by `height` design pixels at `size` output pixels on the long side.
pub(crate) fn render(width: u32, height: u32, layers: &[Layer], size: i128, cache: &Cache) -> Result<Raster, Error> {
    let (columns, rows) = checked_size(width, height, size)?;
    cache.begin_render(false);
    band(width, height, layers, (columns, rows), 0..rows, cache)
}

/// `layers` drawn as the bands of the picture, each `band_rows` rows (the last has what is left), top to bottom: `sink` is
/// given the rows a band is and the band, and a refusal of it ends the render and is the render's. The bands put together are
/// the picture [`render`] makes, within the one tolerance, whatever `band_rows`; no more than a band of the picture is held at a time.
pub(crate) fn render_bands(
    (width, height): (u32, u32),
    layers: &[Layer],
    size: i128,
    band_rows: usize,
    cache: &Cache,
    sink: &mut impl FnMut(Range<usize>, &Raster) -> Result<(), Error>,
) -> Result<(), Error> {
    let (columns, rows) = picture_size(width, height, size)?;
    if !(1..=rows).contains(&band_rows) || columns.saturating_mul(band_rows) > MAX_PIXELS {
        return Err(Error::invalid(
            "band_rows",
            format!(
                "a band has from 1 to {rows} rows (the picture's height) and at most {MAX_PIXELS} pixels (2^27), {columns} wide here; got {band_rows}"
            ),
        ));
    }
    cache.begin_render(band_rows < rows);
    for first in (0..rows).step_by(band_rows) {
        let rows_of = first..(first + band_rows).min(rows);
        let drawn = band(width, height, layers, (columns, rows), rows_of.clone(), cache)?;
        sink(rows_of, &drawn)?;
    }
    Ok(())
}

/// The rows `rows_of` of the picture of `columns` by `rows` pixels that `layers` make on the canvas, as a raster.
fn band(
    width: u32,
    height: u32,
    layers: &[Layer],
    (columns, rows): (usize, usize),
    rows_of: Range<usize>,
    cache: &Cache,
) -> Result<Raster, Error> {
    let target = target_of(width, height, (columns, rows), (rows_of.start, rows_of.len()), cache);
    let joined = draw(layers, &target)?;
    Ok(Raster::made(columns, rows_of.len(), straight(joined)))
}

/// The target of a picture of `columns` by `rows` pixels of a canvas of `width` by `height` design pixels, of which the rows
/// `window` (first, count) are drawn: the one place a render's target is made.
pub(crate) fn target_of(
    width: u32,
    height: u32,
    (columns, rows): (usize, usize),
    window: (usize, usize),
    cache: &Cache,
) -> Target<'_> {
    Target {
        canvas: Rect {
            x: 0.0,
            y: 0.0,
            width: f64::from(width),
            height: f64::from(height),
        },
        scale: Scale::of_canvas(columns, rows, width, height),
        columns,
        rows,
        margin: (0, 0),
        window,
        offset: (0.0, 0.0),
        held: 0,
        cache,
    }
}

/// `layers` drawn one over the other on `target`: premultiplied red, green, blue and alpha of each of its pixels. The one
/// entry that draws a stack, for the whole canvas ([`render`]) and for a scene used as a layer.
///
/// A whole render of at most [`PREFIX_PIXELS`] pixels keeps the join after each layer in the cache (expendable), found by what
/// the layers up to it and the target are made of: a render starts from the highest of them it finds and lays only the layers
/// above, so a change to one layer redoes that layer and those over it.
pub(crate) fn draw(layers: &[Layer], target: &Target) -> Result<Vec<f32>, Error> {
    let (columns, rows) = target.drawn_size();
    let mut joined = vec![0.0_f32; columns * rows * 4];
    let mut first = 0;
    let mut keys = Vec::new();
    if !target.is_band() && target.held == 0 && columns * rows <= PREFIX_PIXELS {
        let mut key = Keyed::new("composite");
        target.keyed(&mut key);
        for layer in layers {
            // a layer that cannot be written into a key is drawn, and refused, as it is without the cache
            if layer.keyed(&mut key).is_err() {
                break;
            }
            keys.push(key.clone().finish());
        }
        if let Some((at, hit)) = keys
            .iter()
            .enumerate()
            .rev()
            .find_map(|(at, key)| target.cache.get::<Vec<f32>>(key).map(|hit| (at, hit)))
        {
            joined.copy_from_slice(&hit);
            first = at + 1;
        }
    }
    for (index, layer) in layers.iter().enumerate().skip(first) {
        let name = layer.name.clone().unwrap_or_else(|| format!("layer{}", index + 1));
        lay(layer, target, &mut joined).map_err(|error| error.in_layer(&name))?;
        // shortcut: every other join is kept (the top one first), halving the copies of a first render; a change to a layer
        // whose join is not kept lays one layer more. Upgrade: keep each if a measurement shows the copies are cheap
        if let Some(key) = keys.get(index).filter(|_| (keys.len() - 1 - index) % 2 == 0) {
            target
                .cache
                .put(*key, joined.clone(), size_of_val(joined.as_slice()), true);
        }
    }
    Ok(joined)
}

/// `layer` laid over `joined`: its source drawn at the scale it will have, then put where it belongs by one warp, each pixel
/// of which is blended where it lands. A layer that is not moved is blended as it is. Where the layer has masks, each pixel
/// is blended at its mask's value too. A layer at opacity 0, or whose masks are 0 at every pixel, is not drawn: in no blend mode can it change what is below.
fn lay(layer: &Layer, target: &Target, joined: &mut [f32]) -> Result<(), Error> {
    #[cfg(test)]
    LAID.with(|laid| laid.set(laid.get() + 1));
    if layer.join.opacity == 0.0 {
        return Ok(());
    }
    let factor = product(&layer.join, target)?;
    // masks that leave nothing visible: the weight is 0 at every pixel, so the layer, looks included, is not drawn
    if factor
        .as_ref()
        .is_some_and(|factor| factor.values.iter().all(|&value| value == 0.0))
    {
        return Ok(());
    }
    let Join {
        opacity, blend, light, ..
    } = layer.join;
    // a layer whose masks leave only some rows is drawn, looks included, for those rows alone, as a band is
    let (head, end) = factor
        .as_ref()
        .filter(|_| windowing(target))
        .map_or((0, target.window.1), |factor| support(factor, target.drawn_size().0));
    let windowed = Target {
        window: (target.window.0 + head, end - head),
        ..*target
    };
    #[cfg(test)]
    if (head, end) != (0, target.window.1) {
        WINDOWED.with(|count| count.set(count.get() + 1));
    }
    let landed = land(layer.drawable()?, &windowed, light)?;
    let columns = landed.output.columns;
    let rows_per_chunk = (CHUNK_PIXELS / columns).max(1);
    // each chunk is a few whole rows of the output, laid by the thread that takes it: a pixel is the same whichever rows are
    // laid with it (`warp_into`), and each pixel is blended into its own place alone
    joined[head * columns * 4..end * columns * 4]
        .par_chunks_mut(rows_per_chunk * columns * 4)
        .enumerate()
        .for_each(|(n, chunk)| {
            let first = n * rows_per_chunk;
            let rows = first..first + chunk.len() / (columns * 4);
            if blend != Blend::Normal || light {
                // every pixel is made light if the layer is (stage 7), scaled by the masks and the opacity (stages 6 and 8), and
                // joined where it lands by the compositing equation of its mode (stage 9)
                landed.each_in(rows, &mut |at, above @ [red, green, blue, alpha]| {
                    let weight = factor
                        .as_ref()
                        .map_or(opacity, |factor| opacity * factor.values[(head + first) * columns + at]);
                    let (colour, alpha) = if light { lit(above) } else { ([red, green, blue], alpha) };
                    join(&mut chunk[at * 4..at * 4 + 4], colour, alpha * weight, blend);
                });
            } else if let Some(factor) = &factor {
                landed.each_in(rows, &mut |at, above| {
                    over_pixel(
                        &mut chunk[at * 4..at * 4 + 4],
                        above,
                        opacity * factor.values[(head + first) * columns + at],
                    );
                });
            } else if landed.is_unmoved() {
                over(chunk, landed.unmoved_pixels(rows), opacity);
            } else {
                landed.each_in(rows, &mut |at, above| {
                    over_pixel(&mut chunk[at * 4..at * 4 + 4], above, opacity);
                });
            }
        });
    Ok(())
}

/// Whether a layer is drawn for the rows its masks leave: only in a render too large to keep joins, which draws every layer each
/// time. A render that keeps joins keeps what each layer drew whole, so a change to its opacity or its mask draws no look again;
/// a look drawn for some rows is not that raster.
#[cfg(not(test))]
fn windowing(target: &Target) -> bool {
    target.columns * target.rows > PREFIX_PIXELS
}

/// [`windowing`], with a switch for the tests to draw every layer whole.
#[cfg(test)]
fn windowing(target: &Target) -> bool {
    target.columns * target.rows > PREFIX_PIXELS && !WHOLE.with(std::cell::Cell::get)
}

/// The rows of `factor` (`columns` wide) that are not 0, as `(first, end)` counted from its first row: all of them where its
/// first and last rows are not 0. A layer with no row left is not drawn (`lay` returned before).
fn support(factor: &Plane, columns: usize) -> (usize, usize) {
    let row_is_empty = |row: &[f32]| row.iter().all(|&value| value == 0.0);
    let rows = factor.values.chunks(columns);
    let head = rows.clone().take_while(|row| row_is_empty(row)).count();
    let tail = rows.rev().take_while(|row| row_is_empty(row)).count();
    (head, factor.values.len() / columns - tail)
}

/// `above` (straight RGBA, at `opacity` of its alpha) laid over `joined` (premultiplied): the "over" of Porter and Duff,
/// `above + joined * (1 - alpha of above)`.
fn over(joined: &mut [f32], above: &[f32], opacity: f32) {
    let (above, _) = above.as_chunks::<4>();
    let (joined, _) = joined.as_chunks_mut::<4>();
    for (below, above) in joined.iter_mut().zip(above) {
        over_pixel(below, *above, opacity);
    }
}

/// One pixel of [`over`].
#[inline]
fn over_pixel(below: &mut [f32], above: [f32; 4], opacity: f32) {
    let alpha = above[3] * opacity;
    for channel in 0..3 {
        below[channel] = above[channel] * alpha + below[channel] * (1.0 - alpha);
    }
    below[3] = alpha + below[3] * (1.0 - alpha);
}

/// `joined` (premultiplied) as straight alpha: the colour over the alpha, nothing where nothing is drawn.
pub(crate) fn straight(mut joined: Vec<f32>) -> Vec<f32> {
    for pixel in joined.as_chunks_mut::<4>().0 {
        let alpha = pixel[3];
        for channel in &mut pixel[..3] {
            // a premultiplied colour never passes its alpha (each step is monotone in both), so the quotient is at most 1
            *channel = if alpha > 0.0 { *channel / alpha } else { 0.0 };
        }
    }
    joined
}

// Inline, because no public route counts the layers a render lays: a prefix of the join kept in the cache is read with no
// layer drawn, and the picture is the same within the one tolerance (the oracles of `tests/it/cache.rs`).
#[cfg(test)]
thread_local! {
    static LAID: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// The layers whose look was drawn for some rows only, and a switch that draws every layer whole, the oracle.
    static WINDOWED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static WHOLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
mod tests {
    use super::{LAID, PREFIX_PIXELS, WHOLE, WINDOWED};
    use crate::{Deg, Error, Fit, Layer, Mask, Place, Raster, Scene, Values};

    fn four(canvas: (u32, u32)) -> Result<Scene, Error> {
        Scene::new(canvas.0, canvas.1)?.add([
            Layer::fill("#204080")?.named("bottom")?,
            Layer::fill("#ff8000")?.named("lower")?.opacity(0.5)?,
            Layer::fill("#00ff80")?.named("upper")?.opacity(0.5)?,
            Layer::fill("#ffffff")?.named("top")?.opacity(0.25)?,
        ])
    }

    /// The layers `scene` lays to render at `size`.
    fn laid(scene: &Scene, size: u32) -> Result<usize, Error> {
        let before = LAID.with(std::cell::Cell::get);
        scene.render(size)?;
        Ok(LAID.with(std::cell::Cell::get) - before)
    }

    #[test]
    fn a_change_redoes_the_layer_and_those_over_it_and_not_those_under_a_kept_join() -> Result<(), Error> {
        let scene = four((20, 10))?;
        assert_eq!(laid(&scene, 20)?, 4, "the first render lays every layer");
        assert_eq!(laid(&scene, 20)?, 0, "the same scene again lays none");
        let top = scene.with_values(&Values::new().set("top.opacity", 0.5))?;
        // every other join is kept: the one under the top layer is not, so the layer below it is laid too
        assert_eq!(laid(&top, 20)?, 2);
        let bottom = scene.with_values(&Values::new().set("bottom.opacity", 0.5))?;
        assert_eq!(laid(&bottom, 20)?, 4);
        assert_eq!(laid(&bottom, 20)?, 0);
        assert_eq!(laid(&scene, 21)?, 4, "another size is another picture");
        Ok(())
    }

    #[test]
    fn a_render_of_more_pixels_than_a_join_may_be_kept_for_keeps_none() -> Result<(), Error> {
        // 3,000 by 1,500 pixels
        let scene = four((20, 10))?;
        const { assert!(3_000 * 1_500 > PREFIX_PIXELS) };
        assert_eq!(laid(&scene, 3_000)?, 4);
        assert_eq!(laid(&scene, 3_000)?, 4, "nothing was kept");
        Ok(())
    }

    /// `base`, looked and masked to four rows of a picture 30 rows high, over a ground, on a canvas whose render at 3,000 is too
    /// large to keep joins.
    fn masked_rows(base: Layer, look: &str) -> Result<Scene, Error> {
        let mut pixels = vec![0.0; 60 * 30 * 4];
        for row in 10..14 {
            pixels[row * 60 * 4..(row + 1) * 60 * 4].fill(1.0);
        }
        let mask =
            Layer::picture(Raster::rgba(60, 30, pixels)?).fit([0.0, 0.0, 300.0, 150.0], Fit::Stretch, [0.5, 0.5])?;
        let look = format!(r#"{{"steps": [{look}]}}"#);
        Scene::new(300, 150)?.add([Layer::fill("#204080")?, base.look(&look)?.mask(Mask::new(mask))?])
    }

    /// The layers a test masks: a fill, a picture that fills the canvas, and a picture turned and shrunk.
    const BASES: [&str; 3] = ["fill", "picture", "turned picture"];

    fn base(kind: &str) -> Result<Layer, Error> {
        let pixels = (0..40 * 30u16)
            .flat_map(|at| [f32::from(at % 7) / 7.0, f32::from(at % 11) / 11.0, 0.3, 1.0])
            .collect();
        let picture = Layer::picture(Raster::rgba(40, 30, pixels)?);
        match kind {
            "fill" => Layer::fill("#ff8000"),
            "picture" => picture.fit([0.0, 0.0, 300.0, 150.0], Fit::Cover, [0.5, 0.5]),
            _ => picture
                .fit([40.0, 20.0, 260.0, 130.0], Fit::Cover, [0.5, 0.5])?
                .place(Place::new().rotate(Deg(20.0)).scale(0.8)),
        }
    }

    #[test]
    fn a_looked_layer_masked_to_some_rows_is_drawn_for_those_rows_and_the_picture_is_the_same() -> Result<(), Error> {
        // a place by the picture, a blur, a blur summed by a Fourier transform, a grain, a statistic of the whole picture
        for look in [
            r#"{"effect": "vignette"}"#,
            r#"{"effect": "gaussian_blur", "params": {"radius": 8.0}}"#,
            r#"{"effect": "gaussian_blur", "params": {"radius": 300.0}}"#,
            r#"{"effect": "grain", "params": {"amount": 0.6, "size": 1.5}}"#,
            r#"{"effect": "auto_levels"}"#,
        ] {
            for kind in BASES {
                let before = WINDOWED.with(std::cell::Cell::get);
                let windowed = masked_rows(base(kind)?, look)?.render(3_000)?;
                assert!(
                    WINDOWED.with(std::cell::Cell::get) > before,
                    "{look} on a {kind} is drawn for the mask's rows"
                );
                WHOLE.with(|whole| whole.set(true));
                let whole = masked_rows(base(kind)?, look)?.render(3_000);
                WHOLE.with(|whole| whole.set(false));
                let whole = whole?;
                let same = windowed
                    .pixels()
                    .iter()
                    .zip(whole.pixels())
                    .all(|(left, right)| left.to_bits() == right.to_bits());
                assert!(
                    same,
                    "{look} on a {kind}: a windowed look makes the pixels a whole one makes"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn a_render_that_keeps_joins_draws_every_layer_whole() -> Result<(), Error> {
        let before = WINDOWED.with(std::cell::Cell::get);
        masked_rows(Layer::fill("#ff8000")?, r#"{"effect": "vignette"}"#)?.render(300)?;
        assert_eq!(
            WINDOWED.with(std::cell::Cell::get),
            before,
            "a draft keeps its cached rasters"
        );
        Ok(())
    }
}
