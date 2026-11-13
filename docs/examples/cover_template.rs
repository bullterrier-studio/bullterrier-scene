//! C6 in Rust: one design, many pictures. The same scene as `cover_template.py`: a photo and a title as parameters,
//! written to JSON and read back, then rendered with other values. Not compiled until C1 builds the crate; it is the
//! acceptance material for the Rust side of `docs/api.md`. A call that can be refused returns `Result` (hence the `?`).

use std::path::Path;

use bullterrier_scene::{Error, Fit, Layer, Place, Rect, Scene, Text, Values};

const W: u32 = 1280;
const H: u32 = 720;

/// The cover: a photo (from a file), a band and a title.
fn build(placeholder: &Path) -> Result<Scene, Error> {
    let photo = Layer::picture_file(placeholder)
        .named("photo")?
        .fit(Rect::new(0.0, 0.0, 1280.0, 720.0), Fit::Cover)?;
    let band = Layer::fill("#101018")?
        .named("band")?
        .fit(Rect::new(0.0, 480.0, 1280.0, 240.0), Fit::Stretch)?
        .opacity(0.82)?;
    let title = Layer::text(
        "Episode title",
        Text::new("bricolage_grotesque")
            .color("#ffffff")
            .option("size", 84.0)
            .option("width", 1080.0)
            .option("axes", serde_json::json!({ "wght": 800 })),
    )?
    .named("title")?
    .place(Place::new().dx(100.0).dy(590.0))?;
    Scene::new(W, H)?.add([photo, band, title])
}

fn main() -> Result<(), Error> {
    let cover = build(Path::new("photos/placeholder.jpg"))?;
    let json = cover.to_json()?;
    let template = Scene::from_json(&json, Path::new("."))?;

    let values = Values::new()
        .set("title.text", "Pilot")
        .set("photo.picture", Path::new("photos/a.jpg"))
        .set("band.color", "#2b0a3d");
    let picture = template.with_values(&values)?.render(W)?; // a `Raster`: RGBA f32, straight alpha
    assert_eq!((picture.width(), picture.height()), (1280, 720));
    Ok(())
}
