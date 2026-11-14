//! The picture a scene takes in and gives out.

use std::path::Path;
use std::sync::OnceLock;

use crate::Error;
use crate::files::{Fingerprint, digest_floats};
use crate::source::picture;

/// A picture of straight (not premultiplied) RGBA, 32-bit floats from 0 to 1, row after row from the top, pixel after
/// pixel from the left, four values a pixel. Colours are sRGB values.
#[derive(Clone)]
pub struct Raster {
    width: usize,
    height: usize,
    pixels: Vec<f32>,
    /// The digest of the pixels, made when first asked for (a picture is hashed once, however many scenes hold it).
    digest: OnceLock<Fingerprint>,
}

impl PartialEq for Raster {
    fn eq(&self, other: &Self) -> bool {
        (self.width, self.height) == (other.width, other.height) && self.pixels == other.pixels
    }
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
        Ok(Self::made(width, height, pixels))
    }

    /// A picture the core made itself: `pixels` holds exactly `width * height * 4` values from 0 to 1.
    pub(crate) fn made(width: usize, height: usize, pixels: Vec<f32>) -> Self {
        Self {
            width,
            height,
            pixels,
            digest: OnceLock::new(),
        }
    }

    /// The SHA-256 of the pixels: made the first time it is asked for, then kept with the picture.
    pub(crate) fn digest(&self) -> &Fingerprint {
        self.digest.get_or_init(|| digest_floats(&self.pixels))
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

    /// Writes the picture to the file `path`, in the format its extension names: `.png` gives an 8-bit RGBA PNG (straight
    /// alpha, sRGB values, image's sRGB profile, no other metadata), `.webp` a lossless WebP; a `.jpg` is refused when it
    /// would drop an alpha channel, as image's writer refuses it. Each level is written as image's writer rounds it,
    /// `level * 255 + 0.5` cut to a whole number (0.5 gives 128). The same picture gives the same bytes. A file already
    /// there is replaced; none is made for a refusal.
    ///
    /// # Errors
    /// [`Error::Write`] naming `path` for an extension that is none of those, a folder that is not there, a path that cannot
    /// be written and a picture too large for the format.
    pub fn write(&self, path: &Path) -> Result<(), Error> {
        picture::write(self, path)
    }

    /// The values, handed over.
    #[must_use]
    pub fn into_pixels(self) -> Vec<f32> {
        self.pixels
    }
}
