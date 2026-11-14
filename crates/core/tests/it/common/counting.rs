//! A kind of layer that counts how often the scene asks it to draw: the way a test sees the cache.

use std::sync::{Arc, Mutex};

use bullterrier_scene::{Error, LayerKind, Raster};
use serde_json::{Value, json};

/// One question the scene asked: the rectangle and the scale.
pub(crate) type Question = ([f64; 4], [f64; 2]);

/// A layer kind whose raster is a function of its `tone`, the rectangle and the scale alone: red is the tone, green and blue
/// ramp with the design position of the pixel. It remembers every question.
pub(crate) struct Counting {
    tone: f64,
    asked: Mutex<Vec<Question>>,
}

impl Counting {
    pub(crate) fn new(tone: f64) -> Arc<Self> {
        Arc::new(Self {
            tone,
            asked: Mutex::new(Vec::new()),
        })
    }

    /// How many times the scene has asked this kind to draw.
    pub(crate) fn draws(&self) -> usize {
        self.questions().len()
    }

    /// Every question, in order.
    pub(crate) fn questions(&self) -> Vec<Question> {
        self.asked.lock().map(|asked| asked.clone()).unwrap_or_default()
    }
}

/// `ceil(length * scale)`, a product within 1e-12 of a whole number counting as that number.
fn pixels(length: f64, scale: f64) -> usize {
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a test size")]
    {
        (length * scale * (1.0 - 1e-12)).ceil().max(0.0) as usize
    }
}

impl LayerKind for Counting {
    fn name(&self) -> &'static str {
        "counting"
    }

    fn options(&self) -> Value {
        json!({ "tone": self.tone })
    }

    fn draw(&self, area: [f64; 4], scale: [f64; 2]) -> Result<Raster, Error> {
        if let Ok(mut asked) = self.asked.lock() {
            asked.push((area, scale));
        }
        let (columns, rows) = (pixels(area[2], scale[0]), pixels(area[3], scale[1]));
        let ramp = |origin: f64, step: usize, scale: f64| {
            #[expect(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                reason = "a test size"
            )]
            {
                ((origin + step as f64 / scale) * 0.01).rem_euclid(1.0) as f32
            }
        };
        #[expect(clippy::cast_possible_truncation, reason = "a tone from 0 to 1")]
        let tone = self.tone as f32;
        let values = (0..rows).flat_map(|row| {
            (0..columns)
                .flat_map(move |column| [tone, ramp(area[0], column, scale[0]), ramp(area[1], row, scale[1]), 1.0])
        });
        Raster::rgba(columns, rows, values.collect())
    }
}
