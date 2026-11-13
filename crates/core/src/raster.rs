//! The picture a scene takes in and gives out.

use crate::Error;

/// A picture of straight (not premultiplied) RGBA, 32-bit floats from 0 to 1, row after row from the top, pixel after
/// pixel from the left, four values a pixel. Colours are sRGB values.
#[derive(Clone, PartialEq)]
pub struct Raster {
    width: usize,
    height: usize,
    pixels: Vec<f32>,
}

impl std::fmt::Debug for Raster {
    /// The size, never the pixels: a printed picture can be gigabytes.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Raster {{ width: {}, height: {} }}", self.width, self.height)
    }
}

impl Raster {
    /// The picture of `width` by `height` pixels whose values are `pixels`: red, green, blue and alpha of each pixel.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `pixels` unless `width` and `height` are at least 1, there are exactly
    /// `width * height * 4` values, and every value is a number from 0 to 1.
    pub fn rgba(width: usize, height: usize, pixels: Vec<f32>) -> Result<Self, Error> {
        if width == 0
            || height == 0
            || width.checked_mul(height).and_then(|count| count.checked_mul(4)) != Some(pixels.len())
        {
            return Err(Error::invalid(
                "pixels",
                format!(
                    "{} values do not make {width} x {height} pixels of 4 channels",
                    pixels.len()
                ),
            ));
        }
        if let Some(at) = pixels.iter().position(|value| !(0.0..=1.0).contains(value)) {
            return Err(Error::invalid(
                "pixels",
                format!(
                    "every value is a number from 0 to 1, but row {}, column {} has {} in channel {}",
                    at / 4 / width,
                    at / 4 % width,
                    pixels[at],
                    at % 4
                ),
            ));
        }
        Ok(Self { width, height, pixels })
    }

    /// A picture the core made itself: `pixels` holds exactly `width * height * 4` values from 0 to 1.
    pub(crate) fn made(width: usize, height: usize, pixels: Vec<f32>) -> Self {
        Self { width, height, pixels }
    }

    /// The number of columns.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// The number of rows.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// The values: `width * height * 4` of them, red, green, blue and alpha of each pixel, row after row.
    #[must_use]
    pub fn pixels(&self) -> &[f32] {
        &self.pixels
    }

    /// The values, handed over.
    #[must_use]
    pub fn into_pixels(self) -> Vec<f32> {
        self.pixels
    }

    /// A transparent picture of `width` by `height` pixels.
    pub(crate) fn clear(width: usize, height: usize) -> Self {
        Self::made(width, height, vec![0.0; width * height * 4])
    }

    /// The `width` by `height` part of this picture whose top left pixel is at (`x`, `y`); transparent where the part
    /// reaches past the picture. The whole picture is handed back as it is, not copied.
    pub(crate) fn window(self, x: i64, y: i64, width: usize, height: usize) -> Self {
        if (x, y, width, height) == (0, 0, self.width, self.height) {
            return self;
        }
        let mut part = Self::clear(width, height);
        let (from_column, to_column, columns) = overlap(x, width, self.width);
        let (from_row, to_row, rows) = overlap(y, height, self.height);
        for row in 0..rows {
            let from = ((from_row + row) * self.width + from_column) * 4;
            let to = ((to_row + row) * width + to_column) * 4;
            part.pixels[to..to + columns * 4].copy_from_slice(&self.pixels[from..from + columns * 4]);
        }
        part
    }
}

/// Where `wanted` positions from `offset` meet a line of `have`: the first position in the line, the first in the part,
/// and how many.
fn overlap(offset: i64, wanted: usize, have: usize) -> (usize, usize, usize) {
    let wanted = i64::try_from(wanted).unwrap_or(i64::MAX);
    let have = i64::try_from(have).unwrap_or(i64::MAX);
    let (start, end) = (offset.max(0), offset.saturating_add(wanted).min(have));
    if end <= start {
        return (0, 0, 0);
    }
    let count = usize::try_from(end - start).unwrap_or_default();
    (
        usize::try_from(start).unwrap_or_default(),
        usize::try_from(start - offset).unwrap_or_default(),
        count,
    )
}

// Inline, because no public route reaches this: a scene asks every source for the whole canvas from its origin, so the
// offsets are for the bands and placements of later batches.
#[cfg(test)]
mod tests {
    use super::Raster;

    /// A window as `window` takes it: `x`, `y`, `width`, `height`.
    type Window = (i64, i64, usize, usize);

    /// A 3 by 2 picture whose pixel at column `c`, row `r` has red `10 r + c` (alpha 1), so every pixel can be told apart.
    fn numbered() -> Raster {
        let pixels = (0..2_u8)
            .flat_map(|row| (0..3_u8).flat_map(move |column| [f32::from(10 * row + column) / 100.0, 0.0, 0.0, 1.0]))
            .collect();
        Raster::made(3, 2, pixels)
    }

    /// The red values of `raster`, row after row; a clear pixel is `-1`.
    #[expect(clippy::cast_possible_truncation, reason = "values from 0 to 12")]
    fn reds(raster: &Raster) -> Vec<i32> {
        raster
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| {
                if pixel[3] == 0.0 {
                    -1
                } else {
                    (pixel[0] * 100.0).round() as i32
                }
            })
            .collect()
    }

    #[test]
    fn a_window_is_the_part_at_its_offset_and_clear_where_it_reaches_past_the_picture() {
        let cases: [(&str, Window, Vec<i32>); 10] = [
            ("the whole picture", (0, 0, 3, 2), vec![0, 1, 2, 10, 11, 12]),
            ("the top row", (0, 0, 3, 1), vec![0, 1, 2]),
            ("the left columns", (0, 0, 2, 2), vec![0, 1, 10, 11]),
            (
                "one row more than the picture",
                (0, 0, 3, 3),
                vec![0, 1, 2, 10, 11, 12, -1, -1, -1],
            ),
            ("a part inside", (1, 0, 2, 2), vec![1, 2, 11, 12]),
            ("past the right and bottom edges", (2, 1, 2, 2), vec![12, -1, -1, -1]),
            ("from before the top left", (-1, -1, 2, 2), vec![-1, -1, -1, 0]),
            (
                "larger than the picture",
                (0, 0, 4, 3),
                vec![0, 1, 2, -1, 10, 11, 12, -1, -1, -1, -1, -1],
            ),
            ("entirely outside", (5, 5, 2, 1), vec![-1, -1]),
            ("entirely before it", (-9, 0, 2, 1), vec![-1, -1]),
        ];
        for (what, (x, y, width, height), expected) in cases {
            let part = numbered().window(x, y, width, height);
            assert_eq!((part.width(), part.height()), (width, height), "{what}");
            assert_eq!(reds(&part), expected, "{what}");
        }
    }
}
