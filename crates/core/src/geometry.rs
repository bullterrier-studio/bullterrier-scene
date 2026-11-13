//! Design pixels and output pixels: the one rule for where a rectangle of the canvas falls on the output, and how many
//! pixels a source draws for it.
//!
//! **The rule.** The output picture has `ceil(canvas x size / long side)` pixels per axis and covers the canvas exactly, so
//! a design length is `columns / width` output pixels along x and `rows / height` along y: the two differ by less than a
//! pixel over the canvas and are equal wherever the canvas is whole at that size. A rectangle of design pixels drawn at a
//! scale (output pixels per design pixel, per axis) is `ceil(width x scale)` by `ceil(height x scale)` pixels that divide
//! it evenly. The warp puts that raster where its true rectangle falls, so a rectangle that ends between two output pixels
//! ends there, with the pixel it ends in covered in part.

use crate::Error;

/// The most pixels a picture has, the render and every layer drawn for it: 2^27 (a float32 RGBA picture of that size is 2 GiB).
pub(crate) const MAX_PIXELS: usize = 1 << 27;

/// A rectangle, in design pixels or in the pixels of a raster, as its context says.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// Output pixels per design pixel along one axis, kept as the two numbers `pixels` and `design` (the length is
/// `length x pixels / design`, multiplied before it is divided), so that a length of whole design pixels that makes whole
/// output pixels comes out exact.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Axis {
    pixels: f64,
    design: f64,
}

impl Axis {
    /// The output pixels `design` design pixels cover, not rounded.
    pub(crate) fn pixels(self, design: f64) -> f64 {
        design * self.pixels / self.design
    }

    /// This scale times `factor`: a source drawn at a part (a fit, a zoom) of the output scale.
    fn times(self, factor: f64) -> Self {
        Self {
            pixels: self.pixels * factor,
            design: self.design,
        }
    }

    /// The whole pixels that divide `design` design pixels at this scale.
    fn count(self, design: f64) -> usize {
        whole_up(self.pixels(design))
    }
}

/// The least whole number of pixels that holds `length` pixels. A length that is whole in exact arithmetic but a few
/// units in the last place over it in a float (a product of a fit factor and a zoom) is not rounded up a pixel: the
/// product is allowed a relative error of 1e-12, about 4,500 times the rounding of a product of two numbers.
pub(crate) fn whole_up(length: f64) -> usize {
    // a length is never negative and none reaches 2^53 (the callers bound it): the cast is exact
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a length of at most 2^27 pixels"
    )]
    {
        (length * (1.0 - 1e-12)).ceil().max(0.0) as usize
    }
}

/// How many output pixels a design pixel is, along each axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Scale {
    pub(crate) x: Axis,
    pub(crate) y: Axis,
}

impl Scale {
    /// The scale of an output of `columns` by `rows` pixels that covers a canvas of `width` by `height` design pixels.
    pub(crate) fn of_canvas(columns: usize, rows: usize, width: u32, height: u32) -> Self {
        // a side of at most 2^27 pixels: exact in a float
        #[expect(clippy::cast_precision_loss, reason = "a side of at most 2^27 pixels")]
        let (columns, rows) = (columns as f64, rows as f64);
        Self {
            x: Axis {
                pixels: columns,
                design: f64::from(width),
            },
            y: Axis {
                pixels: rows,
                design: f64::from(height),
            },
        }
    }

    /// This scale with x multiplied by `x` and y by `y`: the scale a source is asked to draw at once it is fitted and zoomed.
    pub(crate) fn times(self, x: f64, y: f64) -> Self {
        Self {
            x: self.x.times(x),
            y: self.y.times(y),
        }
    }

    /// The size in pixels, as `(columns, rows)`, of `rect` drawn at this scale: `ceil(width x scale)` by `ceil(height x
    /// scale)`.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `size` if it has more than [`MAX_PIXELS`] pixels, so that a layer large on a small canvas is
    /// refused before anything of its size is allocated.
    pub(crate) fn size(self, rect: Rect) -> Result<(usize, usize), Error> {
        let (columns, rows) = (self.x.count(rect.width), self.y.count(rect.height));
        if columns.saturating_mul(rows) > MAX_PIXELS {
            return Err(Error::invalid(
                "size",
                format!(
                    "the layer is drawn {columns} x {rows} pixels; at most {MAX_PIXELS} pixels (2^27) are drawn: lower `size` or make the layer smaller"
                ),
            ));
        }
        Ok((columns, rows))
    }
}

// Inline, because no public route reaches this: a size of a part is asked of the sources by the renderer only, and a
// scale of a canvas with a rounded side (a whole canvas is what the public tests render) is tested by its sizes.
#[cfg(test)]
mod tests {
    use super::{Rect, Scale};

    fn rect(width: f64, height: f64) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        }
    }

    #[test]
    fn a_rectangle_is_whole_pixels_rounded_up_along_each_axis() {
        // a canvas of 3 by 1 design pixels at 5 by 2 output pixels: 5/3 along x, 2 along y
        let scale = Scale::of_canvas(5, 2, 3, 1);
        assert_eq!(scale.size(rect(3.0, 1.0)).ok(), Some((5, 2)));
        assert_eq!(scale.size(rect(1.0, 0.5)).ok(), Some((2, 1)));
        assert_eq!(scale.size(rect(0.0, 0.0)).ok(), Some((0, 0)));
    }

    #[test]
    fn a_factor_changes_the_scale_and_a_whole_factor_keeps_a_whole_size_whole() {
        let scale = Scale::of_canvas(8, 6, 8, 6).times(2.0, 0.5);
        assert_eq!(scale.size(rect(8.0, 6.0)).ok(), Some((16, 3)));
    }

    #[test]
    fn a_layer_may_have_2_pow_27_pixels_and_not_one_more_row() {
        // 4 by 2 design pixels at 16,384 output pixels on a canvas whose long side is 4: 16,384 by 8,192
        let scale = Scale::of_canvas(16_384, 8_192, 4, 2);
        assert_eq!(scale.size(rect(4.0, 2.0)).ok(), Some((16_384, 8_192)));
        assert!(scale.size(rect(4.0, 2.000_5)).is_err());
    }

    #[test]
    fn a_length_of_whole_design_pixels_that_makes_whole_output_pixels_is_exact() {
        // 1280 over 1280 at 640: halves exactly, never 641
        let scale = Scale::of_canvas(640, 360, 1280, 720);
        assert_eq!(scale.size(rect(1280.0, 720.0)).ok(), Some((640, 360)));
        assert_eq!(scale.size(rect(1.0, 1.0)).ok(), Some((1, 1)));
        // and 3 design pixels at 5/3 output pixels each is 5, not 6
        assert_eq!(
            Scale::of_canvas(5, 2, 3, 1)
                .size(rect(3.0, 1.0))
                .ok()
                .map(|size| size.0),
            Some(5)
        );
    }
}
