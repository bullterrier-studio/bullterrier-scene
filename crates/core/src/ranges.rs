//! The ranges of the numbers of a placement, written once: `place`, `fit` and `perspective` check against them and their errors
//! print them.

use crate::Error;

/// The values a number of a placement may take: the one place a range is written. `Layer::fit` and `Layer::place` check
/// against these and their errors name them; C6 serves `parameters()` from them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Range {
    low: f64,
    high: f64,
    unit: &'static str,
}

/// `scale` and `aspect`: a ratio.
pub(crate) const RATIO: Range = Range {
    low: 0.001,
    high: 1000.0,
    unit: "",
};
/// `skew_x` and `skew_y`: degrees, short of the 90 where a skew has no end.
pub(crate) const SKEW: Range = Range {
    low: -89.0,
    high: 89.0,
    unit: " degrees",
};
/// `dx` and `dy`: design pixels.
pub(crate) const TRAVEL: Range = Range {
    low: -1e6,
    high: 1e6,
    unit: " design pixels",
};
/// `feather`: the standard deviation of a mask's blur, in design pixels; as far as a layer may move. What the render can draw
/// of it is bounded again by the pixels of a plane (`feather::fits`).
pub(crate) const FEATHER: Range = Range {
    low: 0.0,
    high: 1e6,
    unit: " design pixels",
};
/// `anchor` and `align`: a fraction of a box.
pub(crate) const FRACTION: Range = Range {
    low: 0.0,
    high: 1.0,
    unit: "",
};

/// `value` as a message shows it: as it is, or in scientific form where its decimal expansion would be hundreds of characters
/// (1e300, -1e-300).
fn shown(value: f64) -> String {
    if value != 0.0 && !(1e-6..1e15).contains(&value.abs()) {
        format!("{value:e}")
    } else {
        value.to_string()
    }
}

impl Range {
    /// `value` if it is a number within the range.
    pub(crate) fn check(self, field: &str, value: f64) -> Result<f64, Error> {
        if (self.low..=self.high).contains(&value) {
            Ok(value)
        } else {
            Err(Error::invalid(
                field,
                format!(
                    "a number from {} to {}{} is wanted, got {}",
                    self.low,
                    self.high,
                    self.unit,
                    shown(value)
                ),
            ))
        }
    }

    /// The two numbers `(x, y)` if each is within the range.
    pub(crate) fn check_pair(self, field: &str, (x, y): (f64, f64)) -> Result<(f64, f64), Error> {
        if (self.low..=self.high).contains(&x) && (self.low..=self.high).contains(&y) {
            Ok((x, y))
        } else {
            Err(Error::invalid(
                field,
                format!(
                    "two numbers from {} to {}{} are wanted, got ({}, {})",
                    self.low,
                    self.high,
                    self.unit,
                    shown(x),
                    shown(y)
                ),
            ))
        }
    }
}
