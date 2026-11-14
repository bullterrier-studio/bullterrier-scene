//! Type as a layer: the options of a text, as plain data the linked vector crate checks.

use std::path::PathBuf;

use serde_json::Value;

use crate::document::number;
use crate::files::Located;

/// The font of a text: a bundled one by name, or a font file.
#[derive(Debug, Clone)]
pub(crate) enum Face {
    Bundled(String),
    File(Located),
}

/// What a text layer says about its type besides the words: the font, the colour and vector's own text options
/// (`size`, `axes`, `tracking`, `leading`, `align`, `width`, `fit`, `features`), each plain data. Give one to
/// [`Layer::text`](crate::Layer::text), which has vector check it; scene restates none of vector's defaults.
#[derive(Debug, Clone)]
#[must_use]
pub struct Text {
    pub(crate) face: Face,
    pub(crate) color: String,
    pub(crate) options: Vec<(String, Value)>,
}

impl Text {
    /// Type set in the bundled font called `font` (`bricolage_grotesque`, `fraunces`...: an unknown name is refused when the
    /// layer is made, listing the names), in black.
    pub fn new(font: &str) -> Self {
        Self {
            face: Face::Bundled(font.to_owned()),
            color: "#000000".to_owned(),
            options: Vec::new(),
        }
    }

    /// Type set in the font file `path` (TrueType or OpenType), read when the layer is made, in black.
    pub fn from_file(path: impl Into<PathBuf>) -> Self {
        Self {
            face: Face::File(Located::new(path)),
            color: "#000000".to_owned(),
            options: Vec::new(),
        }
    }

    /// Type set in the font file of a saved scene.
    pub(crate) fn from_saved(file: Located) -> Self {
        Self {
            face: Face::File(file),
            color: "#000000".to_owned(),
            options: Vec::new(),
        }
    }

    /// The colour of the letters: `#rrggbb` or `#rrggbbaa` (default `#000000`).
    pub fn color(mut self, color: &str) -> Self {
        color.clone_into(&mut self.color);
        self
    }

    /// Sets the vector text option `name` to `value` (a number, text, or for `axes` and `features` an object of numbers),
    /// replacing an earlier value of that name. Vector checks it, with its own words, when the layer is made. `anchor` and
    /// `inside` are refused (scene places the text: its origin is the anchor, and `place` moves it), and so are expressions.
    pub fn option(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.options.retain(|(known, _)| known != name);
        self.options.push((name.to_owned(), whole(value.into())));
        self
    }
}

/// `value` with each whole number written whole (`84.0` as `84`), as the document writes it: a number is one number, however
/// the caller spelt it, so that equal text is equal in `==` and in the saved scene.
fn whole(value: Value) -> Value {
    match value {
        Value::Number(found) if found.is_f64() => found.as_f64().map_or(Value::Number(found), number),
        Value::Object(entries) => {
            Value::Object(entries.into_iter().map(|(name, value)| (name, whole(value))).collect())
        }
        other => other,
    }
}
