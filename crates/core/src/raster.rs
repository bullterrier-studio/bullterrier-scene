//! The picture a scene takes in and gives out.

use crate::Error;

/// A picture of straight (not premultiplied) RGBA, 32-bit floats from 0 to 1, row after row from the top, pixel after
/// pixel from the left, four values a pixel. Colours are sRGB values.
#[derive(Debug, Clone, PartialEq)]
pub struct Raster {
    width: usize,
    height: usize,
    pixels: Vec<f32>,
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
}
