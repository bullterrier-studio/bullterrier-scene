//! A flat colour over the canvas.

use super::Source;
use crate::geometry::{Rect, Scale};
use crate::{Error, Raster};

/// One straight RGBA colour, everywhere.
#[derive(Debug)]
pub(crate) struct Fill {
    rgba: [f32; 4],
}

impl Fill {
    /// The fill of `color`: `#rrggbb` or `#rrggbbaa`, upper or lower case.
    pub(crate) fn parse(color: &str) -> Result<Self, Error> {
        let refuse = || {
            Error::invalid(
                "fill",
                format!("a colour is `#rrggbb` or `#rrggbbaa` in hexadecimal, got {color:?}"),
            )
        };
        let digits = color
            .strip_prefix('#')
            .filter(|digits| digits.is_ascii() && matches!(digits.len(), 6 | 8));
        let digits = digits.filter(|digits| digits.bytes().all(|byte| byte.is_ascii_hexdigit()));
        let digits = digits.ok_or_else(refuse)?;
        let mut rgba = [1.0; 4];
        for (value, pair) in rgba.iter_mut().zip(digits.as_bytes().chunks(2)) {
            let pair = std::str::from_utf8(pair).map_err(|_| refuse())?;
            *value = f32::from(u8::from_str_radix(pair, 16).map_err(|_| refuse())?) / 255.0;
        }
        Ok(Self { rgba })
    }
}

impl Source for Fill {
    fn draw(&self, rect: Rect, scale: Scale) -> Result<Raster, Error> {
        let (width, height) = (scale.len(rect.width), scale.len(rect.height));
        Ok(Raster::made(width, height, self.rgba.repeat(width * height)))
    }
}
