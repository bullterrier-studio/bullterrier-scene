//! C6 in Rust: one design, many pictures. The same scene as `cover_template.py`: a photo and a title as parameters,
//! written to JSON and read back, then rendered with other values. Not compiled until C1 builds the crate; it is the
//! acceptance material for the Rust side of `docs/api.md`.

use bullterrier_scene::{Blend, Error, Fit, Layer, Place, Rect, Scene, Text, Values};

const W: u32 = 1280;
const H: u32 = 720;

fn main() -> Result<(), Error> {
    let cover = Scene::new(W, H)?
        .add(
            Layer::picture_file("photos/placeholder.jpg")
                .named("photo")?
                .fit(Rect::new(0.0, 0.0, 1280.0, 720.0), Fit::Cover),
        )
        .add(
            Layer::fill("#101018")?
                .named("band")?
                .fit(Rect::new(0.0, 480.0, 1280.0, 240.0), Fit::Stretch)
                .opacity(0.82)?
                .blend(Blend::Normal),
        )
        .add(
            Layer::text(
                "Episode title",
                Text::new("bricolage_grotesque")
                    .size(84.0)
                    .color("#ffffff")
                    .width(1080.0)
                    .axis("wght", 800.0),
            )?
            .named("title")?
            .place(Place::new().dx(100.0).dy(590.0)),
        );

    let json = cover.to_json()?;
    let template = Scene::from_json(&json, std::path::Path::new("."))?;

    let values = Values::new()
        .set("title.text", "Pilot")
        .set("photo.picture", "photos/a.jpg")
        .set("band.color", "#2b0a3d");
    let picture = template.with_values(&values)?.render(W)?; // a `Raster`: RGBA f32, straight alpha
    assert_eq!((picture.width(), picture.height()), (1280, 720));
    Ok(())
}
