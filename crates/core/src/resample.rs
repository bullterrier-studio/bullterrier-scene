//! Resampling a picture to another size: bilinear interpolation of premultiplied colour.

use crate::Raster;

/// Two neighbouring source positions and how far from the first the sample lies.
struct Tap {
    first: usize,
    second: usize,
    toward_second: f32,
}

/// The taps of each of `to` output positions on a line of `from` source positions. The whole source is stretched onto the
/// whole output: output position `i` is the centre of output pixel `i`, which lies at `(i + 0.5) * from / to - 0.5` among
/// the source pixel centres, and a position outside the source takes its edge.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "positions of at most 2^27 pixels, floored and clamped into the line"
)]
fn taps(from: usize, to: usize) -> Vec<Tap> {
    let last = from - 1;
    (0..to)
        .map(|index| {
            let at = ((index as f64 + 0.5) * from as f64 / to as f64 - 0.5).clamp(0.0, last as f64);
            let first = at.floor() as usize;
            Tap {
                first,
                second: (first + 1).min(last),
                toward_second: (at - first as f64) as f32,
            }
        })
        .collect()
}

/// `from` as a picture of `width` by `height` pixels (both at least 1). Straight RGBA in and out; the colour is mixed
/// premultiplied, so a transparent pixel adds none. The same size gives the picture itself.
pub(crate) fn resample(from: &Raster, width: usize, height: usize) -> Raster {
    if (from.width(), from.height()) == (width, height) {
        return from.clone();
    }
    let (columns, rows) = (taps(from.width(), width), taps(from.height(), height));
    let source = from.pixels();
    let at = |row: usize, column: usize| {
        let pixel = &source[(row * from.width() + column) * 4..][..4];
        [pixel[0] * pixel[3], pixel[1] * pixel[3], pixel[2] * pixel[3], pixel[3]]
    };
    let mut pixels = Vec::with_capacity(width * height * 4);
    for row in &rows {
        for column in &columns {
            let mut mixed = [0.0_f32; 4];
            for (line, weight_row) in [(row.first, 1.0 - row.toward_second), (row.second, row.toward_second)] {
                for (place, weight_column) in [
                    (column.first, 1.0 - column.toward_second),
                    (column.second, column.toward_second),
                ] {
                    let weight = weight_row * weight_column;
                    for (sum, value) in mixed.iter_mut().zip(at(line, place)) {
                        *sum += weight * value;
                    }
                }
            }
            let alpha = mixed[3];
            // a premultiplied colour never passes its alpha, so the quotient is at most 1; the weights of four taps can
            // sum to a step over 1, and so can the alpha
            let colour = |value: f32| if alpha > 0.0 { value / alpha } else { 0.0 };
            pixels.extend([colour(mixed[0]), colour(mixed[1]), colour(mixed[2]), alpha.min(1.0)]);
        }
    }
    Raster::made(width, height, pixels)
}
