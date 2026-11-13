//! A picture as a layer, with the looks run on it: the one place that knows how the image crate reads a picture file, a
//! recipe, and renders one.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bullterrier_image::{Options, Photo, Presets, Recipe};
use serde_json::Value;

use super::{Drawn, Request, Source};
use crate::geometry::Rect;
use crate::warp::{Affine, warp};
use crate::{Error, Raster};

/// The formats of a recipe the linked image crate reads. A recipe document written by a recipe file carries one; image
/// reads a recipe without it, and refuses a top-level `format`, so a recipe given here has the key checked against this list
/// and removed.
///
/// shortcut: image keeps its list private (`recipe/file.rs`), so this copy can drift at a bump of the pin; the
/// `data_boundary` test reads formats 0 to 4 through image's public `Recipe::from_toml` and fails when they differ. Lift
/// it when image publishes the list (api.md, "Needs from others").
const READ_FORMATS: [i64; 3] = [1, 2, 3];

/// A picture the layer holds, or a file it reads when it is drawn. Its box is its pixels, one design pixel each, at the
/// canvas origin.
#[derive(Debug, Clone)]
enum Pixels {
    Held(Arc<Raster>),
    File(PathBuf),
}

/// A picture, and the looks run on it once it is resampled to the output size, in the order they were given.
#[derive(Debug, Clone)]
pub(crate) struct Picture {
    pixels: Pixels,
    looks: Vec<Look>,
}

/// A recipe that image has checked, with the presets it was checked against.
///
/// In C1 a look's `px` values are image's own: a pixel of a picture whose long side is 480, scaled to the layer's output
/// picture. Batch C5 makes them design pixels of the canvas.
#[derive(Debug, Clone)]
struct Look {
    recipe: Recipe,
    presets: Arc<Presets>,
}

/// `values` brought into 0 to 1: image promises its pictures only "nominally 0..1", and a look can overshoot by a rounding
/// step (a scanlines look gives -1e-15 on a checker). This is where the promise of an outside crate, with its tolerance,
/// enters scene, whose pictures are exactly 0 to 1. A value that is not a number cannot be a colour: `false` for it, and the
/// caller refuses.
fn into_range(values: &mut [f32]) -> bool {
    values.iter_mut().all(|value| {
        *value = value.clamp(0.0, 1.0);
        !value.is_nan()
    })
}

/// `error` of the image crate as a look's: scene's own refusal naming `look`.
fn refused(error: &bullterrier_image::Error) -> Error {
    Error::invalid("look", error.to_string())
}

impl Look {
    /// The look the recipe document `document` is: an object image reads, with a `format` it reads or none.
    fn new(document: &Value) -> Result<Self, Error> {
        let Value::Object(members) = document else {
            return Err(Error::Format {
                pointer: "/look".to_owned(),
                message: format!("a look is a JSON object, got {document}"),
            });
        };
        let mut members = members.clone();
        if let Some(found) = members.remove("format")
            && !found.as_i64().is_some_and(|number| READ_FORMATS.contains(&number))
        {
            let read: Vec<String> = READ_FORMATS.iter().map(ToString::to_string).collect();
            return Err(Error::Format {
                pointer: "/look/format".to_owned(),
                message: format!(
                    "image recipe format {found}; this scene reads image formats {}",
                    read.join(", ")
                ),
            });
        }
        let recipe = Recipe::from_json(&Value::Object(members).to_string()).map_err(|error| refused(&error))?;
        let presets = Presets::shipped().map_err(|error| refused(&error))?;
        let recipe = recipe
            .checked_from("recipe", &presets)
            .map_err(|error| refused(&error))?;
        if recipe.size.is_some() {
            return Err(Error::invalid(
                "look",
                "a look does not set `size`: scene sets the size of a layer's picture",
            ));
        }
        if !recipe.frame.is_empty() {
            return Err(Error::invalid(
                "look",
                "a look does not set `frame`: scene sets the box of a layer's picture",
            ));
        }
        Ok(Self {
            recipe,
            presets: Arc::new(presets),
        })
    }

    /// `layer` with the look run on its colour, straight; its alpha stays.
    fn apply(&self, layer: &Raster) -> Result<Raster, Error> {
        let colour: Vec<f32> = layer
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[red, green, blue, _]| [red, green, blue])
            .collect();
        let picture =
            bullterrier_image::Picture::new(layer.height(), layer.width(), &colour).map_err(|error| refused(&error))?;
        let image = Photo::new(picture)
            .render(&self.recipe, &self.presets, Options::default())
            .map_err(|error| refused(&error))?;
        let mut graded = image.into_values();
        if !(graded.iter().all(|value| value.is_finite()) && into_range(&mut graded)) {
            return Err(Error::invalid(
                "look",
                "image gave a value that is not a number: the picture is refused",
            ));
        }
        let pixels = graded
            .as_chunks::<3>()
            .0
            .iter()
            .zip(layer.pixels().as_chunks::<4>().0)
            .flat_map(|(&[red, green, blue], pixel)| [red, green, blue, pixel[3]])
            .collect();
        Ok(Raster::made(layer.width(), layer.height(), pixels))
    }
}

impl Picture {
    /// A picture held in memory.
    pub(crate) fn held(raster: Raster) -> Self {
        Self {
            pixels: Pixels::Held(Arc::new(raster)),
            looks: Vec::new(),
        }
    }

    /// A picture read from `path` when it is drawn.
    pub(crate) fn file(path: PathBuf) -> Self {
        Self {
            pixels: Pixels::File(path),
            looks: Vec::new(),
        }
    }
}

/// The recipe in the recipe file `path`, as the JSON document `Layer::look` takes, read now.
///
/// # Errors
/// [`Error::Read`] if the file cannot be read, [`Error::Invalid`] naming `look` if it holds no recipe.
pub(crate) fn recipe_file(path: &Path) -> Result<Value, Error> {
    std::fs::metadata(path).map_err(|error| Error::Read {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    let recipe = Recipe::from_file(path).map_err(|error| refused(&error))?;
    serde_json::to_value(&recipe).map_err(|error| Error::invalid("look", error.to_string()))
}

/// The picture in `path`: upright, in sRGB values, with its alpha if it has one.
fn read(path: &Path) -> Result<Raster, Error> {
    let image = bullterrier_image::read(path).map_err(|error| Error::Read {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    let (width, height, channels) = (image.width(), image.height(), image.channels());
    let mut values = image.into_values();
    if !(values.iter().all(|value| value.is_finite()) && into_range(&mut values)) {
        return Err(Error::Read {
            path: path.display().to_string(),
            message: "the picture holds a value that is not a number".to_owned(),
        });
    }
    let pixels = if channels == 4 {
        values
    } else {
        values
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[red, green, blue]| [red, green, blue, 1.0])
            .collect()
    };
    Ok(Raster::made(width, height, pixels))
}

impl Source for Picture {
    /// Without a look, the picture's own pixels, for the warp to resample once; with one, the window at the asked scale (a
    /// part of the picture, or all of it), so that the look runs on the raster the layer will have.
    fn draw(&self, _canvas: Rect, plan: &dyn Fn(Rect) -> Result<Request, Error>) -> Result<Drawn, Error> {
        let read_now;
        let own = match &self.pixels {
            Pixels::Held(raster) => Arc::clone(raster),
            Pixels::File(path) => {
                read_now = Arc::new(read(path)?);
                Arc::clone(&read_now)
            }
        };
        // a pixel count below 2^53: whole design pixels
        #[expect(clippy::cast_precision_loss, reason = "a pixel count below 2^53")]
        let whole = Rect {
            x: 0.0,
            y: 0.0,
            width: own.width() as f64,
            height: own.height() as f64,
        };
        let request = plan(whole)?;
        if self.looks.is_empty() {
            return Ok(Drawn {
                raster: own,
                own: whole,
                covers: whole,
            });
        }
        let window = request.window;
        let (columns, rows) = request.scale.size(window)?;
        // a design pixel is a pixel of the picture; the window's pixels are `columns` by `rows`
        #[expect(clippy::cast_precision_loss, reason = "a side of at most 2^27 pixels")]
        let (to_columns, to_rows) = (columns as f64 / window.width, rows as f64 / window.height);
        let map = Affine {
            a: to_columns,
            b: 0.0,
            c: -window.x * to_columns,
            d: 0.0,
            e: to_rows,
            f: -window.y * to_rows,
        };
        let mut placed = warp(&own, map, window, columns, rows);
        for look in &self.looks {
            placed = look.apply(&placed)?;
        }
        Ok(Drawn {
            raster: Arc::new(placed),
            own: whole,
            covers: window,
        })
    }

    fn looked(&self, recipe: &Value) -> Result<Arc<dyn Source>, Error> {
        let mut looks = self.looks.clone();
        looks.push(Look::new(recipe)?);
        Ok(Arc::new(Self {
            pixels: self.pixels.clone(),
            looks,
        }))
    }
}

// Inline, because no public route reaches this: image gives no NaN on demand, and the overshoot it does give is tested through a
// look.
#[cfg(test)]
mod tests {
    use super::into_range;

    #[test]
    fn values_are_brought_into_range_and_a_nan_is_refused() {
        let mut values = [-3.1e-14, 0.0, 0.25, 1.0, 1.0 + 1e-7];
        assert!(into_range(&mut values));
        assert_eq!(
            values.map(f32::to_bits),
            [0.0_f32, 0.0, 0.25, 1.0, 1.0].map(f32::to_bits)
        );
        assert!(!into_range(&mut [0.5, f32::NAN]));
    }
}
