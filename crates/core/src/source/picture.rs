//! The image adapter: a picture as a layer, and a look, an image recipe run on a raster. The one place that knows how the
//! image crate reads a picture file, a recipe, and renders one.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use bullterrier_image::{Number, Options, Photo, Presets, RECIPE_FORMATS, Recipe, Spread};
use serde_json::{Value, json};

use super::{Drawn, Given, Opened, Request, Rows, Source};
use crate::cache::{Cache, Keyed};
use crate::document::Form;
use crate::files::{Fingerprint, Located, digest_bytes};
use crate::finish::Px;
use crate::geometry::Rect;
use crate::parameters::{Parameter, Value as Setting};
use crate::warp::{resize, resize_rows};
use crate::{Error, Map, Raster};

/// A picture the layer holds, or a file it reads when it is drawn. Its box is its pixels, one design pixel each, at the
/// canvas origin.
#[derive(Debug, Clone)]
enum Pixels {
    Held(Arc<Raster>),
    File(Located),
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
/// A look's `px` values are image's design pixels (its design scale: the canvas's long side over 480), the
/// same number of output pixels on every layer of one canvas.
#[derive(Debug, Clone)]
pub(crate) struct Look {
    recipe: Recipe,
    presets: Arc<Presets>,
    /// The numbers of the recipe that can be set, as image lists them now.
    numbers: Arc<[Number]>,
    /// The unit of the look's `px`.
    px: Px,
    /// The digest of the look's document, made when first asked for: a look is never changed, `with_values` makes another.
    fingerprint: OnceLock<Fingerprint>,
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
    pub(crate) fn new(document: &Value, px: Px) -> Result<Self, Error> {
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
            px,
            presets: Arc::new(presets),
            fingerprint: OnceLock::new(),
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
            px: self.px,
            presets: Arc::clone(&self.presets),
            fingerprint: OnceLock::new(),
        })
    }

    /// This look as the document of a saved scene: `{"format": N, "recipe": {...}}`, with the lowest format the recipe needs, and
    /// `"px": "canvas"` for a look in canvas pixels.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `look` for a recipe image cannot write as data.
    pub(crate) fn document(&self) -> Result<Value, Error> {
        let recipe = serde_json::to_value(&self.recipe).map_err(|error| Error::invalid("look", error.to_string()))?;
        let mut document = json!({"format": self.recipe.format(), "recipe": recipe});
        // a design look is written as it always was
        if self.px != Px::Design {
            document["px"] = json!(self.px.name());
        }
        Ok(document)
    }

    /// Writes into `key` the look's document: the recipe as image writes it, the same text for the same look. **Limit:** a file a
    /// recipe names (a table or a picture given as a path) is in the document by its path, not by its content.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `look` for a recipe image cannot write as data.
    pub(crate) fn keyed(&self, key: &mut Keyed) -> Result<(), Error> {
        let fingerprint = if let Some(found) = self.fingerprint.get() {
            *found
        } else {
            let text = self.document()?.to_string();
            *self.fingerprint.get_or_init(|| digest_bytes(text.as_bytes()))
        };
        key.digest(&fingerprint);
        Ok(())
    }

    /// The room the `looks` ask for together, in pixels of the picture they run on: how far past it they paint on the top, right,
    /// bottom and left, as image declares it for the canvas of each look's unit (`Request::canvas_of`: what sets its design scale). The looks'
    /// sides add, each working on what the one before made, rounded up to whole pixels. `None` if a look may paint anywhere (a
    /// warp about the picture's centre, a statistic of the whole picture): no room holds it.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `look`, in image's words.
    pub(crate) fn room(looks: &[Self], request: &Request) -> Result<Option<[usize; 4]>, Error> {
        let mut total = Spread::NONE;
        for look in looks {
            let canvas = request.canvas_of(look.px);
            total = total.then(
                look.recipe
                    .spread(&look.presets, canvas.0.max(canvas.1))
                    .map_err(|error| refused(&error))?,
            );
        }
        Ok(total.pixels())
    }

    /// The rows of margin a band of the picture needs for this look to give the rows of the whole render (image's
    /// `Recipe::band_margin`), or `None` if the look cannot be run on a band. The canvas is the look's unit's.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `look`, in image's words.
    pub(crate) fn band_margin(&self, request: &Request) -> Result<Option<usize>, Error> {
        let canvas = request.canvas_of(self.px);
        self.recipe
            .band_margin(&self.presets, canvas.0.max(canvas.1))
            .map_err(|error| refused(&error))
    }

    /// The unit of the look's `px`.
    pub(crate) const fn px(&self) -> Px {
        self.px
    }

    /// The names of the maps the recipe can be given (`subject`, `depth`, a `given` region).
    pub(crate) fn known_maps(&self) -> Vec<&str> {
        self.recipe.known_maps()
    }

    /// `layer` with the look run on its colour, straight, whatever the layer is; its alpha stays. `maps` are the picture's
    /// maps at the size of `layer`; `canvas` is the canvas of the look's unit (`Request::canvas_of`: the scene's canvas in the output
    /// pixels this layer is drawn at, or that canvas shrunk to 480 canvas pixels a long side for a look in canvas pixels), which fixes the
    /// design scale of the look's `px`;
    /// a `light` layer's look sees the colour over black and its result is opaque (stage 7 makes the alpha: the light a look
    /// makes outside a shape is kept); the look sees those its recipe can be given, and image refuses the render, naming the
    /// step and the map, if the recipe needs one that is not there. `rows` says that `layer` is some rows of the whole raster
    /// (a window of it); `room` that `layer` is a box grown by that many pixels on the top, right, bottom and left.
    pub(crate) fn apply(
        &self,
        layer: &Raster,
        maps: &BTreeMap<String, Vec<f32>>,
        canvas: (u32, u32),
        light: bool,
        rows: Option<Rows>,
        room: [usize; 4],
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
        // rows of the whole picture are a window of it: its place and design scale are the whole picture's
        let options = match rows {
            Some(rows) => {
                // a raster of at most 2^26 pixels on a canvas of at most 2^27: every side fits
                let side = |length: usize| u32::try_from(length).unwrap_or(u32::MAX);
                let window = Options::default().with_window(side(rows.first), side(layer.width()), side(rows.total));
                // in a window the canvas fixes the design scale and the window only the place (image's rule)
                match self.px {
                    Px::Design => window,
                    Px::Canvas => window.with_canvas(canvas.0, canvas.1),
                }
            }
            None => Options::default().with_canvas(canvas.0, canvas.1),
        };
        // the raster is the box grown by `room`: a step that places itself by its picture is placed by the box
        let options = if room == [0; 4] {
            options
        } else {
            let [top, right, bottom, left] = room.map(|side| u32::try_from(side).unwrap_or(u32::MAX));
            options.with_room(top, right, bottom, left)
        };
        // one thread for each core: image keeps one pool of that size for the process, so renders running at once share it, and the
        // picture is the same at any count
        let options = options.with_threads(0);
        let image = photo
            .render(&self.recipe, &self.presets, options)
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
    pub(crate) fn file(file: Located) -> Self {
        Self {
            pixels: Pixels::File(file),
            maps: BTreeMap::new(),
        }
    }

    /// The maps as maps of a picture of `size` = (columns, rows): files read (or taken from `cache`), holes and maps of another
    /// size refused.
    fn resolved(&self, (columns, rows): (usize, usize), cache: &Cache) -> Result<Vec<(&str, Arc<Map>)>, Error> {
        self.maps
            .iter()
            .map(|(name, given)| {
                let at = format!("map.{name}");
                let map = match given {
                    Given::Held(map) => Arc::clone(map),
                    Given::File(file) => kept(cache, "map", file, read_map, |map| map.values().len())?,
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

/// What `read` makes of the file `file`: the one the cache holds if the file is in the state it was when it was read (its size and
/// modification time are the same: **limit**, a change that keeps both is not seen), else read now and kept. `floats` says how
/// many floats the value holds. A file that cannot be asked is read, and refused, as before.
fn kept<T: Send + Sync + 'static>(
    cache: &Cache,
    what: &str,
    file: &Located,
    read: impl FnOnce(&Located) -> Result<T, Error>,
    floats: impl FnOnce(&T) -> usize,
) -> Result<Arc<T>, Error> {
    let mut key = Keyed::new(what);
    if !file.keyed(&mut key) {
        return read(file).map(Arc::new);
    }
    cache.fetch(&key.finish(), || {
        let value = read(file)?;
        let bytes = floats(&value) * size_of::<f32>();
        Ok((value, bytes))
    })
}

/// The map in the file `file`: a grey picture read as image reads a map (upright, 0 to 1).
fn read_map(file: &Located) -> Result<Map, Error> {
    file.verified()?;
    let path = file.resolved();
    let layer = bullterrier_image::read_map(&path).map_err(|error| Error::Read {
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

/// `raster` written to the file `path` by image's writer, which rounds its levels and picks the format by the extension.
///
/// # Errors
/// [`Error::Write`] with the writer's message.
pub(crate) fn write(raster: &Raster, path: &Path, dpi: Option<f64>) -> Result<(), Error> {
    let options = dpi.map_or_else(bullterrier_image::WriteOptions::new, |dpi| {
        bullterrier_image::WriteOptions::new().with_dpi(dpi)
    });
    // a JPEG holds no alpha: an opaque picture goes as colour alone, a translucent one is refused by image's writer
    let jpeg = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg"));
    let image = if jpeg && raster.pixels().as_chunks::<4>().0.iter().all(|pixel| pixel[3] >= 1.0) {
        let colour: Vec<f32> = raster
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect();
        bullterrier_image::Image::new(raster.height(), raster.width(), &colour)
    } else {
        bullterrier_image::Image::rgba(raster.height(), raster.width(), raster.pixels())
    };
    let written = image.and_then(|image| bullterrier_image::write_with(path, &image, &options));
    written.map_err(|error| Error::Write {
        path: path.display().to_string(),
        message: error.to_string(),
    })
}

/// The picture in `file`: upright, in sRGB values, with its alpha if it has one.
fn read(file: &Located) -> Result<Raster, Error> {
    file.verified()?;
    let path = file.resolved();
    let image = bullterrier_image::read(&path).map_err(|error| Error::Read {
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
    /// `picture` (`null` for a hole), and `maps` if there are any.
    fn document(&self, form: Form) -> Result<Vec<(&'static str, Value)>, Error> {
        let picture = match &self.pixels {
            Pixels::Held(raster) => form.held("picture", (raster.width(), raster.height()), raster.digest())?,
            Pixels::File(file) => form.file(file)?,
            Pixels::Hole => Value::Null,
        };
        let mut found = vec![("picture", picture)];
        if !self.maps.is_empty() {
            let mut maps = serde_json::Map::new();
            for (name, given) in &self.maps {
                let map = match given {
                    Given::Held(map) => form.held(&format!("map.{name}"), (map.width(), map.height()), map.digest())?,
                    Given::File(file) => form.file(file)?,
                    Given::Hole => Value::Null,
                };
                maps.insert(name.clone(), map);
            }
            found.push(("maps", Value::Object(maps)));
        }
        Ok(found)
    }

    /// The picture (its pixels held, or its file as it is now) and each map by its name.
    fn keyed(&self, key: &mut Keyed) -> Result<(), Error> {
        key.text("picture");
        match &self.pixels {
            Pixels::Held(raster) => {
                key.text("held")
                    .number(raster.width())
                    .number(raster.height())
                    .digest(raster.digest());
            }
            Pixels::File(file) => {
                key.text("file");
                file.keyed(key);
            }
            Pixels::Hole => {
                key.text("hole");
            }
        }
        for (name, given) in &self.maps {
            key.text(name);
            match given {
                Given::Held(map) => {
                    key.text("held")
                        .number(map.width())
                        .number(map.height())
                        .digest(map.digest());
                }
                Given::File(file) => {
                    key.text("file");
                    file.keyed(key);
                }
                Given::Hole => {
                    key.text("hole");
                }
            }
        }
        Ok(())
    }

    /// With no look on the layer, the picture's own pixels (masked by a map where the layer says so), for the warp to resample
    /// once; with one, the window at the asked scale (a part of the picture, or all of it) and the maps its looks and masks
    /// use at that size, so that the look runs on the raster the layer will have.
    fn open(&self, _canvas: Rect, cache: &Cache) -> Result<Opened<'_>, Error> {
        let own = match &self.pixels {
            Pixels::Held(raster) => Arc::clone(raster),
            Pixels::File(file) => kept(cache, "picture", file, read, |raster| raster.pixels().len())?,
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
        let maps = self.resolved((own.width(), own.height()), cache)?;
        Ok(Opened::finishing(whole, move |request, finish, _| {
            if !finish.is_looked() {
                // the picture's own pixels, for the warp to resample once; a mask multiplies them where they are, at the
                // density of source pixels per design pixel of the fitted layer
                let density = (1.0 / request.factor.0, 1.0 / request.factor.1);
                let values = maps.iter().map(|(name, map)| (*name, map.values())).collect();
                return Ok(Drawn {
                    raster: finish.masking(&own, density, &values)?,
                    covers: whole,
                    ends_at_window: false,
                    room: [0; 4],
                    maps: BTreeMap::new(),
                });
            }
            let window = request.window;
            let shape = request.window_shape();
            let rows_wanted = request.rows_within_limit(shape)?;
            let (columns, rows) = shape.size;
            // some rows of the picture at the drawn size, each pixel what the whole has (see `Opened::by_window`)
            let raster = if request.rows.is_some() {
                resize_rows(&own, window, (columns, rows), rows_wanted)
            } else {
                resize(&own, window, (columns, rows))
            };
            Ok(Drawn {
                raster,
                covers: window,
                ends_at_window: false,
                room: [0; 4],
                maps: maps
                    .iter()
                    .filter(|(name, _)| finish.uses_map(name))
                    .map(|(name, map)| ((*name).to_owned(), resized(map, window, (columns, rows))))
                    .collect(),
            })
        })
        .own_pixels()
        .by_window())
    }

    fn map_names(&self) -> Option<Vec<&str>> {
        Some(self.maps.keys().map(String::as_str).collect())
    }

    /// `picture`, then `map.<name>` for each map, by name.
    fn parameters(&self) -> Vec<Parameter> {
        let value = match &self.pixels {
            Pixels::Held(raster) => Setting::Picture(Arc::clone(raster)),
            Pixels::File(file) => Setting::File(file.resolved()),
            Pixels::Hole => Setting::Hole,
        };
        let mut found = vec![Parameter::picture("picture", "the picture the layer shows", value)];
        for (name, given) in &self.maps {
            let value = match given {
                Given::Held(map) => Setting::Map(Arc::clone(map)),
                Given::File(file) => Setting::File(file.resolved()),
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
                    Setting::File(path) => Pixels::File(Located::new(path)),
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
                    Setting::File(path) => Given::File(Located::new(path)),
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
