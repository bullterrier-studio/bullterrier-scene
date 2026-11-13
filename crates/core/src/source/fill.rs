//! A flat colour over the canvas.

use std::sync::Arc;

use super::{Drawn, Opened, Source};
use crate::geometry::Rect;
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
    /// One pixel: a flat colour is the same at any scale, so it is the one place that draws less than it was asked.
    fn open(&self, canvas: Rect) -> Result<Opened<'_>, Error> {
        Ok(Opened::new(canvas, move |_| {
            Ok(Drawn {
                raster: Arc::new(Raster::made(1, 1, self.rgba.to_vec())),
                covers: canvas,
            })
        }))
    }
}
