//! The image adapter: a picture as a layer, and a look, an image recipe run on a raster. The one place that knows how the
//! image crate reads a picture file, a recipe, and renders one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bullterrier_image::{Number, Options, Photo, Presets, RECIPE_FORMATS, Recipe};
use serde_json::Value;

use super::{Drawn, Given, Opened, Source};
use crate::geometry::Rect;
use crate::parameters::{Parameter, Value as Setting};
use crate::warp::resize;
use crate::{Error, Map, Raster};

/// A picture the layer holds, or a file it reads when it is drawn. Its box is its pixels, one design pixel each, at the
/// canvas origin.
#[derive(Debug, Clone)]
enum Pixels {
    Held(Arc<Raster>),
    File(PathBuf),
    /// Nothing given yet: a parameter fills it before the scene renders.
    Hole,
}

/// A picture and the maps it was given by name.
#[derive(Debug, Clone)]
pub(crate) struct Picture {
    pixels: Pixels,
    maps: BTreeMap<String, Given>,
}

/// A recipe that image has checked, with the presets it was checked against.
///
/// A look's `px` values are design pixels of the scene's canvas (image's design scale: the canvas's long side over 480), the
/// same number of output pixels on every layer of one canvas.
#[derive(Debug, Clone)]
pub(crate) struct Look {
    recipe: Recipe,
    presets: Arc<Presets>,
    /// The numbers of the recipe that can be set, as image lists them now.
    numbers: Arc<[Number]>,
}

/// The numbers of `recipe` that can be set. Image lists the frame's too, whether the recipe sets a frame or not; a look sets no
/// frame, so they are not a look's parameters.
fn numbers_of(recipe: &Recipe) -> Result<Arc<[Number]>, Error> {
    let numbers = recipe.numbers().map_err(|error| refused(&error))?;
    Ok(numbers
        .into_iter()
        .filter(|number| !number.address.starts_with("frame."))
        .collect())
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
    /// The look the recipe document `document` is: an object image reads, with a `format` it reads or none (a refused one is
    /// an [`Error::Format`] naming the versions read).
    pub(crate) fn new(document: &Value) -> Result<Self, Error> {
        let Value::Object(members) = document else {
            return Err(Error::Format {
                pointer: "/look".to_owned(),
                message: format!("a look is a JSON object, got {document}"),
            });
        };
        if let Some(found) = members.get("format")
            && Recipe::read_format(found).is_err()
        {
            let read: Vec<String> = RECIPE_FORMATS.iter().map(ToString::to_string).collect();
            return Err(Error::Format {
                pointer: "/look/format".to_owned(),
                message: format!(
                    "image recipe format {found}; this scene reads image formats {}",
                    read.join(", ")
                ),
            });
        }
        let recipe = Recipe::from_json(&document.to_string()).map_err(|error| refused(&error))?;
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
            numbers: numbers_of(&recipe)?,
            recipe,
            presets: Arc::new(presets),
        })
    }

    /// The numbers of this look as parameters, at image's own addresses: the unit, range, meaning and whole or cyclic nature
    /// are image's, the value the recipe holds. Image's `Number` has no default of its own, so the value is also the default.
    pub(crate) fn parameters(&self) -> Vec<Parameter> {
        self.numbers
            .iter()
            .map(|number| {
                // a look sets no `frame`, the one place a number has no value (`numbers_of` leaves those out)
                let value = number.value.unwrap_or(number.min);
                Parameter::number(
                    &number.address,
                    &number.doc,
                    &number.unit,
                    (number.min, number.max),
                    (number.whole, number.cyclic),
                    value,
                    value,
                )
            })
            .collect()
    }

    /// This look with each of `values` (an address of its recipe, a number) set: image checks the set as it checks any recipe,
    /// the rules between numbers set together included.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `look`, in image's words.
    pub(crate) fn with_values(&self, values: &[(&str, f64)]) -> Result<Self, Error> {
        let recipe = self
            .recipe
            .with_values(values, &self.presets)
            .map_err(|error| refused(&error))?;
        Ok(Self {
            numbers: numbers_of(&recipe)?,
            recipe,
            presets: Arc::clone(&self.presets),
        })
    }

    /// The names of the maps the recipe can be given (`subject`, `depth`, a `given` region).
    pub(crate) fn known_maps(&self) -> Vec<&str> {
        self.recipe.known_maps()
    }

    /// `layer` with the look run on its colour, straight, whatever the layer is; its alpha stays. `maps` are the picture's
    /// maps at the size of `layer`; `canvas` is the scene's canvas in the output pixels this layer is drawn at (width, height), which fixes the
    /// design scale of the look's `px`;
    /// a `light` layer's look sees the colour over black and its result is opaque (stage 7 makes the alpha: the light a look
    /// makes outside a shape is kept); the look sees those its recipe can be given, and image refuses the render, naming the
    /// step and the map, if the recipe needs one that is not there.
    pub(crate) fn apply(
        &self,
        layer: &Raster,
        maps: &BTreeMap<String, Vec<f32>>,
        canvas: (u32, u32),
        light: bool,
    ) -> Result<Raster, Error> {
        let colour: Vec<f32> = layer
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[red, green, blue, alpha]| {
                let over_black = if light { alpha } else { 1.0 };
                [red * over_black, green * over_black, blue * over_black]
            })
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
            .render(
                &self.recipe,
                &self.presets,
                Options::default().with_canvas(canvas.0, canvas.1),
            )
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
            .flat_map(|(&[red, green, blue], pixel)| [red, green, blue, if light { 1.0 } else { pixel[3] }])
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

    /// A picture to be given: a render refuses it until a value fills the hole.
    pub(crate) fn hole() -> Self {
        Self {
            pixels: Pixels::Hole,
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
                            "the map is a hole: nothing was given for it; fill it with `with_values`",
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
            Pixels::Hole => {
                return Err(Error::invalid(
                    "picture",
                    "the picture is a hole: nothing was given for it; fill it with `with_values`",
                ));
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

    /// `picture`, then `map.<name>` for each map, by name.
    fn parameters(&self) -> Vec<Parameter> {
        let value = match &self.pixels {
            Pixels::Held(raster) => Setting::Picture(Arc::clone(raster)),
            Pixels::File(path) => Setting::File(path.clone()),
            Pixels::Hole => Setting::Hole,
        };
        let mut found = vec![Parameter::picture("picture", "the picture the layer shows", value)];
        for (name, given) in &self.maps {
            let value = match given {
                Given::Held(map) => Setting::Map(Arc::clone(map)),
                Given::File(path) => Setting::File(path.clone()),
                Given::Hole => Setting::Hole,
            };
            found.push(Parameter::picture(
                &format!("map.{name}"),
                "a value from 0 to 1 for each pixel of the picture: a matte, a depth",
                value,
            ));
        }
        found
    }

    fn with_values(&self, values: &[(&str, &Setting)]) -> Result<Arc<dyn Source>, Error> {
        let mut next = self.clone();
        for &(address, value) in values {
            if address == "picture" {
                next.pixels = match value {
                    Setting::Picture(raster) => Pixels::Held(Arc::clone(raster)),
                    Setting::File(path) => Pixels::File(path.clone()),
                    Setting::Hole => Pixels::Hole,
                    other => {
                        return Err(other.wrong(
                            address,
                            "a picture (an array of shape (height, width, 3 or 4)), a path or none",
                        ));
                    }
                };
            } else if let Some(name) = address.strip_prefix("map.") {
                let given = match value {
                    Setting::Map(map) => Given::Held(Arc::clone(map)),
                    Setting::File(path) => Given::File(path.clone()),
                    Setting::Hole => Given::Hole,
                    other => {
                        return Err(other.wrong(address, "a map (an array of shape (height, width)), a path or none"));
                    }
                };
                next.maps.insert(name.to_owned(), given);
            }
        }
        Ok(Arc::new(next))
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
