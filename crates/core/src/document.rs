//! The saved scene: the one place that knows the JSON document ("Saved scene" in `docs/api.md`). `Scene::to_json` writes it,
//! `Scene::from_json` reads it, and two scenes are equal when they write the same document. A source writes its own keys
//! (`Source::document`); everything else of a layer is written and read here.

use std::cell::RefCell;
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value, json};

use crate::blend::Blend;
use crate::budget::{MAX_DEPTH, MAX_LAYERS};
use crate::error::label;
use crate::files::{Fingerprint, Located, hex_of};
use crate::finish::Px;
use crate::layer::Base;
use crate::mask::{Mask, MaskBy, Use};
use crate::placement::{Deg, Fit, Place, Placed, Placement};
use crate::suggest::nearest;
use crate::text::Text;
use crate::{Error, Layer, LayerKinds, Scene};

/// The version of the document this release writes and reads.
const VERSION: i64 = 1;

/// The keys of a layer, in the order they are written.
const LAYER_KEYS: [&str; 22] = [
    "name",
    "picture",
    "maps",
    "drawing",
    "values",
    "text",
    "font",
    "color",
    "options",
    "fill",
    "scene",
    "of",
    "make",
    "fit",
    "place",
    "perspective",
    "opacity",
    "blend",
    "light",
    "inks",
    "masks",
    "looks",
];
/// The keys that say what a layer draws: one of them.
pub(crate) const SOURCES: [&str; 7] = ["picture", "drawing", "text", "fill", "scene", "of", "make"];
/// The keys a source of one kind owns: they are refused beside any other.
const OWNED: [(&str, &str); 5] = [
    ("maps", "picture"),
    ("values", "drawing"),
    ("font", "text"),
    ("color", "text"),
    ("options", "text"),
];
const PLACE_KEYS: [&str; 10] = [
    "dx", "dy", "scale", "aspect", "rotate", "skew_x", "skew_y", "flip_x", "flip_y", "anchor",
];

/// What a document is written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Form {
    /// To be saved: a file is its path as written and the digest it has now, and a picture held in memory is refused (it has
    /// no file).
    Save,
    /// To be compared: a file is the place it is at, and a picture held in memory is the digest of its pixels.
    Compare,
}

impl Form {
    /// The document of `file`.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a path that is not text, [`Error::Read`] if the digest cannot be made (saving only).
    pub(crate) fn file(self, file: &Located) -> Result<Value, Error> {
        match self {
            Self::Save => {
                let path = file.written().to_str().ok_or_else(|| {
                    Error::invalid(
                        "path",
                        format!("a path in a saved scene is text, got {}", file.written().display()),
                    )
                })?;
                Ok(json!({"path": path, "sha256": file.digest()?}))
            }
            Self::Compare => Ok(json!({"path": file.resolved().to_string_lossy()})),
        }
    }

    /// The document of a picture or a map held in memory, `size` columns by rows, with the digest `digest` of its values.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `what` when the document is to be saved: pixels in memory have no file.
    pub(crate) fn held(self, what: &str, size: (usize, usize), digest: &Fingerprint) -> Result<Value, Error> {
        match self {
            Self::Save => Err(Error::invalid(
                what,
                "an array has no file: save it as a picture file and give its Path, or leave a hole to fill with `with_values`",
            )),
            Self::Compare => Ok(json!({"pixels": [size.0, size.1, hex_of(digest)]})),
        }
    }
}

/// `value` as a number of the document: a whole number is written whole.
pub(crate) fn number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9e15 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a whole number below 9e15 is exact in an i64"
        )]
        return Value::from(value as i64);
    }
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// A 32-bit float as the shortest decimal that reads back as the same float.
fn single(value: f32) -> Value {
    number(value.to_string().parse().unwrap_or_else(|_| f64::from(value)))
}

// --- writing -----------------------------------------------------------------------------------------------------------

/// The document of `scene`.
///
/// # Errors
/// What a source or a look refuses to write, with the address of the layer.
pub(crate) fn scene_document(scene: &Scene, form: Form) -> Result<Value, Error> {
    let mut layers = Vec::with_capacity(scene.layers().len());
    for (at, layer) in scene.layers().iter().enumerate() {
        let document = layer_document(layer, form).map_err(|error| error.in_layer(&layer.called(at + 1)))?;
        layers.push(Value::Object(document));
    }
    Ok(json!({
        "version": VERSION,
        "canvas": {"width": scene.width(), "height": scene.height()},
        "layers": layers,
    }))
}

/// The document of the fields of `placed` that are not the defaults.
#[expect(
    clippy::float_cmp,
    reason = "a field is left out only when it is exactly the default"
)]
fn placed_document(placed: &Placed) -> Value {
    let (was, mut found) = (Placed::default(), Map::new());
    for (key, now, before) in [
        ("dx", placed.dx, was.dx),
        ("dy", placed.dy, was.dy),
        ("scale", placed.scale, was.scale),
        ("aspect", placed.aspect, was.aspect),
        ("rotate", placed.rotate, was.rotate),
        ("skew_x", placed.skew_x, was.skew_x),
        ("skew_y", placed.skew_y, was.skew_y),
    ] {
        if now != before {
            found.insert(key.to_owned(), number(now));
        }
    }
    for (key, now) in [("flip_x", placed.flip_x), ("flip_y", placed.flip_y)] {
        if now {
            found.insert(key.to_owned(), Value::Bool(true));
        }
    }
    if placed.anchor != was.anchor {
        found.insert(
            "anchor".to_owned(),
            json!([number(placed.anchor.0), number(placed.anchor.1)]),
        );
    }
    Value::Object(found)
}

/// The document of `layer`: its keys in the order of `LAYER_KEYS`, each only where it differs from its default.
fn layer_document(layer: &Layer, form: Form) -> Result<Map<String, Value>, Error> {
    let mut found = Map::new();
    if let Some(name) = &layer.name {
        found.insert("name".to_owned(), Value::String(name.clone()));
    }
    // masks and looks are listed after the rest; a mask by a map and a mask by a layer are in one list, by call
    let mut masks: Vec<(usize, Value)> = Vec::new();
    let mut looks: Vec<Value> = Vec::new();
    let map_mask = |mask: &crate::finish::MapMask| {
        let mut entry = Map::new();
        entry.insert("map".to_owned(), Value::String(mask.name.clone()));
        feathered(&mut entry, mask.feather, mask.invert);
        (mask.number, Value::Object(entry))
    };
    match &layer.base {
        Base::Body(body) => {
            for (key, value) in body.source.document(form)? {
                found.insert(key.to_owned(), value);
            }
            if let Some(fit) = &body.fit {
                let mut entry = Map::new();
                let area = fit.area;
                entry.insert(
                    "box".to_owned(),
                    Value::Array([area.x, area.y, area.width, area.height].map(number).to_vec()),
                );
                if fit.how != Fit::Cover {
                    entry.insert("how".to_owned(), Value::String(fit.how.name().to_owned()));
                }
                if fit.align != (0.5, 0.5) {
                    entry.insert("align".to_owned(), json!([number(fit.align.0), number(fit.align.1)]));
                }
                found.insert("fit".to_owned(), Value::Object(entry));
            }
            match &body.placement {
                Placement::Place(placed) if *placed != Placed::default() => {
                    found.insert("place".to_owned(), placed_document(placed));
                }
                Placement::Place(_) => {}
                Placement::Perspective(quad) => {
                    let corners = quad.corners().map(|[x, y]| json!([number(x), number(y)]));
                    found.insert("perspective".to_owned(), Value::Array(corners.to_vec()));
                }
            }
            masks.extend(body.finish.masks().iter().map(map_mask));
            for look in body.finish.looks() {
                looks.push(look.document()?);
            }
        }
        Base::Of(reference) => {
            found.insert("of".to_owned(), Value::String(reference.name.clone()));
            if reference.place != Placed::default() {
                found.insert("place".to_owned(), placed_document(&reference.place));
            }
            masks.extend(reference.masks.iter().map(map_mask));
            for look in &reference.looks {
                looks.push(look.document()?);
            }
        }
    }
    let join = &layer.join;
    if join.opacity.to_bits() != 1.0_f32.to_bits() {
        found.insert("opacity".to_owned(), single(join.opacity));
    }
    if join.blend != Blend::Normal {
        found.insert("blend".to_owned(), Value::String(join.blend.to_string()));
    }
    if join.light {
        found.insert("light".to_owned(), Value::Bool(true));
    }
    if !join.inks.is_empty() {
        found.insert("inks".to_owned(), join.inks.iter().cloned().map(Value::String).collect());
    }
    for mask in &join.masks {
        let mut entry = Map::new();
        let inner = layer_document(mask.layer(), form).map_err(|error| error.in_layer(&label(mask.number())))?;
        entry.insert("layer".to_owned(), Value::Object(inner));
        if mask.uses != Use::Alpha {
            entry.insert("use".to_owned(), Value::String(mask.uses.name().to_owned()));
        }
        feathered(&mut entry, mask.feather, mask.invert);
        masks.push((mask.number(), Value::Object(entry)));
    }
    masks.sort_by_key(|(number, _)| *number);
    if !masks.is_empty() {
        found.insert(
            "masks".to_owned(),
            Value::Array(masks.into_iter().map(|(_, mask)| mask).collect()),
        );
    }
    if !looks.is_empty() {
        found.insert("looks".to_owned(), Value::Array(looks));
    }
    Ok(found)
}

/// `feather` and `invert` of a mask, each only where it is not the default.
fn feathered(entry: &mut Map<String, Value>, feather: f64, invert: bool) {
    if feather != 0.0 {
        entry.insert("feather".to_owned(), number(feather));
    }
    if invert {
        entry.insert("invert".to_owned(), Value::Bool(true));
    }
}

impl PartialEq for Layer {
    /// Two layers are equal when they write the same document: a picture or a map held in memory by its pixels, a file by
    /// where it is, a drawing and a look by what their crates write of them.
    fn eq(&self, other: &Self) -> bool {
        layer_document(self, Form::Compare) == layer_document(other, Form::Compare)
    }
}

impl PartialEq for Scene {
    /// Two scenes are equal when they write the same document (see [`Layer`]'s equality). Equal scenes render the same
    /// pictures and list the same parameters.
    fn eq(&self, other: &Self) -> bool {
        scene_document(self, Form::Compare) == scene_document(other, Form::Compare)
    }
}

impl Scene {
    /// This scene as one self-contained JSON document ("Saved scene" in `docs/api.md`): its canvas and its layers, each key
    /// only where it differs from its default, a look and a drawing inline, a picture, a map and a font file by its path
    /// as the scene holds it and its SHA-256 as the file is now. The same scene gives the same text.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a picture or a map held in memory (it has no file; the address names the layer),
    /// [`Error::Read`] for a file whose digest cannot be made.
    pub fn to_json(&self) -> Result<String, Error> {
        Ok(scene_document(self, Form::Save)?.to_string())
    }

    /// The scene a document written by [`Scene::to_json`] describes. A relative path in it is relative to the folder `base`
    /// (an empty path is the working folder). No picture and no map is read now: they are read when the scene renders, and a
    /// digest that is not the file's is an [`Error::Read`] then. A font file is read now (a text's box needs the font), and
    /// so is a path where a look or a drawing belongs.
    ///
    /// # Errors
    /// [`Error::Format`] with the JSON Pointer of the value that is wrong: text that is not JSON (or writes a key twice), a
    /// version this release does not read, a key it does not know, a value of the wrong kind or outside its range, a layer
    /// the scene refuses (a name used twice, a reference that finds no layer or leads back to itself, too many layers or
    /// too deep), a look or a drawing its crate does not read, a version skew included. [`Error::Read`] for a font, a look
    /// file or a drawing file that cannot be read.
    pub fn from_json(text: &str, base: &Path) -> Result<Self, Error> {
        Self::from_json_with(text, base, &LayerKinds::new())
    }

    /// As [`Scene::from_json`], reading a layer of a registered kind (`{"make": {"kind": ..., "options": ...}}`) through
    /// `kinds`.
    ///
    /// # Errors
    /// As [`Scene::from_json`]; and [`Error::Format`] at `/layers/N/make/kind` for a kind `kinds` does not hold (the
    /// message lists the kinds it holds), and at `/layers/N/make/options` for options the kind refuses.
    pub fn from_json_with(text: &str, base: &Path, kinds: &LayerKinds) -> Result<Self, Error> {
        read_scene(&parse(text)?, "", base, kinds, 0)
    }
}

// --- reading -----------------------------------------------------------------------------------------------------------

/// The value of the JSON text, refusing an object that writes a key twice (`serde_json` would keep the last).
fn parse(text: &str) -> Result<Value, Error> {
    let path = RefCell::new(Vec::new());
    let twice = RefCell::new(None);
    let mut reader = serde_json::Deserializer::from_str(text);
    let parsed = Strict {
        path: &path,
        twice: &twice,
    }
    .deserialize(&mut reader)
    .and_then(|value| reader.end().map(|()| value));
    parsed.map_err(|error| match twice.take() {
        Some(pointer) => Error::Format {
            pointer,
            message: "a key is written twice in one object".to_owned(),
        },
        None => Error::Format {
            pointer: String::new(),
            message: format!("not JSON: {error}"),
        },
    })
}

/// A JSON value being read: `path` is where it is (the segments of its pointer), and `twice` takes the pointer of a key an
/// object writes twice. `serde_json` stops a nest at 128 levels, so reading recurses no deeper.
struct Strict<'a> {
    path: &'a RefCell<Vec<String>>,
    twice: &'a RefCell<Option<String>>,
}

impl<'de> DeserializeSeed<'de> for Strict<'_> {
    type Value = Value;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Strict<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(Value::from(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(Value::from(value))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
        Ok(Number::from_f64(value).map_or(Value::Null, Value::Number))
    }

    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        loop {
            self.path.borrow_mut().push(items.len().to_string());
            let item = seq.next_element_seed(Strict {
                path: self.path,
                twice: self.twice,
            })?;
            self.path.borrow_mut().pop();
            match item {
                Some(item) => items.push(item),
                None => return Ok(Value::Array(items)),
            }
        }
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut members = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if members.contains_key(&key) {
                self.path.borrow_mut().push(key);
                *self.twice.borrow_mut() = Some(pointer_of(&self.path.borrow()));
                return Err(de::Error::custom("a key is written twice"));
            }
            self.path.borrow_mut().push(key.clone());
            let value = map.next_value_seed(Strict {
                path: self.path,
                twice: self.twice,
            })?;
            self.path.borrow_mut().pop();
            members.insert(key, value);
        }
        Ok(Value::Object(members))
    }
}

/// The JSON Pointer of the value at `segments`.
fn pointer_of(segments: &[String]) -> String {
    segments.iter().map(|segment| escaped("", segment)).collect()
}

/// The pointer of the member `key` (or the item) of the value at `at`.
fn escaped(at: &str, key: &str) -> String {
    format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a bool",
        Value::Number(_) => "a number",
        Value::String(_) => "a text",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

/// The refusal of a value at `at`: what it should have been, and what it is.
fn wrong(at: &str, wanted: &str, value: &Value) -> Error {
    let shown = value.to_string();
    let shown = if shown.chars().count() > 40 {
        format!("{} ...", shown.chars().take(40).collect::<String>())
    } else {
        shown
    };
    Error::Format {
        pointer: at.to_owned(),
        message: format!("{wanted} is wanted, got {} {shown}", kind(value)),
    }
}

fn object<'a>(value: &'a Value, at: &str, wanted: &str) -> Result<&'a Map<String, Value>, Error> {
    value.as_object().ok_or_else(|| wrong(at, wanted, value))
}

fn text<'a>(value: &'a Value, at: &str, wanted: &str) -> Result<&'a str, Error> {
    value.as_str().ok_or_else(|| wrong(at, wanted, value))
}

fn real(value: &Value, at: &str) -> Result<f64, Error> {
    value.as_f64().ok_or_else(|| wrong(at, "a number", value))
}

fn flag(value: &Value, at: &str) -> Result<bool, Error> {
    value.as_bool().ok_or_else(|| wrong(at, "a bool", value))
}

/// `value` as `N` numbers.
fn numbers<const N: usize>(value: &Value, at: &str, wanted: &str) -> Result<[f64; N], Error> {
    let items = value
        .as_array()
        .filter(|items| items.len() == N)
        .ok_or_else(|| wrong(at, wanted, value))?;
    let mut found = [0.0; N];
    for (index, (slot, item)) in found.iter_mut().zip(items).enumerate() {
        *slot = real(item, &escaped(at, &index.to_string()))?;
    }
    Ok(found)
}

/// The members of `map` are all among `valid`; the first that is not is refused, naming the keys and the nearest.
fn known_keys(map: &Map<String, Value>, at: &str, what: &str, valid: &[&str]) -> Result<(), Error> {
    let Some(key) = map.keys().find(|key| !valid.contains(&key.as_str())) else {
        return Ok(());
    };
    let near = nearest(key, valid.iter().copied());
    let hint = match near.as_slice() {
        [] => String::new(),
        [one] => format!(" (did you mean {one}?)"),
        several => format!(" (did you mean one of {}?)", several.join(", ")),
    };
    Err(Error::Format {
        pointer: escaped(at, key),
        message: format!(
            "`{key}` is not a key of {what}; the keys are: {}{hint}",
            valid.join(", ")
        ),
    })
}

/// A refusal of the core as the refusal of the document at `at`: its words, under the pointer of the member of `within` the
/// error's address names, if it names one. A sibling's pointer (`/drawing/...`, `/look/format`) is put under `at`.
fn refused(at: &str, within: Option<&Map<String, Value>>, error: Error) -> Error {
    match error {
        Error::Invalid { address, message } => {
            let head = address.split('.').next().unwrap_or_default();
            if within.is_some_and(|map| map.contains_key(head)) {
                Error::Format {
                    pointer: escaped(at, head),
                    message,
                }
            } else {
                Error::Format {
                    pointer: at.to_owned(),
                    message: format!("{address}: {message}"),
                }
            }
        }
        Error::Format { pointer, message } => Error::Format {
            pointer: format!("{at}{pointer}"),
            message,
        },
        read => read,
    }
}

/// The folder `base` and `written` as a file of the scene: `{"path": ..., "sha256": ...}` (the digest is optional on reading).
fn read_file(value: &Value, at: &str, base: &Path) -> Result<Located, Error> {
    let wanted = "a file is {\"path\": ..., \"sha256\": ...}";
    let map = object(value, at, wanted)?;
    known_keys(map, at, "a file", &["path", "sha256"])?;
    let path = map
        .get("path")
        .ok_or_else(|| Error::Format {
            pointer: at.to_owned(),
            message: format!("{wanted}: `path` is missing"),
        })
        .and_then(|path| text(path, &escaped(at, "path"), "a path (text)"))?;
    let digest = map
        .get("sha256")
        .map(|digest| text(digest, &escaped(at, "sha256"), "a digest (text)").map(str::to_owned))
        .transpose()?;
    Ok(Located::saved(path, base, digest))
}

/// The refusal of a scene or a mask layer nested more than `MAX_DEPTH` deep in the document. Every level of nesting the
/// document has is at least one level of depth for the scene (`Budget`), so this refuses nothing `Scene::add` would take, and
/// it stops the reader's own recursion before the scene could.
fn too_deep(at: &str, depth: usize) -> Result<(), Error> {
    if depth > MAX_DEPTH {
        return Err(Error::Format {
            pointer: at.to_owned(),
            message: format!(
                "scenes and mask layers nest at most {MAX_DEPTH} deep: drawing leans on other layers at most {MAX_DEPTH} deep"
            ),
        });
    }
    Ok(())
}

/// `depth` is how many scenes and mask layers the value is inside.
fn read_scene(value: &Value, at: &str, base: &Path, kinds: &LayerKinds, depth: usize) -> Result<Scene, Error> {
    too_deep(at, depth)?;
    let map = object(value, at, "a scene is an object")?;
    known_keys(map, at, "a scene", &["version", "canvas", "layers"])?;
    let version = map.get("version").ok_or_else(|| Error::Format {
        pointer: at.to_owned(),
        message: format!("`version` is missing; this release reads scene version {VERSION}"),
    })?;
    if version.as_i64() != Some(VERSION) {
        return Err(Error::Format {
            pointer: escaped(at, "version"),
            message: format!("scene version {version}; this release reads scene version {VERSION}"),
        });
    }
    let canvas = map.get("canvas").ok_or_else(|| Error::Format {
        pointer: at.to_owned(),
        message: "`canvas` is missing: {\"width\": ..., \"height\": ...}".to_owned(),
    })?;
    let canvas_at = escaped(at, "canvas");
    let sides = object(canvas, &canvas_at, "a canvas is {\"width\": ..., \"height\": ...}")?;
    known_keys(sides, &canvas_at, "a canvas", &["width", "height"])?;
    let side = |key: &str| -> Result<i128, Error> {
        let value = sides.get(key).ok_or_else(|| Error::Format {
            pointer: canvas_at.clone(),
            message: format!("`{key}` is missing"),
        })?;
        let side_at = escaped(&canvas_at, key);
        value
            .as_i64()
            .map(i128::from)
            .ok_or_else(|| wrong(&side_at, "a whole number of design pixels", value))
    };
    let scene = Scene::new(side("width")?, side("height")?).map_err(|error| refused(&canvas_at, Some(sides), error))?;
    let layers_at = escaped(at, "layers");
    let Some(layers) = map.get("layers") else {
        return Ok(scene);
    };
    let items = layers
        .as_array()
        .ok_or_else(|| wrong(&layers_at, "a list of layers", layers))?;
    if items.len() > MAX_LAYERS {
        return Err(Error::Format {
            pointer: layers_at,
            message: format!("{} layers; a scene draws at most {MAX_LAYERS}", items.len()),
        });
    }
    let layers = items
        .iter()
        .enumerate()
        .map(|(index, item)| read_layer(item, &escaped(&layers_at, &index.to_string()), base, kinds, depth))
        .collect::<Result<Vec<_>, _>>()?;
    scene
        .add(layers)
        .map_err(|error| refused_by_the_scene(&layers_at, items, error))
}

/// The refusal of `Scene::add` as the refusal of the document: an error of a layer (`photo.of`, `photo.look`: its address starts
/// with the layer's name) is at that layer, at its `of` for a reference that finds nothing or leads back to itself; any other
/// (a name used twice, too many layers) is at `layers`.
fn refused_by_the_scene(layers_at: &str, items: &[Value], error: Error) -> Error {
    let Error::Invalid { address, message } = &error else {
        return error;
    };
    let head = address.split('.').next().unwrap_or_default();
    let place = items.iter().enumerate().position(|(index, item)| {
        item.get("name")
            .and_then(Value::as_str)
            .map_or_else(|| format!("layer{}", index + 1), str::to_owned)
            == head
    });
    let Some(place) = place.filter(|_| address != head) else {
        return refused(layers_at, None, error);
    };
    let found_at = escaped(layers_at, &place.to_string());
    if *address == format!("{head}.of") {
        Error::Format {
            pointer: escaped(&found_at, "of"),
            message: message.clone(),
        }
    } else {
        Error::Format {
            pointer: found_at,
            message: format!("{address}: {message}"),
        }
    }
}

fn read_layer(value: &Value, at: &str, base: &Path, kinds: &LayerKinds, depth: usize) -> Result<Layer, Error> {
    too_deep(at, depth)?;
    let map = object(value, at, "a layer is an object")?;
    known_keys(map, at, "a layer", &LAYER_KEYS)?;
    let sources: Vec<&str> = SOURCES.into_iter().filter(|key| map.contains_key(*key)).collect();
    let [source] = sources[..] else {
        return Err(Error::Format {
            pointer: at.to_owned(),
            message: if sources.is_empty() {
                format!("a layer draws something: one of {}", SOURCES.join(", "))
            } else {
                format!("a layer draws one thing, got {}", sources.join(" and "))
            },
        });
    };
    for (key, owner) in OWNED {
        if map.contains_key(key) && owner != source {
            return Err(Error::Format {
                pointer: escaped(at, key),
                message: format!("`{key}` belongs to a layer that draws a {owner}, not a {source}"),
            });
        }
    }
    if map.contains_key("place") && map.contains_key("perspective") {
        return Err(Error::Format {
            pointer: escaped(at, "perspective"),
            message: "a layer has `place` or `perspective`, not both".to_owned(),
        });
    }
    let member = |key: &str| escaped(at, key);
    let mut layer = read_source(map, source, at, base, kinds, depth)?;
    if let Some(fit) = map.get("fit") {
        layer = read_fit(layer, fit, &member("fit"))?;
    }
    if let Some(place) = map.get("place") {
        layer = read_place(layer, place, &member("place"))?;
    }
    if let Some(corners) = map.get("perspective") {
        let at = member("perspective");
        let wanted = "four corners [[x, y], ...]";
        let rows = corners
            .as_array()
            .filter(|rows| rows.len() == 4)
            .ok_or_else(|| wrong(&at, wanted, corners))?;
        let mut quad = [[0.0; 2]; 4];
        for (index, (slot, row)) in quad.iter_mut().zip(rows).enumerate() {
            *slot = numbers(row, &escaped(&at, &index.to_string()), "a corner [x, y]")?;
        }
        layer = layer.perspective(quad).map_err(|error| refused(&at, None, error))?;
    }
    if let Some(masks) = map.get("masks") {
        let at = member("masks");
        let items = masks.as_array().ok_or_else(|| wrong(&at, "a list of masks", masks))?;
        for (index, item) in items.iter().enumerate() {
            layer = read_mask(layer, item, &escaped(&at, &index.to_string()), base, kinds, depth)?;
        }
    }
    if let Some(looks) = map.get("looks") {
        let at = member("looks");
        let items = looks.as_array().ok_or_else(|| wrong(&at, "a list of looks", looks))?;
        for (index, item) in items.iter().enumerate() {
            layer = read_look(layer, item, &escaped(&at, &index.to_string()), base)?;
        }
    }
    if let Some(opacity) = map.get("opacity") {
        let at = member("opacity");
        layer = layer
            .opacity(real(opacity, &at)?)
            .map_err(|error| refused(&at, None, error))?;
    }
    if let Some(mode) = map.get("blend") {
        let at = member("blend");
        let mode =
            Blend::from_str(text(mode, &at, "a blend mode (text)")?).map_err(|error| refused(&at, None, error))?;
        layer = layer.blend(mode);
    }
    if let Some(light) = map.get("light") {
        layer = layer.as_light(flag(light, &member("light"))?);
    }
    if let Some(inks) = map.get("inks") {
        let at = member("inks");
        let items = inks.as_array().ok_or_else(|| wrong(&at, "a list of ink names", inks))?;
        let names = items
            .iter()
            .enumerate()
            .map(|(index, item)| text(item, &escaped(&at, &index.to_string()), "an ink name (text)"))
            .collect::<Result<Vec<_>, _>>()?;
        layer = layer.inks(names).map_err(|error| refused(&at, None, error))?;
    }
    if let Some(name) = map.get("name") {
        let at = member("name");
        layer = layer
            .named(text(name, &at, "a name (text)")?)
            .map_err(|error| refused(&at, None, error))?;
    }
    if let Some(values) = map.get("values") {
        layer = read_values(&layer, values, &member("values"))?;
    }
    Ok(layer)
}

/// The layer that draws what `source` (a key of `map`) says.
fn read_source(
    map: &Map<String, Value>,
    source: &str,
    at: &str,
    base: &Path,
    kinds: &LayerKinds,
    depth: usize,
) -> Result<Layer, Error> {
    let at_source = escaped(at, source);
    let value = &map[source];
    let layer = match source {
        "picture" => {
            let mut layer = if value.is_null() {
                Layer::picture_hole()
            } else {
                Layer::picture_saved(read_file(value, &at_source, base)?)
            };
            if let Some(maps) = map.get("maps") {
                let maps_at = escaped(at, "maps");
                for (name, given) in object(maps, &maps_at, "maps are an object of name and file")? {
                    let file_at = escaped(&maps_at, name);
                    layer = if given.is_null() {
                        layer.map_hole(name)
                    } else {
                        layer.map_saved(name, read_file(given, &file_at, base)?)
                    }
                    .map_err(|error| refused(&file_at, None, error))?;
                }
            }
            layer
        }
        "drawing" => {
            let document = match value {
                Value::String(path) => std::fs::read_to_string(base.join(path)).map_err(|error| Error::Read {
                    path: base.join(path).display().to_string(),
                    message: error.to_string(),
                })?,
                Value::Object(_) => value.to_string(),
                other => {
                    return Err(wrong(
                        &at_source,
                        "a vector document (an object) or the path of a file",
                        other,
                    ));
                }
            };
            Layer::drawing(&document).map_err(|error| refused(at, Some(map), error))?
        }
        "text" => read_text(map, at, base)?,
        "fill" => {
            Layer::fill(text(value, &at_source, "a colour (text)")?).map_err(|error| refused(at, Some(map), error))?
        }
        "scene" => Layer::scene(read_scene(value, &at_source, base, kinds, depth + 1)?),
        "make" => read_make(value, &at_source, kinds)?,
        _ => Layer::of(text(value, &at_source, "the name of a layer (text)")?),
    };
    Ok(layer)
}

/// The layer `{"kind": ..., "options": ...}` names, made through `kinds`.
fn read_make(value: &Value, at: &str, kinds: &LayerKinds) -> Result<Layer, Error> {
    let map = object(value, at, "a made layer is {\"kind\": ..., \"options\": ...}")?;
    known_keys(map, at, "a made layer", &["kind", "options"])?;
    let kind_at = escaped(at, "kind");
    let kind = map.get("kind").ok_or_else(|| Error::Format {
        pointer: at.to_owned(),
        message: "`kind` is missing: the name of a registered kind".to_owned(),
    })?;
    kinds
        .make(
            text(kind, &kind_at, "the name of a kind (text)")?,
            map.get("options").unwrap_or(&Value::Null),
        )
        .map_err(|error| refused(at, Some(map), error))
}

fn read_text(map: &Map<String, Value>, at: &str, base: &Path) -> Result<Layer, Error> {
    let content = text(&map["text"], &escaped(at, "text"), "the words (text)")?;
    let font_at = escaped(at, "font");
    let mut type_set = match map.get("font") {
        Some(Value::String(name)) => Text::new(name),
        Some(file) => Text::from_saved(read_file(file, &font_at, base)?),
        None => {
            return Err(Error::Format {
                pointer: at.to_owned(),
                message: "a text layer has a `font`: a bundled font's name or {\"path\": ..., \"sha256\": ...}"
                    .to_owned(),
            });
        }
    };
    if let Some(color) = map.get("color") {
        type_set = type_set.color(text(color, &escaped(at, "color"), "a colour (text)")?);
    }
    let options = map.get("options");
    if let Some(options) = options {
        for (name, value) in object(options, &escaped(at, "options"), "the options are an object")? {
            type_set = type_set.option(name, value.clone());
        }
    }
    Layer::text(content, &type_set).map_err(|error| match error {
        Error::Invalid { address, message } => {
            let pointer = match address.as_str() {
                "font" => font_at,
                "content" => escaped(at, "text"),
                "color" => escaped(at, "color"),
                option => escaped(&escaped(at, "options"), option),
            };
            Error::Format { pointer, message }
        }
        other => other,
    })
}

fn read_values(layer: &Layer, values: &Value, at: &str) -> Result<Layer, Error> {
    let members = object(values, at, "values are an object of name and number")?;
    let mut set = Vec::with_capacity(members.len());
    for (name, value) in members {
        set.push((
            format!("drawing.{name}"),
            crate::Value::Number(real(value, &escaped(at, name))?),
        ));
    }
    let settings: Vec<(&str, &crate::Value)> = set.iter().map(|(address, value)| (address.as_str(), value)).collect();
    layer
        .with_values(false, &settings)
        .map_err(|error| refused(at, None, error))
}

fn read_fit(layer: Layer, fit: &Value, at: &str) -> Result<Layer, Error> {
    let map = object(fit, at, "a fit is {\"box\": [x, y, width, height]}")?;
    known_keys(map, at, "a fit", &["box", "how", "align"])?;
    let area = map.get("box").ok_or_else(|| Error::Format {
        pointer: at.to_owned(),
        message: "`box` is missing: [x, y, width, height]".to_owned(),
    })?;
    let area = numbers::<4>(area, &escaped(at, "box"), "[x, y, width, height]")?;
    let how = match map.get("how") {
        Some(how) => {
            let how_at = escaped(at, "how");
            Fit::from_str(text(how, &how_at, "\"cover\", \"contain\" or \"stretch\"")?)
                .map_err(|error| refused(&how_at, None, error))?
        }
        None => Fit::Cover,
    };
    let align = match map.get("align") {
        Some(align) => numbers::<2>(align, &escaped(at, "align"), "[x, y]")?,
        None => [0.5, 0.5],
    };
    layer
        .fit(area, how, align)
        .map_err(|error| refused(at, Some(map), error))
}

fn read_place(layer: Layer, place: &Value, at: &str) -> Result<Layer, Error> {
    let map = object(place, at, "a place is an object")?;
    known_keys(map, at, "a place", &PLACE_KEYS)?;
    let mut set = Place::new();
    for (key, value) in map {
        let here = escaped(at, key);
        set = match key.as_str() {
            "dx" => set.dx(real(value, &here)?),
            "dy" => set.dy(real(value, &here)?),
            "scale" => set.scale(real(value, &here)?),
            "aspect" => set.aspect(real(value, &here)?),
            "rotate" => set.rotate(Deg(real(value, &here)?)),
            "skew_x" => set.skew_x(Deg(real(value, &here)?)),
            "skew_y" => set.skew_y(Deg(real(value, &here)?)),
            "flip_x" => set.flip_x(flag(value, &here)?),
            "flip_y" => set.flip_y(flag(value, &here)?),
            _ => {
                let [x, y] = numbers::<2>(value, &here, "[x, y]")?;
                set.anchor(x, y)
            }
        };
    }
    layer.place(set).map_err(|error| refused(at, Some(map), error))
}

fn read_mask(
    layer: Layer,
    value: &Value,
    at: &str,
    base: &Path,
    kinds: &LayerKinds,
    depth: usize,
) -> Result<Layer, Error> {
    let map = object(value, at, "a mask is {\"layer\": ...} or {\"map\": \"name\"}")?;
    known_keys(map, at, "a mask", &["layer", "map", "use", "feather", "invert"])?;
    let by: MaskBy = match (map.get("layer"), map.get("map")) {
        (Some(inner), None) => read_layer(inner, &escaped(at, "layer"), base, kinds, depth + 1)?.into(),
        (None, Some(name)) => MaskBy::from(text(name, &escaped(at, "map"), "the name of a map (text)")?),
        _ => {
            return Err(Error::Format {
                pointer: at.to_owned(),
                message: "a mask is by a `layer` or by a `map`: one of them".to_owned(),
            });
        }
    };
    let mut mask = Mask::new(by);
    if let Some(uses) = map.get("use") {
        let use_at = escaped(at, "use");
        mask = mask.using(
            Use::from_str(text(uses, &use_at, "\"alpha\" or \"luminance\"")?)
                .map_err(|error| refused(&use_at, None, error))?,
        );
    }
    if let Some(feather) = map.get("feather") {
        mask = mask.feather(real(feather, &escaped(at, "feather"))?);
    }
    if let Some(invert) = map.get("invert")
        && flag(invert, &escaped(at, "invert"))?
    {
        mask = mask.invert();
    }
    layer.mask(mask).map_err(|error| refused(at, Some(map), error))
}

fn read_look(layer: Layer, value: &Value, at: &str, base: &Path) -> Result<Layer, Error> {
    if let Value::String(path) = value {
        return layer.look_file(base.join(path));
    }
    let map = object(
        value,
        at,
        "a look is {\"format\": n, \"recipe\": {...}} or the path of a recipe file",
    )?;
    known_keys(map, at, "a look", &["format", "recipe", "px"])?;
    let px = match map.get("px") {
        None => Px::Design,
        Some(Value::String(word)) => word.parse().map_err(|_| Error::Format {
            pointer: escaped(at, "px"),
            message: format!("`px` is \"design\" or \"canvas\", got {word:?}"),
        })?,
        Some(other) => return Err(wrong(&escaped(at, "px"), "\"design\" or \"canvas\"", other)),
    };
    let recipe_at = escaped(at, "recipe");
    let recipe = map.get("recipe").ok_or_else(|| Error::Format {
        pointer: at.to_owned(),
        message: "`recipe` is missing".to_owned(),
    })?;
    let mut document = object(recipe, &recipe_at, "a recipe is an object")?.clone();
    if let Some(format) = map.get("format") {
        document.insert("format".to_owned(), format.clone());
    }
    layer
        .look_in(px, &Value::Object(document).to_string())
        .map_err(|error| match error {
            Error::Format { pointer, message } => Error::Format {
                pointer: if pointer == "/look/format" {
                    escaped(at, "format")
                } else {
                    recipe_at.clone()
                },
                message,
            },
            Error::Invalid { message, .. } => Error::Format {
                pointer: recipe_at.clone(),
                message,
            },
            other => other,
        })
}
