//! The WebAssembly package of bullterrier-scene, for the browser (a Web Worker) and Node: a saved scene read with the bytes
//! of the files it names, its parameters listed and set, and rendered to 8-bit RGBA. The core runs unchanged (one thread,
//! no GPU), so a render looks the same as the native render of the same size; this file only converts between
//! JavaScript's values and the core's, and gives each core error its JavaScript `Error`.

use bullterrier_scene as core;
use js_sys::{Array, Error, JSON, Object, Reflect, TypeError, Uint8Array, Uint8ClampedArray};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
/**
 * What a scene is read with: `files` are the bytes of each file the document names, by the path exactly as it writes it;
 * `cacheBytes` is the most the cache of this scene and the scenes made from it keeps (default 256 MiB, 0 keeps nothing).
 */
export interface SceneOptions {
    files?: Record<string, Uint8Array>;
    cacheBytes?: number;
}

/** What a render is asked for: `size` is the long side in pixels, a whole number from 1 to 65,000 (default: the canvas's). */
export interface RenderOptions {
    size?: number;
}

/** A render: `data` is the 8-bit RGBA bytes of `new ImageData(data, width, height)`, straight alpha. */
export interface Rendered {
    width: number;
    height: number;
    data: Uint8ClampedArray;
}

/**
 * A value of the scene that can be set, by address (`scene.parameters()`, `scene.withValues`), in the keys of
 * `bt-scene parameters --json` plus `value`, the value the scene holds now (a picture's: the path as the scene writes it,
 * or `null` for a hole). `choices`, `min_length` and `nullable` are there only for a choice, a text with a least length and
 * a value that may be left out.
 */
export interface SceneParameter {
    address: string;
    kind: string;
    meaning: string;
    default: number | boolean | string | number[] | null;
    unit: string | null;
    min: number | null;
    max: number | null;
    cyclic: boolean;
    value: number | boolean | string | number[] | null;
    choices?: string[];
    min_length?: number;
    nullable?: boolean;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// A saved scene's JSON text.
    #[wasm_bindgen(typescript_type = "string")]
    pub type Text;

    /// What a scene is read with.
    #[wasm_bindgen(typescript_type = "SceneOptions")]
    pub type SceneOptions;

    /// What a render is asked for.
    #[wasm_bindgen(typescript_type = "RenderOptions")]
    pub type RenderOptions;

    /// The layers of a scene, as the document writes them.
    #[wasm_bindgen(typescript_type = "object[]")]
    pub type Layers;

    /// What new layers are read with: the files they name.
    #[wasm_bindgen(typescript_type = "Pick<SceneOptions, \"files\">")]
    pub type LayersOptions;

    /// A pixel's column or row.
    #[wasm_bindgen(typescript_type = "number")]
    pub type Whole;

    /// The values to set, by address.
    #[wasm_bindgen(typescript_type = "Record<string, number | boolean | string | number[]>")]
    pub type Values;
}

/// The `Error` a core error becomes: its `name` is the Python package's exception (`InvalidError`, `FormatError`,
/// `ReadError`, else `BullterrierSceneError`), its message the core's, and it carries the variant's field (`address`,
/// `pointer`, `path`).
fn thrown_for(error: &core::Error) -> JsValue {
    let (name, field) = match error {
        core::Error::Invalid { address, .. } => ("InvalidError", Some(("address", address))),
        core::Error::Format { pointer, .. } => ("FormatError", Some(("pointer", pointer))),
        core::Error::Read { path, .. } => ("ReadError", Some(("path", path))),
        _ => ("BullterrierSceneError", None),
    };
    let thrown = Error::new(&error.to_string());
    thrown.set_name(name);
    if let Some((key, value)) = field
        && let Err(failure) = Reflect::set(&thrown, &key.into(), &value.into())
    {
        return failure;
    }
    thrown.into()
}

/// Run a core call and throw its error as the matching `Error`.
fn raised<T>(result: Result<T, core::Error>) -> Result<T, JsValue> {
    result.map_err(|error| thrown_for(&error))
}

/// The `TypeError` for a value that is not what a call reads.
fn refuse(what: &str, expected: &str, value: &JsValue) -> JsValue {
    let found = value.js_typeof().as_string().unwrap_or_default();
    TypeError::new(&format!("{what}: expected {expected}, got a {found}")).into()
}

/// The object `given` is (`what` names it), or a `TypeError`; `undefined` is no object given.
fn object_of<'a>(given: Option<&'a JsValue>, what: &str, expected: &str) -> Result<Option<&'a Object>, JsValue> {
    match given {
        None => Ok(None),
        Some(value) if value.is_undefined() => Ok(None),
        Some(value) => value
            .dyn_ref::<Object>()
            .filter(|_| value.is_object() && !Array::is_array(value))
            .map(Some)
            .ok_or_else(|| refuse(what, expected, value)),
    }
}

/// The own keys of `object` with their values.
fn entries(object: &Object) -> impl Iterator<Item = (String, JsValue)> {
    Object::entries(object).into_iter().map(|pair| {
        let pair: Array = pair.unchecked_into();
        (pair.get(0).as_string().unwrap_or_default(), pair.get(1))
    })
}

/// A member of `object`, `undefined` when it has none.
fn member(object: &Object, key: &str) -> Result<JsValue, JsValue> {
    Reflect::get(object, &key.into())
}

/// `given` as a whole number (`what` names it), or a `TypeError`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a whole number: cut to i128 it is the same number, or one past any size or pixel the core takes, which it refuses"
)]
fn whole(what: &str, expected: &str, given: &JsValue) -> Result<i128, JsValue> {
    given
        .as_f64()
        .filter(|number| number.is_finite() && number.fract() == 0.0)
        .map(|number| number as i128)
        .ok_or_else(|| refuse(what, expected, given))
}

/// The files `options.files` gives: the bytes of each by the path as the document writes it.
fn files_of(options: &Object) -> Result<core::Files, JsValue> {
    let mut files = core::Files::given();
    let given = member(options, "files")?;
    if let Some(given) = object_of(Some(&given), "files", "an object of paths to Uint8Arrays")? {
        for (path, bytes) in entries(given) {
            let bytes = bytes
                .dyn_ref::<Uint8Array>()
                .ok_or_else(|| refuse(&format!("files.{path}"), "a Uint8Array", &bytes))?;
            files = files.with(path, bytes.to_vec());
        }
    }
    Ok(files)
}

/// The long side `options.size` asks for, or the canvas's long side.
fn size_of(scene: &core::Scene, options: Option<RenderOptions>) -> Result<i128, JsValue> {
    let options = options.map(JsValue::from);
    if let Some(options) = object_of(options.as_ref(), "options", "an object")? {
        let given = member(options, "size")?;
        if !given.is_undefined() {
            return whole("size", "a whole number of pixels", &given);
        }
    }
    Ok(i128::from(scene.width().max(scene.height())))
}

/// The value a JavaScript value sets at `address`: a number, a boolean, a text, or three numbers (a colour).
fn value_of(address: &str, value: &JsValue) -> Result<core::Value, JsValue> {
    if let Some(number) = value.as_f64() {
        return Ok(core::Value::Number(number));
    }
    if let Some(on) = value.as_bool() {
        return Ok(core::Value::Bool(on));
    }
    if let Some(text) = value.as_string() {
        return Ok(core::Value::Text(text));
    }
    if Array::is_array(value) {
        let channels: Option<Vec<f64>> = Array::from(value).iter().map(|channel| channel.as_f64()).collect();
        if let Some(Ok(rgb)) = channels.map(<[f64; 3]>::try_from) {
            return Ok(core::Value::Rgb(rgb));
        }
    }
    Err(refuse(
        &format!("values.{address}"),
        "a number, a boolean, a text or three numbers (a colour)",
        value,
    ))
}

/// A value as JSON, as `bt-scene parameters --json` writes a default: a file is its path as the scene writes it, a picture
/// or map held in memory and a hole are `null`.
fn plain(value: &core::Value) -> serde_json::Value {
    match value {
        core::Value::Number(number) => serde_json::json!(number),
        core::Value::Bool(on) => serde_json::json!(on),
        core::Value::Text(text) => serde_json::json!(text),
        core::Value::Rgb(channels) => serde_json::json!(channels),
        core::Value::File(path) => serde_json::json!(path.display().to_string()),
        _ => serde_json::Value::Null,
    }
}

/// A parameter in the keys of `bt-scene parameters --json`, plus `value`.
fn described(parameter: &core::Parameter) -> serde_json::Value {
    let mut written = serde_json::Map::new();
    written.insert("address".to_owned(), serde_json::json!(parameter.address));
    written.insert("kind".to_owned(), serde_json::json!(parameter.kind.name()));
    written.insert("meaning".to_owned(), serde_json::json!(parameter.meaning));
    written.insert("default".to_owned(), plain(&parameter.default));
    written.insert("unit".to_owned(), serde_json::json!(parameter.unit));
    written.insert("min".to_owned(), serde_json::json!(parameter.min));
    written.insert("max".to_owned(), serde_json::json!(parameter.max));
    written.insert("cyclic".to_owned(), serde_json::json!(parameter.cyclic));
    written.insert("value".to_owned(), plain(&parameter.value));
    if !parameter.choices.is_empty() {
        written.insert("choices".to_owned(), serde_json::json!(parameter.choices));
    }
    if let Some(least) = parameter.min_length {
        written.insert("min_length".to_owned(), serde_json::json!(least));
    }
    if parameter.nullable {
        written.insert("nullable".to_owned(), serde_json::json!(true));
    }
    serde_json::Value::Object(written)
}

/// The 8-bit level of `level` (0..1), rounded as the core's PNG writer rounds: `level * 255 + 0.5`, cut to a whole number.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a level is 0..1, so the sum is 0.5..255.5 and cutting it is the rounding"
)]
fn eight_bit(level: f32) -> u8 {
    (level * 255.0 + 0.5) as u8
}

/// The most bytes the cache of a scene read here keeps unless `cacheBytes` says: wasm memory never shrinks and ends at 4 GiB
/// (a phone's tab far sooner), against the native 2 GiB. Measured with demo 57 (`docs/javascript-api.md`, "Memory").
const CACHE_BYTES: usize = 256 << 20;

/// A scene: a canvas and its layers, as a saved scene writes them.
#[wasm_bindgen(js_name = Scene)]
pub struct Scene(core::Scene);

#[wasm_bindgen(js_class = Scene)]
impl Scene {
    /// The scene a saved scene's JSON `text` writes (`toJson` writes one), with `options.files` the bytes of each file it
    /// names (a picture, a map, a font, a look, a drawing), by the path exactly as the document writes it. A file the
    /// document names is read from those bytes when the scene is read, never from a disk. A layer of a plugin's kind
    /// (`make`) is refused: the package has none. `options.cacheBytes` bounds the cache this scene and the scenes made from it
    /// share (256 MiB unless given).
    ///
    /// # Errors
    /// A `TypeError` naming `text` unless it is a string, `options` or `files` unless each is an object, or `files.<path>` for bytes that are no
    /// `Uint8Array`, `cacheBytes` unless it is a whole number from 0 to 2^32 - 1; `FormatError` (with `pointer`, the JSON Pointer of the value) for text that is no saved scene;
    /// `ReadError` (with `path`) for a file the document names that `files` lacks; `InvalidError` (with `address`) for a
    /// value the scene refuses.
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(text: &Text, options: Option<SceneOptions>) -> Result<Scene, JsValue> {
        let given: &JsValue = text.as_ref();
        let text = given.as_string().ok_or_else(|| refuse("text", "a string", given))?;
        let options = options.map(JsValue::from);
        let mut files = core::Files::given();
        let mut cache_bytes = CACHE_BYTES;
        if let Some(options) = object_of(options.as_ref(), "options", "an object")? {
            let given = member(options, "cacheBytes")?;
            if !given.is_undefined() {
                let expected = "a whole number of bytes from 0 to 2^32 - 1";
                cache_bytes = usize::try_from(whole("cacheBytes", expected, &given)?)
                    .map_err(|_| refuse("cacheBytes", expected, &given))?;
            }
            files = files_of(options)?;
        }
        raised(core::Scene::from_json_with(&text, &files, &core::LayerKinds::new()))
            .map(|scene| Scene(scene.with_cache_budget(cache_bytes)))
    }

    /// The canvas's width in design pixels.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn width(&self) -> u32 {
        self.0.width()
    }

    /// The canvas's height in design pixels.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn height(&self) -> u32 {
        self.0.height()
    }

    /// Every value of the scene that can be set, in scene order: what a page needs for a control each.
    ///
    /// # Errors
    /// None in practice: a `SyntaxError` only if the list could not be handed to JavaScript.
    #[wasm_bindgen(unchecked_return_type = "SceneParameter[]")]
    pub fn parameters(&self) -> Result<JsValue, JsValue> {
        let listed: Vec<_> = self.0.parameters().iter().map(described).collect();
        JSON::parse(&serde_json::Value::Array(listed).to_string())
    }

    /// A new scene with the value at each address of `values` set: a number, a boolean, a text (a colour as `#rrggbb`, a
    /// choice's name) or three numbers from 0 to 1 (a look's colour). All or nothing; this scene is left as it is, and the
    /// two share what was drawn, so a render after a small change draws only what it changed.
    ///
    /// # Errors
    /// A `TypeError` naming `values` unless it is an object, or `values.<address>` for a value of another type;
    /// `InvalidError` (with `address`) for an address the scene lacks or a value it refuses.
    #[wasm_bindgen(js_name = withValues)]
    pub fn with_values(&self, values: &Values) -> Result<Scene, JsValue> {
        let given: &JsValue = values.as_ref();
        let object = object_of(Some(given), "values", "an object of addresses to values")?
            .ok_or_else(|| refuse("values", "an object of addresses to values", given))?;
        let mut set = core::Values::new();
        for (address, value) in entries(object) {
            let value = value_of(&address, &value)?;
            set = set.set(address, value);
        }
        raised(self.0.with_values(&set)).map(Scene)
    }

    /// The picture of the scene at `options.size`, the long side in pixels (default: the canvas's long side); the canvas
    /// keeps its aspect and the short side is rounded up. Straight alpha, transparent where nothing is drawn, sRGB values,
    /// each level rounded to 8 bits as a PNG keeps it: it looks the same as the native render of the same size.
    ///
    /// # Errors
    /// A `TypeError` naming `options` unless it is an object, or `size` unless it is a whole number; `InvalidError` (with
    /// `address` `size`) for a size outside 1 to 65,000 or a picture of more than 2^27 pixels, or naming a layer whose look
    /// or drawing refuses this size; `ReadError` for a file that is not the one the scene was saved with.
    #[wasm_bindgen(unchecked_return_type = "Rendered")]
    #[expect(
        clippy::cast_precision_loss,
        reason = "a side is at most 65,000 pixels, far inside f64's 53 bits"
    )]
    pub fn render(&self, options: Option<RenderOptions>) -> Result<JsValue, JsValue> {
        let picture = raised(self.0.render(size_of(&self.0, options)?))?;
        let bytes: Vec<u8> = picture.pixels().iter().map(|&level| eight_bit(level)).collect();
        // a plain object holding a copy in JavaScript's memory: nothing to free, and the array is the same on every read
        let rendered = Object::new();
        Reflect::set(&rendered, &"width".into(), &JsValue::from_f64(picture.width() as f64))?;
        Reflect::set(&rendered, &"height".into(), &JsValue::from_f64(picture.height() as f64))?;
        Reflect::set(&rendered, &"data".into(), &Uint8ClampedArray::from(bytes.as_slice()))?;
        Ok(rendered.into())
    }

    /// The address of the topmost layer that shows at the pixel (`x`, `y`) of `render(options)`'s picture (column and row from
    /// the top left), or `null` where none does: the start of its `parameters()` addresses (its name, `layer<n>` for one
    /// without, `group.scene.inner` inside a group). A layer shows where, drawn alone, it leaves alpha an 8-bit picture keeps;
    /// a blend mode changes its colour, not where it is; at opacity 0 or masked away it is not there.
    ///
    /// # Errors
    /// A `TypeError` naming `x` or `y` unless each is a whole number, and as `render` for `options`; `InvalidError` (with
    /// `address` `x` or `y`) for a pixel outside the picture, and as `render`.
    #[wasm_bindgen(js_name = layerAt, unchecked_return_type = "string | null")]
    pub fn layer_at(&self, x: &Whole, y: &Whole, options: Option<RenderOptions>) -> Result<JsValue, JsValue> {
        let x = whole("x", "a whole number, a column of the picture", x.as_ref())?;
        let y = whole("y", "a whole number, a row of the picture", y.as_ref())?;
        let found = raised(self.0.layer_at(x, y, size_of(&self.0, options)?))?;
        // `null`, not the `undefined` an `Option` becomes
        Ok(found.map_or(JsValue::NULL, JsValue::from))
    }

    /// A new scene of this canvas with `layers` as its stack, bottom first, each a layer as the document writes it (an item of
    /// `JSON.parse(scene.toJson()).layers`, changed, moved, left out or new; a group is a layer with a `scene`), with
    /// `options.files` the bytes of each file they name, as `fromJson` takes them. It shares this scene's cache: a layer that
    /// did not change is not drawn again. A named layer keeps its addresses wherever it goes. This scene is left as it is.
    ///
    /// # Errors
    /// A `TypeError` naming `layers` unless it is an array, and as `fromJson` for `options`; `FormatError` (with `pointer`,
    /// from `/layers/<index>`) for an item that is no layer; `ReadError` and `InvalidError` as `fromJson`.
    #[wasm_bindgen(js_name = withLayers)]
    pub fn with_layers(&self, layers: &Layers, options: Option<LayersOptions>) -> Result<Scene, JsValue> {
        let given: &JsValue = layers.as_ref();
        if !Array::is_array(given) {
            return Err(refuse(
                "layers",
                "an array of layers as the document writes them",
                given,
            ));
        }
        let options = options.map(JsValue::from);
        let files = match object_of(options.as_ref(), "options", "an object")? {
            Some(options) => files_of(options)?,
            None => core::Files::given(),
        };
        // the document of this canvas, as the core writes it, holding the given layers
        let mut document: serde_json::Value = serde_json::from_str(&raised(
            core::Scene::new(self.0.width(), self.0.height()).and_then(|empty| empty.to_json()),
        )?)
        .map_err(|error| JsValue::from(Error::new(&error.to_string())))?;
        let text = JSON::stringify(given)?.as_string().unwrap_or_default();
        document["layers"] =
            serde_json::from_str(&text).map_err(|error| JsValue::from(Error::new(&error.to_string())))?;
        let read = raised(core::Scene::from_json_with(
            &document.to_string(),
            &files,
            &core::LayerKinds::new(),
        ))?;
        raised(self.0.with_layers(read.layers().iter().cloned())).map(Scene)
    }

    /// The scene as a saved scene's JSON: `fromJson` reads it back, with the same files. Each file is written by its path
    /// and the SHA-256 of its bytes.
    ///
    /// # Errors
    /// `ReadError` for a file the scene names that cannot be read to take its digest.
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, JsValue> {
        raised(self.0.to_json())
    }
}
