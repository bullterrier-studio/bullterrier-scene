//! A vector drawing as a layer, and the one place that knows how the vector crate reads and draws a document.

use std::sync::Arc;

use bullterrier_vector::{Font, Inputs, ScaleBy, Size, Values};
use serde_json::{Value, json};

use super::{Drawn, Opened, Source};
use crate::geometry::{Rect, whole_up};
use crate::text::{Face, Text};
use crate::{Error, Raster};

/// A drawing with a frame. Its box is the frame, one design pixel to a pixel of the frame, at the canvas origin or, for a
/// text, where the text's anchor makes it.
#[derive(Debug)]
pub(crate) struct Drawing {
    document: bullterrier_vector::Drawing,
    /// The top left of the box in the layer's space: `(0, 0)` for a drawing, the glyphs' bounds for a text.
    x: f64,
    y: f64,
    width: f64,
    height: f64,
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
            document,
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
        let (font, inputs) = match &text.face {
            Face::Bundled(name) => (
                Font::bundled(name).map_err(|error| Error::invalid("font", error.to_string()))?,
                Inputs::new(),
            ),
            Face::File(path) => {
                let bytes = std::fs::read(path).map_err(|error| Error::Read {
                    path: path.display().to_string(),
                    message: format!("the font cannot be read: {error}"),
                })?;
                let font = Font::new(bytes.clone())
                    .and_then(|font| font.named("font"))
                    .map_err(|error| Error::invalid("font", format!("{}: {error}", path.display())))?;
                (font, Inputs::new().set("font", bytes))
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

impl Source for Drawing {
    /// The whole frame at the asked scale: vector draws no part of a frame yet.
    ///
    /// Vector draws a frame at one scale, centred, in a raster of whole pixels, which leaves a fraction of a pixel of slack along
    /// the axis that does not bind, and cannot draw a frame at two scales. So the raster is made to the pixel: the drawing's
    /// shapes are scaled along x and y to the density the layer is drawn at, moved by the fractions of a pixel that
    /// `Request::phase` says, and put in a frame that is exactly the raster at one scale. The raster then covers the box
    /// from `phase` before its top left, at that density; the warp copies it onto whole pixels and cuts the box out of it
    /// by coverage, so an edge on a pixel boundary is hard and `stretch` stretches.
    fn open(&self, _canvas: Rect) -> Result<Opened<'_>, Error> {
        let own = Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        };
        let opened = Opened::new(own, move |request| {
            // output pixels per design pixel of the box along each axis, and the fractions of a pixel the box starts at
            let (across, down) = (request.scale.x.pixels(1.0), request.scale.y.pixels(1.0));
            let (shift_x, shift_y) = request.phase;
            // whole pixels, a side of at most 2^27: exact in a float
            #[expect(clippy::cast_precision_loss, reason = "a raster side of at most 2^27 pixels")]
            let covers = Rect {
                x: own.x - shift_x / across,
                y: own.y - shift_y / down,
                width: whole_up(shift_x + own.width * across) as f64 / across,
                height: whole_up(shift_y + own.height * down) as f64 / down,
            };
            let (width, height) = request.scale.size(covers)?;
            // vector's one scale: the mean of the two, so that each axis is scaled by a factor of about 1
            let one = f64::midpoint(across, down);
            #[expect(clippy::cast_precision_loss, reason = "a side of at most 65,000 pixels")]
            let (frame, size) = (
                (width as f64 / one, height as f64 / one),
                Size::px(width as f64, height as f64),
            );
            let bytes = self
                .document
                .scale(ScaleBy::new((across / one, down / one)).about((0.0, 0.0)))
                .translate(shift_x / one, shift_y / one)
                .with_size(frame.0, frame.1)
                .to_rgba(size, &Values::new())
                .map_err(|error| Error::invalid("drawing", error.to_string()))?;
            let pixels = bytes.iter().map(|&byte| f32::from(byte) / 255.0).collect();
            Ok(Drawn {
                raster: Arc::new(Raster::made(width, height, pixels)),
                covers,
                ends_at_window: request.window == own,
            })
        });
        Ok(opened.drawing_whole())
    }
}
