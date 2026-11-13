//! Design pixels and output pixels: what a layer is asked to draw, and how big that is.

/// A rectangle of the canvas, in design pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// How many output pixels one design pixel is: `size` over `long`, the canvas's long side, kept as the two whole numbers so
/// that a length of whole design pixels comes out exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scale {
    size: u32,
    long: u32,
}

impl Scale {
    /// The scale of a render at `size` (the long side in output pixels) of a canvas whose long side is `long` design pixels.
    pub(crate) fn new(size: u32, long: u32) -> Self {
        Self { size, long }
    }

    /// The output pixels that cover `design` design pixels, rounded up: a box that is not whole in output pixels is drawn
    /// at the next whole size.
    pub(crate) fn len(self, design: f64) -> usize {
        // multiplied before divided, so a whole result is exact
        let output = (design * f64::from(self.size) / f64::from(self.long)).ceil();
        // a length is never negative, and none reaches 2^53: the cast is exact
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a length of at most 2^27"
        )]
        {
            output.max(0.0) as usize
        }
    }

    /// The output pixel a position of `design` design pixels falls on, rounded down.
    pub(crate) fn at(self, design: f64) -> i64 {
        let output = (design * f64::from(self.size) / f64::from(self.long)).floor();
        #[expect(clippy::cast_possible_truncation, reason = "a position within the canvas")]
        {
            output as i64
        }
    }
}
