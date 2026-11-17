//! The picture a scene takes in and gives out.

use std::path::Path;
use std::sync::OnceLock;

use crate::Error;
use crate::files::{Fingerprint, digest_floats};
use crate::pdf::{self, PdfOptions};
use crate::ranges::FILE_DPI;
use crate::source::picture;

/// `level` (0 to 1) as an 8-bit number, rounded as image's writer rounds a PNG: `level * 255 + 0.5` cut to a whole number.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a level is 0..1, so the number is 0..255"
)]
pub(crate) fn eight_bit(level: f32) -> u8 {
    (level * 255.0 + 0.5) as u8
}

/// What `Raster::write_with` and `Scene::render_to_png_with` are asked for beyond the picture: the resolution the file
/// says it prints at. Built by `new` and the setter, checked where it is given.
///
/// ```text
/// WriteOptions::new().dpi(300.0)?
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WriteOptions {
    pub(crate) dpi: Option<f64>,
}

impl WriteOptions {
    /// No resolution: the file says nothing of how large it prints.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The file says it prints at `dpi` dots per inch (a PNG's `pHYs` chunk, a JPEG's JFIF density). No pixel changes; a
    /// WebP holds none and is refused. A PDF takes its own, [`PdfOptions::new`].
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `dpi` unless it is a number from 1 to 65,535.
    pub fn dpi(mut self, dpi: f64) -> Result<Self, Error> {
        self.dpi = Some(FILE_DPI.check("dpi", dpi)?);
        Ok(self)
    }
}

/// Pixels per metre of a resolution, as image's writer rounds it (`round(dpi / 0.0254)`): 300 dpi is 11,811.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a checked dpi of 1 to 65,535 is 39 to 2,580,000 pixels a metre"
)]
pub(crate) fn pixels_per_metre(dpi: f64) -> u32 {
    (dpi / 0.0254).round() as u32
}

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
    /// alpha, sRGB values, image's sRGB profile, no other metadata), `.webp` a lossless WebP, and `.jpg` an 8-bit JPEG of a picture with no transparency (a
    /// picture with any alpha below 1 is refused for a `.jpg`, as image's writer refuses an alpha channel). Each level is written as image's writer rounds it,
    /// `level * 255 + 0.5` cut to a whole number (0.5 gives 128). The same picture gives the same bytes. A file already
    /// there is replaced; none is made for a refusal.
    ///
    /// # Errors
    /// [`Error::Write`] naming `path` for an extension that is none of those, a folder that is not there, a path that cannot
    /// be written and a picture too large for the format.
    pub fn write(&self, path: &Path) -> Result<(), Error> {
        self.write_with(path, &WriteOptions::new())
    }

    /// [`write`](Self::write) with the `options`: the resolution a PNG or a JPEG says it prints at. A file with no `dpi` is
    /// the file `write` makes, byte for byte.
    ///
    /// # Errors
    /// As [`write`](Self::write); [`Error::Write`] naming `path` for a `.webp`, which holds no resolution, given a `dpi`.
    pub fn write_with(&self, path: &Path, options: &WriteOptions) -> Result<(), Error> {
        picture::write(self, path, options.dpi)
    }

    /// Writes the picture to the file `path` as a one-page PDF at a physical size: the picture is one lossless (Flate) image
    /// on a page of `width / dpi` by `height / dpi` inches, with the bleed, crop marks, flattening and CMYK profile the
    /// `options` ask for. The same picture and options give the same bytes. A file already there is replaced; none is made
    /// for a refusal. The picture stays raster: nothing is kept as vectors.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `dpi` for a page past 14,400 pt (the limit of PDF readers), `bleed_mm` or `raster` for an
    /// image (the picture and its bleed) of more than 2^27 pixels, `cmyk_profile` for a profile that is not CMYK;
    /// [`Error::Read`] for a profile file that cannot be read; [`Error::Write`] naming `path` for a path that cannot be written.
    pub fn write_pdf(&self, path: &Path, options: &PdfOptions) -> Result<(), Error> {
        pdf::write(self, path, options)
    }

    /// The values, handed over.
    #[must_use]
    pub fn into_pixels(self) -> Vec<f32> {
        self.pixels
    }
}
