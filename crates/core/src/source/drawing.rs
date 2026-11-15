//! A vector drawing as a layer, and the one place that knows how the vector crate reads and draws a document.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use bullterrier_vector::{Font, Inputs, Param, ScaleBy, Size, Unit, Values, Window};
use serde_json::{Value, json};

use super::{Drawn, Opened, Source};
use crate::cache::{Cache, Keyed};
use crate::document::{Form, number};
use crate::files::{Fingerprint, digest_bytes};
use crate::geometry::Rect;
use crate::parameters::{Parameter, Value as Setting};
use crate::text::{Face, Text};
use crate::{Error, Raster};

/// A drawing with a frame. Its box is the frame, one design pixel to a pixel of the frame, at the canvas origin or, for a
/// text, where the text's anchor makes it.
#[derive(Debug, Clone)]
pub(crate) struct Drawing {
    document: bullterrier_vector::Drawing,
    /// The Params the document reads, in the order it first uses them.
    params: Vec<Param>,
    /// The number the layer holds for each Param it was given one for; the others take their default.
    values: BTreeMap<String, f64>,
    /// What a text layer was made from, to make it again when its content or colour is set.
    typeset: Option<Typeset>,
    /// The digest of the document, made when first asked for: a document never changes in a drawing (a value or a rebuilt text
    /// makes another), so it is kept through the copies a value makes.
    fingerprint: OnceLock<Fingerprint>,
    /// The digest of the font file a text was set in: the bytes are read when the layer is made and are not in the document.
    font: Option<Fingerprint>,
    /// The top left of the box in the layer's space: `(0, 0)` for a drawing, the glyphs' bounds for a text.
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// A text layer's content and type, as `Layer::text` was given them.
#[derive(Debug, Clone)]
struct Typeset {
    content: String,
    text: Text,
}

/// `error` of the vector crate as scene's: a document it cannot read keeps its pointer under `/drawing`, anything else is a
/// refused value at `drawing`.
fn refused(error: &bullterrier_vector::Error) -> Error {
    match error {
        bullterrier_vector::Error::Format { pointer, message } => Error::Format {
            pointer: format!("/drawing{pointer}"),
            message: format!("vector document: {message}"),
        },
        other => Error::invalid("drawing", other.to_string()),
    }
}

impl Drawing {
    /// The drawing the JSON document `json` describes, checked now by the vector crate.
    pub(crate) fn parse(json: &str) -> Result<Self, Error> {
        let document = bullterrier_vector::Drawing::from_json(json, &Inputs::new()).map_err(|error| refused(&error))?;
        let (width, height) = document.size().ok_or_else(|| {
            Error::invalid(
                "drawing",
                "a drawing layer needs a frame: build the drawing with a `size`",
            )
        })?;
        Ok(Self {
            params: document.parameters().map_err(|error| refused(&error))?,
            document,
            values: BTreeMap::new(),
            typeset: None,
            fingerprint: OnceLock::new(),
            font: None,
            x: 0.0,
            y: 0.0,
            width,
            height,
        })
    }

    /// `content` set in the type `text` says: a vector text drawing whose frame is the glyphs' bounds, and whose box sits where the text's anchor (the left end, the middle or the right end of the first
    /// baseline, by `align`) is the layer's origin.
    pub(crate) fn text(content: &str, text: &Text) -> Result<Self, Error> {
        for (name, value) in &text.options {
            check_option(name, value)?;
        }
        let (font, inputs, font_digest) = match &text.face {
            Face::Bundled(name) => (
                Font::bundled(name).map_err(|error| Error::invalid("font", error.to_string()))?,
                Inputs::new(),
                None,
            ),
            Face::File(file) => {
                let bytes = file.bytes().map_err(|error| match error {
                    Error::Read { path, message } => Error::Read {
                        path,
                        message: format!("the font cannot be read: {message}"),
                    },
                    other => other,
                })?;
                let font = Font::new(bytes.clone())
                    .and_then(|font| font.named("font"))
                    .map_err(|error| Error::invalid("font", format!("{}: {error}", file.resolved().display())))?;
                (
                    font,
                    Inputs::new().set("font", bytes.clone()),
                    Some(digest_bytes(&bytes)),
                )
            }
        };
        // vector writes the document of a text in this font; the options are put in as the plain data they are and vector's
        // reader checks them, so scene restates none of its arguments or defaults
        let skeleton = bullterrier_vector::text(content, bullterrier_vector::Text::new(&font))
            .to_json()
            .map_err(|error| text_refused(&error))?;
        let mut document: Value =
            serde_json::from_str(&skeleton).map_err(|error| Error::invalid("text", error.to_string()))?;
        let missing = || Error::invalid("text", "vector wrote a text drawing in a form this scene does not know");
        let node = document
            .pointer_mut("/root/0/text")
            .and_then(Value::as_object_mut)
            .ok_or_else(missing)?;
        for (name, value) in &text.options {
            node.insert(name.clone(), value.clone());
        }
        document
            .pointer_mut("/root")
            .and_then(Value::as_array_mut)
            .ok_or_else(missing)?
            .push(json!({"fill": {"color": text.color}}));
        let read = |document: &Value| {
            bullterrier_vector::Drawing::from_json(&document.to_string(), &inputs).map_err(|error| text_refused(&error))
        };
        // the bounds of the glyphs with the anchor at the origin, then the same text with its anchor where those bounds
        // start, so that its frame is the bounds
        let bounds = read(&document)?.bounds(&Values::new()).map_err(|_| {
            Error::invalid(
                "content",
                "the text has no glyph to draw: its content is empty, or only spaces",
            )
        })?;
        let (x, y, width, height) = (bounds.x, bounds.y, bounds.width, bounds.height);
        if !(width > 0.0 && height > 0.0) {
            return Err(Error::invalid(
                "content",
                "the text has no extent to draw: its glyphs have no width or no height",
            ));
        }
        document
            .pointer_mut("/root/0/text")
            .and_then(Value::as_object_mut)
            .ok_or_else(missing)?
            .insert("anchor".to_owned(), json!([-x, -y]));
        Ok(Self {
            document: read(&document)?.with_size(width, height),
            params: Vec::new(),
            values: BTreeMap::new(),
            typeset: Some(Typeset {
                content: content.to_owned(),
                text: text.clone(),
            }),
            fingerprint: OnceLock::new(),
            font: font_digest,
            x,
            y,
            width,
            height,
        })
    }
}

/// `error` of the vector crate as a refused text option: the last part of its pointer (`/root/0/text/size`) names the option.
fn text_refused(error: &bullterrier_vector::Error) -> Error {
    match error {
        bullterrier_vector::Error::Format { pointer, message } => Error::invalid(
            pointer
                .rsplit('/')
                .next()
                .filter(|last| !last.is_empty())
                .unwrap_or("text"),
            message.clone(),
        ),
        other => Error::invalid("text", other.to_string()),
    }
}

/// An option scene does not pass on: the ones it owns or places itself, and anything that is not plain data (an expression, a
/// Param, a list).
fn check_option(name: &str, value: &Value) -> Result<(), Error> {
    let refusal = match name {
        "content" | "font" => Some("scene owns the content and the font: they are arguments of the layer"),
        "anchor" => Some("scene places the text: its origin is its anchor, and `place` moves it"),
        "inside" | "runs" => Some(
            "a text in a shape or in runs is a drawing: build it with vector and use a drawing layer (`Layer.drawing`)",
        ),
        _ => None,
    };
    if let Some(reason) = refusal {
        return Err(Error::invalid(name, reason));
    }
    let numbers = |entries: &serde_json::Map<String, Value>| entries.values().all(Value::is_number);
    let plain = match value {
        Value::Number(_) | Value::String(_) => true,
        Value::Object(entries) => matches!(name, "axes" | "features") && numbers(entries),
        _ => false,
    };
    if plain {
        Ok(())
    } else {
        Err(Error::invalid(
            name,
            format!(
                "a text option is a number or a text (`axes` and `features` an object of numbers): expressions and Params are not taken, got {value}"
            ),
        ))
    }
}

/// The unit of a Param as the parameter format spells it. The format has no unit for a plain number (a font axis, a seed), and a
/// `float` needs one: it is a `count`.
fn unit_name(unit: Unit) -> &'static str {
    match unit {
        Unit::None => "count",
        Unit::Px => "px",
        Unit::Deg => "degrees",
        Unit::Cycle => "cycles",
        Unit::Fraction => "fraction",
    }
}

impl Source for Drawing {
    /// `text`, `font`, `color` and `options` for a text layer; `drawing` (vector's own canonical document) and `values` (the
    /// Params that were set) for any other.
    fn document(&self, form: Form) -> Result<Vec<(&'static str, Value)>, Error> {
        if let Some(typeset) = &self.typeset {
            let font = match &typeset.text.face {
                Face::Bundled(name) => Value::String(name.clone()),
                Face::File(file) => form.file(file)?,
            };
            let mut found = vec![
                ("text", Value::String(typeset.content.clone())),
                ("font", font),
                ("color", Value::String(typeset.text.color.clone())),
            ];
            if !typeset.text.options.is_empty() {
                found.push(("options", Value::Object(typeset.text.options.iter().cloned().collect())));
            }
            return Ok(found);
        }
        let text = self.document.to_json().map_err(|error| refused(&error))?;
        let document = serde_json::from_str(&text).map_err(|error| Error::invalid("drawing", error.to_string()))?;
        let mut found = vec![("drawing", document)];
        if !self.values.is_empty() {
            let values = self.values.iter().map(|(name, value)| (name.clone(), number(*value)));
            found.push(("values", Value::Object(values.collect())));
        }
        Ok(found)
    }

    /// The document, the font file of a text, the Params that were set and the box: what a drawing is drawn from.
    fn keyed(&self, key: &mut Keyed) -> Result<(), Error> {
        let fingerprint = if let Some(found) = self.fingerprint.get() {
            *found
        } else {
            let text = self.document.to_json().map_err(|error| refused(&error))?;
            *self.fingerprint.get_or_init(|| digest_bytes(text.as_bytes()))
        };
        key.text("drawing").digest(&fingerprint).debug(&(
            self.font,
            &self.values,
            self.x,
            self.y,
            self.width,
            self.height,
        ));
        Ok(())
    }

    /// A drawing is drawn at the fraction of a pixel its box starts at.
    fn follows_grid(&self) -> bool {
        true
    }

    /// A drawing takes vector's drawing of its shapes.
    fn is_costly(&self) -> bool {
        true
    }

    /// `text` and `color` for a text layer; for any drawing `drawing.<param>` for each Param, as vector describes it: a cyclic
    /// one (degrees, cycles) spans one turn.
    fn parameters(&self) -> Vec<Parameter> {
        let mut found = Vec::new();
        if let Some(typeset) = &self.typeset {
            found.push(Parameter::text("text", "the words", &typeset.content));
            found.push(Parameter::color(
                "color",
                "the colour of the letters",
                &typeset.text.color,
            ));
        }
        for param in &self.params {
            let unit = param.unit_of();
            found.push(Parameter::number(
                &format!("drawing.{}", param.name()),
                param.meaning_of(),
                unit_name(unit),
                (param.min(), param.max()),
                (false, matches!(unit, Unit::Deg | Unit::Cycle)),
                param.default(),
                self.values.get(param.name()).copied().unwrap_or(param.default()),
            ));
        }
        found
    }

    fn with_values(&self, values: &[(&str, &Setting)]) -> Result<Arc<dyn Source>, Error> {
        let mut next = self.clone();
        let mut again = self.typeset.clone();
        for &(address, value) in values {
            match (address, again.as_mut()) {
                ("text", Some(typeset)) => {
                    let content = value.text(address)?;
                    if content.is_empty() {
                        return Err(Error::invalid(
                            address,
                            "at least 1 character is wanted, got an empty text",
                        ));
                    }
                    content.clone_into(&mut typeset.content);
                }
                ("color", Some(typeset)) => value.text(address)?.clone_into(&mut typeset.text.color),
                _ => {
                    // a Param, as `parameters` lists it
                    if let Some(param) = address
                        .strip_prefix("drawing.")
                        .and_then(|name| self.params.iter().find(|param| param.name() == name))
                    {
                        let accepted = param
                            .accept(value.number(address)?)
                            .map_err(|error| Error::invalid(address, error.to_string()))?;
                        next.values.insert(param.name().to_owned(), accepted);
                    }
                }
            }
        }
        if let Some(typeset) = again {
            let rebuilt = Self::text(&typeset.content, &typeset.text).map_err(|error| match &error {
                Error::Invalid { address, .. } if address == "content" => error.about("text"),
                _ => error,
            })?;
            return Ok(Arc::new(rebuilt));
        }
        Ok(Arc::new(next))
    }

    /// The whole frame at the asked scale, or the rows of it a band reads (`Request::rows`): vector draws a window that is the
    /// crop of the whole render.
    ///
    /// Vector draws a frame at one scale, centred, in a raster of whole pixels, which leaves a fraction of a pixel of slack along
    /// the axis that does not bind, and cannot draw a frame at two scales. So the raster is made to the pixel: the drawing's
    /// shapes are scaled along x and y to the density the layer is drawn at, moved by the fractions of a pixel that
    /// `Request::phase` says, and put in a frame that is exactly the raster at one scale. The raster then covers the box
    /// from `phase` before its top left, at that density; the warp copies it onto whole pixels and cuts the box out of it
    /// by coverage, so an edge on a pixel boundary is hard and `stretch` stretches.
    fn open(&self, _canvas: Rect, _cache: &Cache) -> Result<Opened<'_>, Error> {
        let own = Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        };
        let given = self
            .values
            .iter()
            .fold(Values::new(), |given, (name, value)| given.set(name.clone(), *value));
        let opened = Opened::new(own, move |request, _| {
            // output pixels per design pixel of the box along each axis, and the fractions of a pixel the box starts at
            let (across, down) = (request.scale.x.pixels(1.0), request.scale.y.pixels(1.0));
            let (shift_x, shift_y) = request.phase;
            let (shape, (first, end)) = request.drawn_rows(own)?;
            let covers = shape.covers;
            let (width, height) = shape.size;
            // vector's one scale: the mean of the two, so that each axis is scaled by a factor of about 1
            let one = f64::midpoint(across, down);
            #[expect(clippy::cast_precision_loss, reason = "a side of at most 65,000 pixels")]
            let (frame, size) = (
                (width as f64 / one, height as f64 / one),
                Size::px(width as f64, height as f64),
            );
            // the rows asked for are the crop of the whole box's raster, byte for byte (vector's window render); a window vector
            // refuses (a side past 65,000 pixels, 2^27 pixels in all) is a raster its whole render refuses too
            #[expect(clippy::cast_precision_loss, reason = "a side of at most 65,000 pixels")]
            let window = Window::px(0.0, first as f64, width as f64, (end - first) as f64);
            let bytes = self
                .document
                .scale(ScaleBy::new((across / one, down / one)).about((0.0, 0.0)))
                .translate(shift_x / one, shift_y / one)
                .with_size(frame.0, frame.1)
                .to_rgba_window(size, window, &given)
                .map_err(|error| Error::invalid("drawing", error.to_string()))?;
            let pixels = bytes.iter().map(|&byte| f32::from(byte) / 255.0).collect();
            Ok(Drawn {
                raster: Arc::new(Raster::made(width, end - first, pixels)),
                covers,
                ends_at_window: request.window == own,
                room: [0; 4],
                maps: BTreeMap::new(),
            })
        });
        Ok(opened.drawing_whole().by_window())
    }
}
