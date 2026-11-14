//! Drawing a scene: the size of the picture, each layer drawn at it, joined by its blend mode in premultiplied floats, and
//! given back as straight alpha.

use crate::blend::{Blend, join};
use crate::geometry::{MAX_PIXELS, Rect, Scale};
use crate::landing::{Target, land};
use crate::mask::product;
use crate::warp::warp_into;
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

/// `layer` laid over `joined`: its source drawn at the scale it will have, then put where it belongs by one warp, each pixel
/// of which is blended where it lands. A layer that is not moved is blended as it is. Where the layer has masks, each pixel
/// is blended at its mask's value too.
fn lay(layer: &Layer, target: &Target, joined: &mut [f32]) -> Result<(), Error> {
    let factor = product(layer, target)?;
    let landed = land(layer, target)?;
    let opacity = layer.opacity;
    let blend = layer.blend;
    if blend != Blend::Normal {
        // every pixel is joined where it lands, by the compositing equation of its mode
        landed.each(&mut |at, [red, green, blue, alpha]| {
            let weight = factor.as_ref().map_or(opacity, |factor| opacity * factor.values[at]);
            join(
                &mut joined[at * 4..at * 4 + 4],
                [red, green, blue],
                alpha * weight,
                blend,
            );
        });
    } else if let Some(factor) = factor {
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
