//! Drawing a scene: the size of the picture, each layer drawn at it, joined by normal blend in premultiplied floats, and
//! given back as straight alpha.

use crate::geometry::{MAX_PIXELS, Rect, Scale};
use crate::{Error, Layer, Raster};

/// The longest side of a picture, in output pixels.
const MAX_SIZE: u32 = 65_000;

/// `layers` drawn on a canvas of `width` by `height` design pixels at `size` output pixels on the long side.
pub(crate) fn render(width: u32, height: u32, layers: &[Layer], size: u32) -> Result<Raster, Error> {
    if !(1..=MAX_SIZE).contains(&size) {
        return Err(Error::invalid(
            "size",
            format!("size is a whole number from 1 to {MAX_SIZE}, got {size}"),
        ));
    }
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
    let scale = Scale::new(size, long);
    let canvas = Rect {
        x: 0.0,
        y: 0.0,
        width: f64::from(width),
        height: f64::from(height),
    };
    // premultiplied red, green, blue and alpha of what is drawn so far
    let mut joined = vec![0.0_f32; columns * rows * 4];
    for (index, layer) in layers.iter().enumerate() {
        let name = layer.name.clone().unwrap_or_else(|| format!("layer{}", index + 1));
        let drawn = layer
            .source
            .draw(canvas, scale)
            .map_err(|error| error.in_layer(&name))?;
        over(&mut joined, drawn.pixels(), layer.opacity);
    }
    Ok(Raster::made(columns, rows, straight(joined)))
}

/// `above` (straight RGBA, at `opacity` of its alpha) laid over `joined` (premultiplied): the "over" of Porter and Duff,
/// `above + joined * (1 - alpha of above)`.
fn over(joined: &mut [f32], above: &[f32], opacity: f32) {
    let (above, _) = above.as_chunks::<4>();
    let (joined, _) = joined.as_chunks_mut::<4>();
    for (below, above) in joined.iter_mut().zip(above) {
        let alpha = above[3] * opacity;
        for channel in 0..3 {
            below[channel] = above[channel] * alpha + below[channel] * (1.0 - alpha);
        }
        below[3] = alpha + below[3] * (1.0 - alpha);
    }
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
