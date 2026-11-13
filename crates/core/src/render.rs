//! Drawing a scene: the size of the picture, each layer drawn at it, joined by normal blend in premultiplied floats, and
//! given back as straight alpha.

use crate::geometry::{MAX_PIXELS, Rect, Scale};
use crate::mask::product;
use crate::placement::{Layout, Placement};
use crate::source::Drawn;
use crate::warp::{Map, is_identity, warp_into};
use crate::{Error, Layer, Raster};

/// The longest side of a picture, in output pixels.
const MAX_SIZE: u32 = 65_000;

/// `layers` drawn on a canvas of `width` by `height` design pixels at `size` output pixels on the long side.
pub(crate) fn render(width: u32, height: u32, layers: &[Layer], size: i128) -> Result<Raster, Error> {
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
    let (columns, rows) = (output(width), output(height));
    if columns.saturating_mul(rows) > MAX_PIXELS {
        return Err(Error::invalid(
            "size",
            format!(
                "size {size} gives a picture of {columns} x {rows} pixels; at most {MAX_PIXELS} pixels are drawn (2^27)"
            ),
        ));
    }
    let target = Target {
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
    };
    // premultiplied red, green, blue and alpha of what is drawn so far
    let mut joined = vec![0.0_f32; columns * rows * 4];
    for (index, layer) in layers.iter().enumerate() {
        let name = layer.name.clone().unwrap_or_else(|| format!("layer{}", index + 1));
        lay(layer, &target, &mut joined).map_err(|error| error.in_layer(&name))?;
    }
    Ok(Raster::made(columns, rows, straight(joined)))
}

/// Where a layer is drawn: the output grid of the canvas, with `margin` more pixels on each side (along x and y) for a mask
/// whose blur reaches past the canvas.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Target {
    /// The scene's canvas, in design pixels.
    canvas: Rect,
    pub(crate) scale: Scale,
    /// The canvas's size in output pixels.
    columns: usize,
    rows: usize,
    margin: (usize, usize),
}

impl Target {
    /// The size in pixels, margin included: `(columns, rows)`.
    pub(crate) fn size(&self) -> (usize, usize) {
        (self.columns + 2 * self.margin.0, self.rows + 2 * self.margin.1)
    }

    /// This target with `reach` more pixels of margin on each side, along x and y.
    pub(crate) fn grown(&self, reach: (usize, usize)) -> Self {
        Self {
            margin: (self.margin.0 + reach.0, self.margin.1 + reach.1),
            ..*self
        }
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

/// A layer drawn at the scale it will have and ready to be put where it belongs: the raster, and the one warp that puts it on
/// the target.
pub(crate) struct Landed {
    drawn: Drawn,
    map: Map,
    clip: Rect,
    size: (usize, usize),
}

impl Landed {
    /// Whether the raster is the target as it is (an unmoved layer the size of the output).
    fn is_unmoved(&self) -> bool {
        matches!(self.map, Map::Affine(affine) if is_identity(&self.drawn.raster, affine, self.clip, self.size.0, self.size.1))
    }

    /// Gives `put(index, pixel)` each pixel (straight RGBA) the layer lands on, `index` being its place in the target, row
    /// after row. A pixel the layer does not reach is not given.
    pub(crate) fn each(&self, put: &mut impl FnMut(usize, [f32; 4])) {
        if self.is_unmoved() {
            for (at, pixel) in self.drawn.raster.pixels().as_chunks::<4>().0.iter().enumerate() {
                put(at, *pixel);
            }
        } else {
            warp_into(&self.drawn.raster, self.map, self.clip, self.size, put);
        }
    }
}

/// `layer` drawn at the scale it will have, and put on `target`: the source asked for the part of its box that shows, then the
/// map of the one warp worked out.
pub(crate) fn land(layer: &Layer, target: &Target) -> Result<Landed, Error> {
    // a source with no box of its own (a fill) takes the part of the canvas the target holds, margin included, so that a mask
    // that fills the canvas goes on into the margin; but only where nothing acts on the box, for `fit` and `place` work on
    // it and a layer must be the same shape whatever its blur adds
    let unplaced = layer.fit.is_none() && layer.placement == Placement::default();
    let opened = layer
        .source
        .open(if unplaced { target.rect() } else { target.canvas })?;
    let layout = Layout::new(layer.fit, layer.placement, opened.own, target.scale);
    let request = layout.request(opened.own, opened.whole);
    // the one place the drawn-size limit is checked, before any source draws
    request.scale.size(request.window)?;
    let drawn = opened.draw(request)?;
    let (map, clip) = layout.warp(&drawn);
    let map = if target.margin == (0, 0) {
        map
    } else {
        // the output starts `margin` pixels up and left of the canvas
        #[expect(clippy::cast_precision_loss, reason = "a margin of at most 2^27 pixels")]
        map.shifted(target.margin.0 as f64, target.margin.1 as f64)
    };
    Ok(Landed {
        drawn,
        map,
        clip,
        size: target.size(),
    })
}

/// `layer` laid over `joined`: its source drawn at the scale it will have, then put where it belongs by one warp, each pixel
/// of which is blended where it lands. A layer that is not moved is blended as it is. Where the layer has masks, each pixel
/// is blended at its mask's value too.
fn lay(layer: &Layer, target: &Target, joined: &mut [f32]) -> Result<(), Error> {
    let factor = product(layer, target)?;
    let landed = land(layer, target)?;
    let opacity = layer.opacity;
    if let Some(factor) = factor {
        landed.each(&mut |at, above| over_pixel(&mut joined[at * 4..at * 4 + 4], above, opacity * factor.values[at]));
    } else if landed.is_unmoved() {
        over(joined, landed.drawn.raster.pixels(), opacity);
    } else {
        warp_into(
            &landed.drawn.raster,
            landed.map,
            landed.clip,
            landed.size,
            &mut |at, above| {
                over_pixel(&mut joined[at * 4..at * 4 + 4], above, opacity);
            },
        );
    }
    Ok(())
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
fn straight(mut joined: Vec<f32>) -> Vec<f32> {
    for pixel in joined.as_chunks_mut::<4>().0 {
        let alpha = pixel[3];
        for channel in &mut pixel[..3] {
            // a premultiplied colour never passes its alpha (each step is monotone in both), so the quotient is at most 1
            *channel = if alpha > 0.0 { *channel / alpha } else { 0.0 };
        }
    }
    joined
}
