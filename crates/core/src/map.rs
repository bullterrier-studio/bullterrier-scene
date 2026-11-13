//! A map: one value from 0 to 1 for each pixel of a picture, the matte of a cut-out or the depth of a scene.

use crate::Error;

/// A picture-sized grid of values from 0 to 1, row after row from the top, pixel after pixel from the left: what a
/// picture layer holds beside its pixels (`Layer::map`), a look reads (`subject`, `depth`, a `given` region) and a
/// mask (`Layer::mask("subject")`) multiplies.
#[derive(Clone, PartialEq)]
pub struct Map {
    width: usize,
    height: usize,
    values: Vec<f32>,
}

impl std::fmt::Debug for Map {
    /// The size, never the values: a printed map can be gigabytes.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Map {{ width: {}, height: {} }}", self.width, self.height)
    }
}

impl Map {
    /// The map of `width` by `height` pixels whose values are `values`.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `values` unless `width` and `height` are at least 1, there are exactly `width * height`
    /// values, and every value is a number from 0 to 1.
    pub fn new(width: usize, height: usize, values: Vec<f32>) -> Result<Self, Error> {
        if width == 0 || height == 0 || width.checked_mul(height) != Some(values.len()) {
            return Err(Error::invalid(
                "values",
                format!("{} values do not make a map of {width} x {height} pixels", values.len()),
            ));
        }
        if let Some(at) = values.iter().position(|value| !(0.0..=1.0).contains(value)) {
            return Err(Error::invalid(
                "values",
                format!(
                    "every value is a number from 0 to 1, but row {}, column {} has {}",
                    at / width,
                    at % width,
                    values[at]
                ),
            ));
        }
        Ok(Self { width, height, values })
    }

    /// A map the core made itself: `values` holds exactly `width * height` values from 0 to 1.
    pub(crate) fn made(width: usize, height: usize, values: Vec<f32>) -> Self {
        Self { width, height, values }
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

    /// The values: `width * height` of them, row after row.
    #[must_use]
    pub fn values(&self) -> &[f32] {
        &self.values
    }
}
