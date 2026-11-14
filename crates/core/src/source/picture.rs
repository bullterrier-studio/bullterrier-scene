//! The image adapter: a picture as a layer, and a look, an image recipe run on a raster. The one place that knows how the
//! image crate reads a picture file, a recipe, and renders one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bullterrier_image::{Options, Photo, Presets, Recipe};
use serde_json::Value;

use super::{Drawn, Given, Opened, Source};
use crate::geometry::Rect;
use crate::warp::resize;
use crate::{Error, Map, Raster};

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

/// A picture and the maps it was given by name.
#[derive(Debug, Clone)]
pub(crate) struct Picture {
    pixels: Pixels,
    maps: BTreeMap<String, Given>,
}

/// A recipe that image has checked, with the presets it was checked against.
///
/// A look's `px` values are image's own: a pixel of a picture whose long side is 480, scaled to the layer's output picture
/// (batch C5b makes them design pixels of the canvas).
#[derive(Debug, Clone)]
pub(crate) struct Look {
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
    pub(crate) fn new(document: &Value) -> Result<Self, Error> {
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

    /// The names of the maps the recipe can be given (`subject`, `depth`, a `given` region).
    pub(crate) fn known_maps(&self) -> Vec<&str> {
        self.recipe.known_maps()
    }

    /// `layer` with the look run on its colour, straight, whatever the layer is; its alpha stays. `maps` are the picture's
    /// maps at the size of `layer`; the look sees those its recipe can be given, and image refuses the render, naming the
    /// step and the map, if the recipe needs one that is not there.
    pub(crate) fn apply(&self, layer: &Raster, maps: &BTreeMap<String, Vec<f32>>) -> Result<Raster, Error> {
        let colour: Vec<f32> = layer
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[red, green, blue, _]| [red, green, blue])
            .collect();
        let picture =
            bullterrier_image::Picture::new(layer.height(), layer.width(), &colour).map_err(|error| refused(&error))?;
        let known = self.recipe.known_maps();
        let mut photo = Photo::new(picture);
        for (name, values) in maps.iter().filter(|(name, _)| known.contains(&name.as_str())) {
            let map = bullterrier_image::Layer::new(layer.height(), layer.width(), values.clone())
                .map_err(|error| refused(&error))?;
            photo = photo.with_map(name, map).map_err(|error| refused(&error))?;
        }
        let image = photo
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
            maps: BTreeMap::new(),
        }
    }

    /// A picture read from `path` when it is drawn.
    pub(crate) fn file(path: PathBuf) -> Self {
        Self {
            pixels: Pixels::File(path),
            maps: BTreeMap::new(),
        }
    }

    /// The maps as maps of a picture of `size` = (columns, rows): files read, holes and maps of another size refused.
    fn resolved(&self, (columns, rows): (usize, usize)) -> Result<Vec<(&str, Arc<Map>)>, Error> {
        self.maps
            .iter()
            .map(|(name, given)| {
                let at = format!("map.{name}");
                let map = match given {
                    Given::Held(map) => Arc::clone(map),
                    Given::File(path) => Arc::new(read_map(path)?),
                    Given::Hole => {
                        return Err(Error::invalid(
                            &at,
                            "the map is a hole: nothing was given for it, and a hole is filled by a parameter (batch C6)",
                        ));
                    }
                };
                if (map.width(), map.height()) != (columns, rows) {
                    return Err(Error::invalid(
                        &at,
                        format!(
                            "the map is {} x {} pixels, but its picture is {columns} x {rows}",
                            map.width(),
                            map.height()
                        ),
                    ));
                }
                Ok((name.as_str(), map))
            })
            .collect()
    }
}

/// The map in the file `path`: a grey picture read as image reads a map (upright, 0 to 1).
fn read_map(path: &Path) -> Result<Map, Error> {
    let layer = bullterrier_image::read_map(path).map_err(|error| Error::Read {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    Ok(Map::made(layer.width(), layer.height(), layer.values().to_vec()))
}

/// `map` as the part `window` of its picture drawn at `columns` by `rows` pixels: resampled as the picture is, by the same
/// window and kernel.
fn resized(map: &Map, window: Rect, size: (usize, usize)) -> Vec<f32> {
    let levels = map
        .values()
        .iter()
        .flat_map(|&value| [value, value, value, 1.0])
        .collect();
    let raster = Arc::new(Raster::made(map.width(), map.height(), levels));
    resize(&raster, window, size)
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[0])
        .collect()
}

/// Whether `recipe` is a look this scene takes (a reference keeps its looks until it knows the layer they run on).
///
/// # Errors
/// As [`Source::looked`] of a picture: what image refuses in the recipe, a `format` it does not read.
pub(crate) fn check_look(recipe: &Value) -> Result<(), Error> {
    Look::new(recipe).map(|_| ())
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
    /// With no look on the layer, the picture's own pixels (masked by a map where the layer says so), for the warp to resample
    /// once; with one, the window at the asked scale (a part of the picture, or all of it) and the maps its looks and masks
    /// use at that size, so that the look runs on the raster the layer will have.
    fn open(&self, _canvas: Rect) -> Result<Opened<'_>, Error> {
        let own = match &self.pixels {
            Pixels::Held(raster) => Arc::clone(raster),
            Pixels::File(path) => Arc::new(read(path)?),
        };
        // a pixel count below 2^53: whole design pixels
        #[expect(clippy::cast_precision_loss, reason = "a pixel count below 2^53")]
        let whole = Rect {
            x: 0.0,
            y: 0.0,
            width: own.width() as f64,
            height: own.height() as f64,
        };
        let maps = self.resolved((own.width(), own.height()))?;
        Ok(Opened::finishing(whole, move |request, finish| {
            if !finish.is_looked() {
                // the picture's own pixels, for the warp to resample once; a mask multiplies them where they are, at the
                // density of source pixels per design pixel of the fitted layer
                let density = (1.0 / request.factor.0, 1.0 / request.factor.1);
                let values = maps.iter().map(|(name, map)| (*name, map.values())).collect();
                return Ok(Drawn {
                    raster: finish.masking(&own, density, &values)?,
                    covers: whole,
                    ends_at_window: false,
                    maps: BTreeMap::new(),
                });
            }
            let window = request.window;
            let (columns, rows) = request.scale.size(window)?;
            Ok(Drawn {
                raster: resize(&own, window, (columns, rows)),
                covers: window,
                ends_at_window: false,
                maps: maps
                    .iter()
                    .filter(|(name, _)| finish.uses_map(name))
                    .map(|(name, map)| ((*name).to_owned(), resized(map, window, (columns, rows))))
                    .collect(),
            })
        }))
    }

    fn map_names(&self) -> Option<Vec<&str>> {
        Some(self.maps.keys().map(String::as_str).collect())
    }

    fn with_map(&self, name: &str, given: Given) -> Result<Arc<dyn Source>, Error> {
        let mut maps = self.maps.clone();
        maps.insert(name.to_owned(), given);
        Ok(Arc::new(Self { maps, ..self.clone() }))
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
