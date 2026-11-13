//! A picture as a layer, and the one place that knows how the image crate reads a picture file.

use std::path::PathBuf;

use super::Source;
use crate::geometry::{Rect, Scale};
use crate::resample::resample;
use crate::{Error, Raster};

/// A picture the layer holds, or a file it reads when it is drawn. Its box is its pixels, one design pixel each, at the
/// canvas origin.
#[derive(Debug)]
pub(crate) enum Picture {
    Held(Raster),
    File(PathBuf),
}

/// The picture in `path`: upright, in sRGB values, with its alpha if it has one.
fn read(path: &std::path::Path) -> Result<Raster, Error> {
    let refuse = |message: String| Error::Read {
        path: path.display().to_string(),
        message,
    };
    let image = bullterrier_image::read(path).map_err(|error| refuse(error.to_string()))?;
    let (width, height, channels) = (image.width(), image.height(), image.channels());
    let values = image.into_values();
    let pixels = match channels {
        4 => values,
        _ => values
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[red, green, blue]| [red, green, blue, 1.0])
            .collect(),
    };
    Ok(Raster::made(width, height, pixels))
}

impl Source for Picture {
    fn draw(&self, rect: Rect, scale: Scale) -> Result<Raster, Error> {
        let read_now;
        let own = match self {
            Self::Held(raster) => raster,
            Self::File(path) => {
                read_now = read(path)?;
                &read_now
            }
        };
        // `width` and `height` are pixel counts of a picture that fits in memory: whole design pixels
        #[expect(clippy::cast_precision_loss, reason = "a pixel count below 2^53")]
        let (design_width, design_height) = (own.width() as f64, own.height() as f64);
        let placed = resample(own, scale.len(design_width), scale.len(design_height));
        Ok(placed.window(
            scale.at(rect.x),
            scale.at(rect.y),
            scale.len(rect.width),
            scale.len(rect.height),
        ))
    }
}
