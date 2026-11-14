//! A flat colour over the canvas.

use std::sync::Arc;

use std::collections::BTreeMap;

use super::{Drawn, Opened, Source};
use crate::cache::{Cache, Keyed};
use crate::document::Form;
use crate::geometry::Rect;
use crate::parameters::{Parameter, Value};
use crate::{Error, Raster};

/// One straight RGBA colour, everywhere.
#[derive(Debug)]
pub(crate) struct Fill {
    rgba: [f32; 4],
    /// The colour as a parameter writes it: lower case.
    color: String,
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
        Ok(Self {
            rgba,
            color: color.to_ascii_lowercase(),
        })
    }
}

impl Source for Fill {
    fn document(&self, _form: Form) -> Result<Vec<(&'static str, serde_json::Value)>, Error> {
        Ok(vec![("fill", self.color.clone().into())])
    }

    fn parameters(&self) -> Vec<Parameter> {
        vec![Parameter::color("color", "the colour of the layer", &self.color)]
    }

    fn with_values(&self, values: &[(&str, &Value)]) -> Result<Arc<dyn Source>, Error> {
        let mut color = self.color.clone();
        for &(address, value) in values {
            if address == "color" {
                value.text(address)?.clone_into(&mut color);
            }
        }
        Ok(Arc::new(Self::parse(&color).map_err(|error| error.about("color"))?))
    }

    fn keyed(&self, key: &mut Keyed) -> Result<(), Error> {
        key.text("fill").debug(&self.rgba);
        Ok(())
    }

    /// One pixel: a flat colour is the same at any scale, so it is the one place that draws less than it was asked. A layer
    /// that runs a look on it is drawn at the size the look runs at, the window of the box that shows.
    fn open(&self, canvas: Rect, _cache: &Cache) -> Result<Opened<'_>, Error> {
        Ok(Opened::finishing(canvas, move |request, finish, _| {
            let (raster, covers) = if finish.is_looked() {
                let (columns, rows) = request.scale.size(request.window)?;
                (
                    Raster::made(columns, rows, self.rgba.repeat(columns * rows)),
                    request.window,
                )
            } else {
                (Raster::made(1, 1, self.rgba.to_vec()), canvas)
            };
            Ok(Drawn {
                raster: Arc::new(raster),
                covers,
                ends_at_window: false,
                room: [0; 4],
                maps: BTreeMap::new(),
            })
        })
        .own_pixels())
    }
}
