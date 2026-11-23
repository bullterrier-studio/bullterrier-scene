//! Helpers: run the command, and make a folder holding a saved scene and its photo.

use std::path::{Path, PathBuf};

use bullterrier_scene::{Fit, Layer, Place, Scene, Text};
use tempfile::TempDir;

// the repository's one tolerance, the file the core's tests use
#[path = "../../../../core/tests/it/common/looks.rs"]
mod looks;

pub(crate) type Outcome = Result<(), Box<dyn std::error::Error>>;

/// What a command did.
pub(crate) struct Ran {
    pub(crate) code: i32,
    pub(crate) out: String,
    pub(crate) err: String,
}

/// `bt-scene ARGS`, in this process, with version `9.9.9`.
pub(crate) fn bt(args: &[&str]) -> Ran {
    let given: Vec<String> = args.iter().map(|&arg| arg.to_owned()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = bullterrier_scene_cli::main(&given, "9.9.9", &mut out, &mut err);
    Ran {
        code,
        out: String::from_utf8_lossy(&out).into_owned(),
        err: String::from_utf8_lossy(&err).into_owned(),
    }
}

/// `path` as the text of an argument.
pub(crate) fn text(path: &Path) -> &str {
    path.to_str()
        .unwrap_or_else(|| unreachable!("a temporary path is text"))
}

/// A picture file of `width` by `height` pixels: a ramp, so that a wrong row or column shows.
pub(crate) fn photo(path: &Path, (width, height): (usize, usize)) -> Outcome {
    let values: Vec<f32> = (0..height * width)
        .flat_map(|index| {
            let (x, y) = (index % width, index / width);
            let level = |step: usize, size: usize| f32::from(u8::try_from(step * 255 / size).unwrap_or(255)) / 255.0;
            [level(x, width), level(y, height), 0.4]
        })
        .collect();
    bullterrier_image::write(path, &bullterrier_image::Image::new(height, width, &values)?)?;
    Ok(())
}

/// A folder holding `photo.png`, a second picture `other.png`, and `scene.json`: a photo (named by its relative path), a band
/// and a title, 64 by 36 design pixels.
pub(crate) struct Project {
    pub(crate) folder: TempDir,
}

impl Project {
    pub(crate) fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        photo(&folder.path().join("photo.png"), (64, 36))?;
        photo(&folder.path().join("other.png"), (20, 30))?;
        let picture = folder.path().join("photo.png");
        let scene = Scene::new(64, 36)?.add([
            Layer::picture_file(&picture)
                .named("photo")?
                .fit([0.0, 0.0, 64.0, 36.0], Fit::Cover, [0.5, 0.5])?,
            Layer::fill("#101018")?
                .named("band")?
                .fit([0.0, 24.0, 64.0, 12.0], Fit::Stretch, [0.5, 0.5])?
                .opacity(0.82)?,
            Layer::text(
                "Title",
                &Text::new("bricolage_grotesque").color("#ffffff").option("size", 8.0),
            )?
            .named("title")?
            .place(Place::new().dx(3.0).dy(32.0))?,
        ])?;
        let document = scene.to_json()?.replace(text(&picture), "photo.png");
        std::fs::write(folder.path().join("scene.json"), document)?;
        Ok(Self { folder })
    }

    /// A file of the folder.
    pub(crate) fn file(&self, name: &str) -> PathBuf {
        self.folder.path().join(name)
    }

    /// `scene.json`, read as the command reads it.
    pub(crate) fn scene(&self) -> Result<Scene, Box<dyn std::error::Error>> {
        let text = std::fs::read_to_string(self.file("scene.json"))?;
        Ok(Scene::from_json(&text, self.folder.path())?)
    }
}

/// The 8-bit values of the picture file `path` (red, green, blue and alpha of each pixel) and its size, read by image's reader.
pub(crate) fn decoded(path: &Path) -> Result<(usize, usize, Vec<u8>), Box<dyn std::error::Error>> {
    let image = bullterrier_image::read(path)?;
    assert_eq!(image.channels(), 4, "alpha is kept");
    let (width, height) = (image.width(), image.height());
    let bytes = image.into_values().iter().map(|&level| byte(level)).collect();
    Ok((width, height, bytes))
}

/// `level` (a multiple of 1/255, or near it) as its 8-bit number.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a level is 0..1, so the number is 0..255"
)]
fn byte(level: f32) -> u8 {
    (level * 255.0).round() as u8
}

/// Fails naming `what` and the difference unless the PNG at `got` looks the same as the PNG at `expected` (the one tolerance).
pub(crate) fn assert_same_picture(got: &Path, expected: &Path, what: &str) -> Outcome {
    let ((w, h, got), (ew, eh, expected)) = (decoded(got)?, decoded(expected)?);
    assert_eq!((w, h), (ew, eh), "{what}: size");
    let verdict = looks::levels_look_the_same(&got, &expected);
    assert!(verdict.is_ok(), "{what}: {}", verdict.err().unwrap_or_default());
    Ok(())
}
