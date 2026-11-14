//! The saved scene (`Scene::to_json`, `Scene::from_json`): the oracle is the round trip (a scene read back equals the scene,
//! draws the same bits and lists the same parameters; the text is the same twice and read-and-written again), a table of
//! known-bad documents with the pointer and the words of each refusal, and the rules of a file (a path, its digest).

use std::path::{Path, PathBuf};

use bullterrier_scene::{
    Blend, Deg, Error, Fit, Layer, MAX_DEPTH, MAX_LAYERS, Mask, Place, Raster, Scene, Text, Use, Values,
};
use proptest::prelude::*;
use serde_json::Value;

use crate::common::{Outcome, rectangle, scene, varied};

const FONT: &str = "bricolage_grotesque";
const GRADE: &str = r#"{"steps": [{"effect": "grade", "params": {"exposure": 1.0}, "name": "up"}]}"#;

/// A grey picture file of `width` by `height` pixels (a map reads it).
fn write_grey(path: &Path, width: usize, height: usize) -> Outcome {
    let grey: Vec<f32> = (0..width * height)
        .flat_map(|at| {
            let level = f32::from(u8::try_from(at * 7 % 256).unwrap_or(0)) / 255.0;
            [level, level, level, 1.0]
        })
        .collect();
    bullterrier_image::write(path, &bullterrier_image::Image::rgba(height, width, &grey)?)?;
    Ok(())
}

fn write_picture(path: &Path, picture: &Raster) -> Outcome {
    bullterrier_image::write(
        path,
        &bullterrier_image::Image::rgba(picture.height(), picture.width(), picture.pixels())?,
    )?;
    Ok(())
}

/// The files a scene names, in a folder of its own.
struct Files {
    folder: tempfile::TempDir,
}

impl Files {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let files = Self {
            folder: tempfile::tempdir()?,
        };
        write_picture(&files.path("photo.png"), &varied(12, 8)?)?;
        write_grey(&files.path("matte.png"), 12, 8)?;
        Ok(files)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.folder.path().join(name)
    }

    fn base(&self) -> &Path {
        self.folder.path()
    }
}

/// A scene that uses every part of the document, naming the files of `files` by relative path.
fn rich(files: &Files) -> Result<Scene, Error> {
    let badge = scene(
        20,
        10,
        [
            Layer::fill("#336699")?.named("plate")?.opacity(0.5)?,
            Layer::text("Hi", &Text::new(FONT).color("#ffffff").option("size", 6))
                .and_then(|layer| layer.named("word"))?,
        ],
    )?;
    let hole = Layer::picture_hole()
        .named("hole")?
        .fit([0.0, 0.0, 24.0, 16.0], Fit::Contain, [0.0, 1.0])?;
    let photo = Layer::picture_file(files.path("photo.png"))
        .map_file("matte", files.path("matte.png"))?
        .map_hole("later")?
        .named("photo")?
        .fit([0.0, 0.0, 24.0, 16.0], Fit::Cover, [0.25, 0.5])?
        .place(
            Place::new()
                .dx(1.5)
                .dy(-2.0)
                .scale(0.9)
                .aspect(1.1)
                .rotate(Deg(-4.0))
                .skew_x(Deg(3.0))
                .flip_y(true)
                .anchor(0.25, 0.75),
        )?
        .mask("matte")?
        .mask(Mask::new("matte").feather(1.5).invert())?
        .look(GRADE)?
        .look(r#"{"steps": [{"effect": "grade", "params": {"contrast": 0.1}}]}"#)?
        .opacity(0.82)?
        .blend(Blend::SoftLight);
    let sheet = Layer::drawing(&rectangle((40, 20), (10.0, 5.0), (30.0, 15.0), "#ff0000"))?
        .named("sheet")?
        .perspective([[0.0, 0.0], [20.0, 2.0], [22.0, 14.0], [1.0, 15.0]])?
        .as_light(true);
    let glow = Layer::of("sheet")
        .named("glow")?
        .place(Place::new().dx(3.0).scale(1.2))?
        .look(GRADE)?
        .mask(Layer::fill("#ffffff")?.fit([2.0, 2.0, 10.0, 10.0], Fit::Stretch, [0.5, 0.5])?)?
        .mask(
            Mask::new(Layer::of("photo").mask(Layer::fill("#808080")?)?.opacity(0.5)?)
                .using(Use::Luminance)
                .feather(2.0)
                .invert(),
        )?;
    let group = Layer::scene(badge)
        .named("badge")?
        .place(Place::new().dx(10.0).dy(8.0))?
        .blend(Blend::Multiply)
        .look(GRADE)?;
    let mut scene_ = Scene::new(24, 16)?.add([hole, photo, sheet, glow, group])?;
    scene_ = scene_.add([Layer::fill("#10203040")?.opacity(0.123_456_79)?])?;
    Ok(scene_)
}

fn same_bits(left: &Raster, right: &Raster) -> bool {
    (left.width(), left.height()) == (right.width(), right.height())
        && left
            .pixels()
            .iter()
            .map(|v| v.to_bits())
            .eq(right.pixels().iter().map(|v| v.to_bits()))
}

/// What a scene read back must be: equal, drawing the same bits, listing the same parameters; its text the same on every call,
/// and the text of the scene read back the same text.
fn round_trips(scene_: &Scene, base: &Path, holes_filled: bool) -> Outcome {
    let text = scene_.to_json()?;
    assert_eq!(text, scene_.to_json()?, "the text differs between two calls");
    let back = Scene::from_json(&text, base)?;
    assert!(back == *scene_, "the scene read back is not equal:\n{text}");
    assert_eq!(back.to_json()?, text, "the text read and written again differs");
    assert_eq!(back.parameters(), scene_.parameters());
    if holes_filled {
        assert!(same_bits(&back.render(48)?, &scene_.render(48)?), "the pictures differ");
    }
    Ok(())
}

#[test]
fn a_scene_with_every_part_comes_back_equal_and_draws_the_same_picture() -> Outcome {
    let files = Files::new()?;
    let holes = rich(&files)?;
    round_trips(&holes, files.base(), false)?;
    // `with_values` fills the holes with files, whose digests the text then holds
    let filled = holes.with_values(
        &Values::new()
            .set("hole.picture", files.path("photo.png"))
            .set("photo.map.later", files.path("matte.png")),
    )?;
    round_trips(&filled, files.base(), true)
}

fn read(text: &str) -> Result<Scene, Error> {
    Scene::from_json(text, Path::new(""))
}

#[test]
fn the_document_of_a_small_scene_is_this_text() -> Outcome {
    let small = scene(
        24,
        16,
        [
            Layer::fill("#101018")?
                .named("band")?
                .fit([0.0, 8.0, 24.0, 8.0], Fit::Stretch, [0.5, 0.5])?
                .opacity(0.82)?,
            Layer::picture_hole().named("photo")?,
        ],
    )?;
    assert_eq!(
        small.to_json()?,
        r##"{"version":1,"canvas":{"width":24,"height":16},"layers":[{"name":"band","fill":"#101018","fit":{"box":[0,8,24,8],"how":"stretch"},"opacity":0.82},{"name":"photo","picture":null}]}"##
    );
    Ok(())
}

#[test]
fn a_document_with_every_default_left_out_is_the_scene_built_with_the_defaults() -> Outcome {
    let minimal = read(
        r##"{"version":1,"canvas":{"width":24,"height":16},"layers":[{"fill":"#ff0000"},{"of":"layer1"},{"text":"Hi","font":"bricolage_grotesque"}]}"##,
    )?;
    let built = scene(
        24,
        16,
        [
            Layer::fill("#ff0000")?,
            Layer::of("layer1"),
            Layer::text("Hi", &Text::new(FONT))?,
        ],
    )?;
    assert_eq!(minimal, built);
    let explicit = read(
        r##"{"version":1,"canvas":{"width":24,"height":16},"layers":[{"fill":"#ff0000","opacity":1,"blend":"normal","light":false,"place":{"dx":0,"scale":1},"masks":[],"looks":[]},{"of":"layer1"},{"text":"Hi","font":"bricolage_grotesque","color":"#000000"}]}"##,
    )?;
    assert_eq!(explicit, built);
    assert_eq!(explicit.to_json()?, built.to_json()?);
    Ok(())
}

/// A mask by a layer and a mask by a map are numbered together by the calls (`mask`, `mask#2`...): the document keeps their
/// order, so the numbers come back.
#[test]
fn masks_of_both_kinds_keep_the_order_of_their_calls() -> Outcome {
    let files = Files::new()?;
    let layer = Layer::picture_file(files.path("photo.png"))
        .map_file("matte", files.path("matte.png"))?
        .named("photo")?
        .mask(Layer::fill("#111111")?.opacity(0.5)?)?
        .mask("matte")?
        .mask(Mask::new(Layer::fill("#222222")?).feather(2.0))?
        .mask(Mask::new("matte").invert())?;
    let made = scene(12, 8, [layer])?;
    let document: Value = serde_json::from_str(&made.to_json()?)?;
    let kinds: Vec<&str> = document["layers"][0]["masks"]
        .as_array()
        .ok_or("no masks")?
        .iter()
        .map(|mask| if mask.get("map").is_some() { "map" } else { "layer" })
        .collect();
    assert_eq!(kinds, ["layer", "map", "layer", "map"]);
    let back = read(&made.to_json()?)?;
    assert_eq!(back.parameters(), made.parameters());
    assert!(
        back.parameters()
            .iter()
            .any(|parameter| parameter.address == "photo.mask#3.opacity")
    );
    Ok(())
}

#[test]
fn equality_is_the_same_document_and_nothing_else() -> Outcome {
    let base = scene(24, 16, [Layer::fill("#ff0000")?.named("a")?])?;
    let same = scene(24, 16, [Layer::fill("#FF0000")?.named("a")?])?;
    assert!(base == same, "a colour's case is no difference");
    let others: [(&str, Scene); 8] = [
        ("canvas", scene(24, 17, [Layer::fill("#ff0000")?.named("a")?])?),
        ("name", scene(24, 16, [Layer::fill("#ff0000")?.named("b")?])?),
        ("colour", scene(24, 16, [Layer::fill("#ff0001")?.named("a")?])?),
        (
            "opacity",
            scene(24, 16, [Layer::fill("#ff0000")?.named("a")?.opacity(0.99)?])?,
        ),
        (
            "blend",
            scene(24, 16, [Layer::fill("#ff0000")?.named("a")?.blend(Blend::Screen)])?,
        ),
        (
            "place",
            scene(
                24,
                16,
                [Layer::fill("#ff0000")?.named("a")?.place(Place::new().dx(1.0))?],
            )?,
        ),
        (
            "order",
            scene(24, 16, [Layer::fill("#ff0000")?, Layer::fill("#ff0000")?.named("a")?])?,
        ),
        (
            "a look",
            scene(24, 16, [Layer::fill("#ff0000")?.named("a")?.look(GRADE)?])?,
        ),
    ];
    for (what, other) in &others {
        assert_ne!(base, *other, "{what}");
    }
    // two pictures held in memory are equal by their pixels
    let held = |value: f32| -> Result<Scene, Error> {
        scene(2, 2, [Layer::picture(Raster::rgba(1, 1, vec![value, 0.0, 0.0, 1.0])?)])
    };
    assert_eq!(held(0.5)?, held(0.5)?);
    assert_ne!(held(0.5)?, held(0.25)?);
    // two masks in the other order are two scenes
    let two = |first: &str, second: &str| -> Result<Scene, Error> {
        scene(
            4,
            4,
            [Layer::fill("#ffffff")?
                .mask(Layer::fill(first)?)?
                .mask(Layer::fill(second)?)?],
        )
    };
    assert_ne!(two("#111111", "#222222")?, two("#222222", "#111111")?);
    Ok(())
}

/// A scene that has a picture or a map held in memory has no file to name.
#[test]
fn a_picture_or_a_map_in_memory_is_refused_naming_the_layer_and_what_to_do() -> Outcome {
    let files = Files::new()?;
    let held = Layer::picture(varied(4, 4)?).named("photo")?;
    let refused = scene(4, 4, [held.clone()])?.to_json();
    assert!(
        matches!(&refused, Err(Error::Invalid { address, message }) if address == "photo.picture" && message.contains("no file")),
        "{refused:?}"
    );
    let mapped = Layer::picture_file(files.path("photo.png"))
        .map("matte", bullterrier_scene::Map::new(1, 1, vec![0.5])?)?
        .named("photo")?;
    let refused = scene(4, 4, [mapped])?.to_json();
    assert!(
        matches!(&refused, Err(Error::Invalid { address, .. }) if address == "photo.map.matte"),
        "{refused:?}"
    );
    let masked = Layer::fill("#fff000")?
        .mask(Layer::picture(varied(4, 4)?))?
        .named("under")?;
    let refused = scene(4, 4, [masked])?.to_json();
    assert!(
        matches!(&refused, Err(Error::Invalid { address, .. }) if address == "under.mask.picture"),
        "{refused:?}"
    );
    Ok(())
}

// --- the files -------------------------------------------------------------------------------------------------------

#[test]
fn a_relative_path_is_read_from_the_folder_it_is_resolved_against_and_the_digest_is_the_files() -> Outcome {
    let files = Files::new()?;
    let text = r#"{"version":1,"canvas":{"width":12,"height":8},"layers":[{"picture":{"path":"photo.png"},"maps":{"matte":{"path":"matte.png"}}}]}"#;
    // loading reads nothing: the folder it is not in is no matter until it renders
    let loaded = Scene::from_json(text, files.base())?;
    let drawn = loaded.render(12)?;
    let by_path = scene(
        12,
        8,
        [Layer::picture_file(files.path("photo.png")).map_file("matte", files.path("matte.png"))?],
    )?;
    assert!(same_bits(&drawn, &by_path.render(12)?));
    // from another folder the file is not there: a read error at render, not at load
    let elsewhere = Scene::from_json(text, &files.path("nowhere"))?;
    let refused = elsewhere.render(12);
    assert!(
        matches!(&refused, Err(Error::Read { path, .. }) if path.contains("nowhere") && path.ends_with("photo.png")),
        "{refused:?}"
    );
    // saved: the path as it was written, and the digest the file has
    let saved: Value = serde_json::from_str(&loaded.to_json()?)?;
    let picture = &saved["layers"][0]["picture"];
    assert_eq!(picture["path"], "photo.png");
    let digest = picture["sha256"].as_str().ok_or("no digest")?.to_owned();
    assert_eq!(digest.len(), 64);
    // a digest that is not the file's is a read error at render naming both
    let wrong = "0".repeat(64);
    let tampered = text.replace(r#""photo.png""#, &format!(r#""photo.png","sha256":"{wrong}""#));
    let refused = Scene::from_json(&tampered, files.base())?.render(12);
    assert!(
        matches!(&refused, Err(Error::Read { message, .. }) if message.contains(&wrong) && message.contains(&digest)),
        "{refused:?}"
    );
    // the digest of a file that is there is accepted
    let kept = text.replace(r#""photo.png""#, &format!(r#""photo.png","sha256":"{digest}""#));
    assert!(same_bits(&Scene::from_json(&kept, files.base())?.render(12)?, &drawn));
    // the file changed after the scene was saved: the check sees it
    let inverted: Vec<f32> = varied(12, 8)?.pixels().iter().map(|value| 1.0 - value).collect();
    write_picture(&files.path("photo.png"), &Raster::rgba(12, 8, inverted)?)?;
    let refused = Scene::from_json(&kept, files.base())?.render(12);
    assert!(
        matches!(&refused, Err(Error::Read { message, .. }) if message.contains("changed")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_file_that_is_not_there_is_a_read_error_at_render_and_at_to_json_not_at_load() -> Outcome {
    let files = Files::new()?;
    let text =
        r#"{"version":1,"canvas":{"width":4,"height":4},"layers":[{"name":"gone","picture":{"path":"missing.png"}}]}"#;
    let loaded = Scene::from_json(text, files.base())?;
    assert!(matches!(loaded.render(4), Err(Error::Read { path, .. }) if path.ends_with("missing.png")));
    assert!(matches!(loaded.to_json(), Err(Error::Read { path, .. }) if path.ends_with("missing.png")));
    Ok(())
}

#[test]
fn a_path_where_a_look_or_a_drawing_belongs_is_read_at_load_and_written_inline() -> Outcome {
    let files = Files::new()?;
    std::fs::write(
        files.path("night.toml"),
        "[[step]]\neffect = \"grade\"\nexposure = 1.0\n",
    )?;
    std::fs::write(
        files.path("sheet.json"),
        rectangle((40, 20), (10.0, 5.0), (30.0, 15.0), "#ff0000"),
    )?;
    let text =
        r#"{"version":1,"canvas":{"width":40,"height":20},"layers":[{"drawing":"sheet.json","looks":["night.toml"]}]}"#;
    let loaded = Scene::from_json(text, files.base())?;
    let by_calls = scene(
        40,
        20,
        [
            Layer::drawing(&rectangle((40, 20), (10.0, 5.0), (30.0, 15.0), "#ff0000"))?
                .look_file(files.path("night.toml"))?,
        ],
    )?;
    assert_eq!(loaded, by_calls);
    let saved: Value = serde_json::from_str(&loaded.to_json()?)?;
    assert!(saved["layers"][0]["drawing"].is_object());
    assert!(saved["layers"][0]["looks"][0]["recipe"].is_object());
    // read now: a path that is not there is a read error at load
    let refused = Scene::from_json(text, &files.path("nowhere"));
    assert!(
        matches!(&refused, Err(Error::Read { path, .. }) if path.ends_with("sheet.json")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_font_file_is_read_at_load_with_its_digest_checked() -> Outcome {
    let files = Files::new()?;
    let refused =
        read(r#"{"version":1,"canvas":{"width":4,"height":4},"layers":[{"text":"Hi","font":{"path":"nowhere.ttf"}}]}"#);
    assert!(
        matches!(&refused, Err(Error::Read { path, .. }) if path.ends_with("nowhere.ttf")),
        "{refused:?}"
    );
    std::fs::write(files.path("bad.ttf"), b"not a font")?;
    let wrong = "1".repeat(64);
    let text = format!(
        r#"{{"version":1,"canvas":{{"width":4,"height":4}},"layers":[{{"text":"Hi","font":{{"path":"bad.ttf","sha256":"{wrong}"}}}}]}}"#
    );
    let refused = Scene::from_json(&text, files.base());
    assert!(
        matches!(&refused, Err(Error::Read { message, .. }) if message.contains(&wrong)),
        "{refused:?}"
    );
    Ok(())
}

// --- known-bad documents -------------------------------------------------------------------------------------------

/// The document with `layers` on a canvas of 24 by 16.
fn with(layers: &str) -> String {
    format!(r#"{{"version":1,"canvas":{{"width":24,"height":16}},"layers":[{layers}]}}"#)
}

fn nested(depth: usize) -> String {
    let mut inner = with(r##"{"fill":"#ffffff"}"##);
    for _ in 0..depth {
        inner = with(&format!(r#"{{"scene":{inner}}}"#));
    }
    inner
}

/// A layer masked by a layer masked by... `depth` mask layers deep.
fn masked(depth: usize) -> String {
    let mut inner = r##"{"fill":"#ffffff"}"##.to_owned();
    for _ in 0..depth {
        inner = format!(r##"{{"fill":"#ffffff","masks":[{{"layer":{inner}}}]}}"##);
    }
    with(&inner)
}

fn too_many_layers() -> String {
    with(&vec![r##"{"fill":"#ffffff"}"##; MAX_LAYERS + 1].join(","))
}

/// A pointer and a piece of the words the refusal of a document has.
#[test]
fn a_document_the_reader_cannot_take_is_refused_at_the_pointer_of_the_value_with_its_words() {
    let deep = nested(MAX_DEPTH + 1);
    let deep_at = "/layers/0/scene".repeat(MAX_DEPTH + 1);
    let masks = masked(MAX_DEPTH + 1);
    let masks_at = format!("/layers/0{}", "/masks/0/layer".repeat(MAX_DEPTH + 1));
    let many = too_many_layers();
    #[rustfmt::skip]
    let rows: Vec<(&str, String, &str, &str)> = vec![
        ("not JSON", "{".to_owned(), "", "not JSON"),
        ("trailing text", r#"{"version":1,"canvas":{"width":1,"height":1}} x"#.to_owned(), "", "not JSON"),
        ("a key twice", r#"{"version":1,"version":1}"#.to_owned(), "/version", "written twice"),
        ("a key twice, deeper", with(r##"{"fill":"#fff000","fill":"#000fff"}"##), "/layers/0/fill", "written twice"),
        ("no object", "[]".to_owned(), "", "an object"),
        ("no version", r#"{"canvas":{"width":1,"height":1}}"#.to_owned(), "", "`version` is missing"),
        ("a version this release does not read", r#"{"version":2,"canvas":{"width":1,"height":1}}"#.to_owned(), "/version", "reads scene version 1"),
        ("a version that is text", r#"{"version":"1","canvas":{"width":1,"height":1}}"#.to_owned(), "/version", "scene version"),
        ("no canvas", r#"{"version":1}"#.to_owned(), "", "`canvas` is missing"),
        ("a canvas that is a list", r#"{"version":1,"canvas":[1,1]}"#.to_owned(), "/canvas", "a canvas is"),
        ("a canvas without a side", r#"{"version":1,"canvas":{"width":1}}"#.to_owned(), "/canvas", "`height` is missing"),
        ("a canvas side that is no whole number", r#"{"version":1,"canvas":{"width":1.5,"height":1}}"#.to_owned(), "/canvas/width", "whole number"),
        ("a canvas side beyond the limit", r#"{"version":1,"canvas":{"width":100001,"height":1}}"#.to_owned(), "/canvas", "1 to 100000"),
        ("a canvas side of 0", r#"{"version":1,"canvas":{"width":0,"height":1}}"#.to_owned(), "/canvas", "canvas"),
        ("a key nobody knows", r#"{"version":1,"canvas":{"width":1,"height":1},"layer":[]}"#.to_owned(), "/layer", "did you mean layers"),
        ("layers that are no list", r#"{"version":1,"canvas":{"width":1,"height":1},"layers":{}}"#.to_owned(), "/layers", "a list of layers"),
        ("a layer that is no object", with("3"), "/layers/0", "an object"),
        ("a layer with no source", with(r#"{"opacity":1}"#), "/layers/0", "one of picture"),
        ("a layer with two sources", with(r##"{"fill":"#ffffff","of":"x"}"##), "/layers/0", "fill and of"),
        ("a misspelt key of a layer", with(r##"{"fill":"#ffffff","opcity":0.5}"##), "/layers/0/opcity", "did you mean opacity"),
        ("maps beside a fill", with(r##"{"fill":"#ffffff","maps":{}}"##), "/layers/0/maps", "belongs to a layer that draws a picture"),
        ("a bad colour", with(r#"{"fill":"red"}"#), "/layers/0/fill", "#rrggbb"),
        ("a colour that is no text", with(r#"{"fill":3}"#), "/layers/0/fill", "a colour (text) is wanted, got a number 3"),
        ("a bad blend name", with(r##"{"fill":"#ffffff","blend":"multipy"}"##), "/layers/0/blend", "multiply"),
        ("a negative opacity", with(r##"{"fill":"#ffffff","opacity":-0.5}"##), "/layers/0/opacity", "from 0 to 1"),
        ("an opacity that is text", with(r##"{"fill":"#ffffff","opacity":"half"}"##), "/layers/0/opacity", "a number is wanted"),
        ("a huge number", with(r##"{"fill":"#ffffff","opacity":1e300}"##), "/layers/0/opacity", "from 0 to 1"),
        ("a number beyond a float", with(r##"{"fill":"#ffffff","opacity":1e999}"##), "", "not JSON"),
        ("a place that is a number", with(r##"{"fill":"#ffffff","place":3}"##), "/layers/0/place", "an object"),
        ("a place far away", with(r##"{"fill":"#ffffff","place":{"dx":1e300}}"##), "/layers/0/place/dx", "-1000000 to 1000000"),
        ("a place key nobody knows", with(r##"{"fill":"#ffffff","place":{"dz":1}}"##), "/layers/0/place/dz", "keys are"),
        ("a flip that is a number", with(r##"{"fill":"#ffffff","place":{"flip_x":1}}"##), "/layers/0/place/flip_x", "a bool"),
        ("an anchor outside its range", with(r##"{"fill":"#ffffff","place":{"anchor":[2,0]}}"##), "/layers/0/place/anchor", "0 to 1"),
        ("a fit without a box", with(r##"{"fill":"#ffffff","fit":{}}"##), "/layers/0/fit", "`box` is missing"),
        ("a fit box of three numbers", with(r##"{"fill":"#ffffff","fit":{"box":[1,2,3]}}"##), "/layers/0/fit/box", "[x, y, width, height]"),
        ("a fit box with no size", with(r##"{"fill":"#ffffff","fit":{"box":[0,0,0,5]}}"##), "/layers/0/fit/box", "width and a height above 0"),
        ("a fit word nobody knows", with(r##"{"fill":"#ffffff","fit":{"box":[0,0,4,4],"how":"fill"}}"##), "/layers/0/fit/how", "cover"),
        ("place and perspective", with(r##"{"fill":"#ffffff","place":{},"perspective":[[0,0],[1,0],[1,1],[0,1]]}"##), "/layers/0/perspective", "not both"),
        ("a bowtie", with(r##"{"fill":"#ffffff","perspective":[[0,0],[4,4],[4,0],[0,4]]}"##), "/layers/0/perspective", "convex"),
        ("three corners", with(r##"{"fill":"#ffffff","perspective":[[0,0],[1,0],[1,1]]}"##), "/layers/0/perspective", "four corners"),
        ("a name that is no name", with(r##"{"fill":"#ffffff","name":"a b"}"##), "/layers/0/name", "letters, digits"),
        ("a name twice", with(r##"{"name":"a","fill":"#ffffff"},{"name":"a","fill":"#000000"}"##), "/layers", "used once"),
        ("a default name at another place", with(r##"{"name":"layer2","fill":"#ffffff"}"##), "/layers", "default name"),
        ("a reference to nothing", with(r#"{"of":"ghost"}"#), "/layers", "ghost"),
        ("a reference cycle", with(r#"{"name":"a","of":"b"},{"name":"b","of":"a"}"#), "/layers", "a -> b -> a"),
        ("a reference with a fit", with(r##"{"name":"a","fill":"#ffffff"},{"of":"a","fit":{"box":[0,0,1,1]}}"##), "/layers/1/fit", "no fit of its own"),
        ("a mask that is no object", with(r##"{"fill":"#ffffff","masks":[1]}"##), "/layers/0/masks/0", "a mask is"),
        ("a mask by nothing", with(r##"{"fill":"#ffffff","masks":[{}]}"##), "/layers/0/masks/0", "`layer` or by a `map`"),
        ("a mask by a map the picture lacks", with(r#"{"picture":null,"masks":[{"map":"matte"}]}"#), "/layers/0/masks/0", "no map of this layer"),
        ("a mask word nobody knows", with(r##"{"fill":"#ffffff","masks":[{"layer":{"fill":"#ffffff"},"use":"red"}]}"##), "/layers/0/masks/0/use", "luminance"),
        ("a mask with a name", with(r##"{"fill":"#ffffff","masks":[{"layer":{"name":"m","fill":"#ffffff"}}]}"##), "/layers/0/masks/0", "no name"),
        ("a mask feather below 0", with(r##"{"fill":"#ffffff","masks":[{"layer":{"fill":"#ffffff"},"feather":-1}]}"##), "/layers/0/masks/0/feather", "0 to 1000000"),
        ("a look with a format nobody reads", with(r##"{"fill":"#ffffff","looks":[{"format":9,"recipe":{"steps":[]}}]}"##), "/layers/0/looks/0/format", "image formats 1, 2, 3"),
        ("a look that is no object", with(r##"{"fill":"#ffffff","looks":[3]}"##), "/layers/0/looks/0", "a look is"),
        ("a look without a recipe", with(r##"{"fill":"#ffffff","looks":[{}]}"##), "/layers/0/looks/0", "`recipe` is missing"),
        ("a look image refuses", with(r##"{"fill":"#ffffff","looks":[{"recipe":{"steps":[{"effect":"nothing_like_it"}]}}]}"##), "/layers/0/looks/0/recipe", "nothing_like_it"),
        ("a look that sets a size", with(r##"{"fill":"#ffffff","looks":[{"recipe":{"steps":[{"effect":"grade","params":{"exposure":1.0}}],"size":10}}]}"##), "/layers/0/looks/0/recipe", "size"),
        ("a drawing of a version vector does not read", with(r#"{"drawing":{"version":99,"root":[]}}"#), "/layers/0/drawing/version", "version 99"),
        ("a drawing that is a number", with(r#"{"drawing":3}"#), "/layers/0/drawing", "vector document"),
        ("a drawing without a frame", with(r#"{"drawing":{"version":1,"root":[{"path":"M0 0 L1 0 L1 1 Z"}]}}"#), "/layers/0/drawing", "frame"),
        ("a Param of a drawing the drawing lacks", with(&format!(r#"{{"drawing":{},"values":{{"spin":1}}}}"#, rectangle((4, 4), (0.0, 0.0), (4.0, 4.0), "#ff0000"))), "/layers/0/values", "spin"),
        ("a text without a font", with(r#"{"text":"Hi"}"#), "/layers/0", "`font`"),
        ("a font nobody has", with(r#"{"text":"Hi","font":"no_such_font"}"#), "/layers/0/font", "bricolage_grotesque"),
        ("a text without a glyph", with(r#"{"text":" ","font":"bricolage_grotesque"}"#), "/layers/0/text", "no glyph"),
        ("a text option nobody knows", with(r#"{"text":"Hi","font":"bricolage_grotesque","options":{"sizes":1}}"#), "/layers/0/options/sizes", "sizes"),
        ("a text option scene places", with(r#"{"text":"Hi","font":"bricolage_grotesque","options":{"anchor":1}}"#), "/layers/0/options/anchor", "anchor"),
        ("a picture that is a number", with(r#"{"picture":3}"#), "/layers/0/picture", "a file is"),
        ("a picture without a path", with(r#"{"picture":{"sha256":"ab"}}"#), "/layers/0/picture", "`path` is missing"),
        ("a path that is a number", with(r#"{"picture":{"path":3}}"#), "/layers/0/picture/path", "a path (text)"),
        ("a map name that is no name", with(r#"{"picture":null,"maps":{"a b":null}}"#), "/layers/0/maps/a b", "letters, digits"),
        ("a scene that is no scene", with(r#"{"scene":3}"#), "/layers/0/scene", "a scene is an object"),
        ("a scene of another version", with(r#"{"scene":{"version":2,"canvas":{"width":1,"height":1}}}"#), "/layers/0/scene/version", "scene version 2"),
        ("a scene deeper than MAX_DEPTH", deep, &deep_at, "at most 16 deep"),
        ("a mask layer deeper than MAX_DEPTH", masks, &masks_at, "at most 16 deep"),
        ("too many layers", many, "/layers", "at most 1000"),
    ];
    let mut wrong = Vec::new();
    for (what, document, pointer, words) in rows {
        match read(&document) {
            Err(Error::Format {
                pointer: found,
                message,
            }) if found == pointer && message.contains(words) => {}
            other => wrong.push(format!("{what}: wanted {pointer:?} and {words:?}, got {other:?}")),
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// --- hostile input -----------------------------------------------------------------------------------------------

/// Run `work` in a thread with a stack of 256 KB.
fn small_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T, Box<dyn std::error::Error>> {
    Ok(std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(work)?
        .join()
        .map_err(|_| "the thread overflowed its stack or panicked")?)
}

#[test]
fn a_nest_of_arrays_or_of_scenes_is_read_in_a_small_stack_and_refused() -> Outcome {
    let arrays = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    let objects = format!(r#"{}"x"{}"#, r#"{"a":"#.repeat(100_000), "}".repeat(100_000));
    let in_options = with(&format!(
        r#"{{"text":"Hi","font":"bricolage_grotesque","options":{{"axes":{}}}}}"#,
        "[".repeat(5_000)
    ));
    let scenes = nested(40);
    let outcomes = small_stack(move || {
        [arrays, objects, in_options, scenes]
            .iter()
            .map(|text| Scene::from_json(text, Path::new("")).map(|_| ()))
            .collect::<Vec<_>>()
    })?;
    for outcome in outcomes {
        assert!(matches!(outcome, Err(Error::Format { .. })), "{outcome:?}");
    }
    Ok(())
}

#[test]
fn a_nest_of_masks_or_of_scenes_as_deep_as_the_scene_allows_is_written_read_and_dropped_in_a_small_stack() -> Outcome {
    let result = small_stack(|| -> Result<bool, Error> {
        let mut layer = Layer::fill("#ffffff")?;
        for _ in 0..MAX_DEPTH - 1 {
            layer = Layer::fill("#ffffff")?.mask(layer)?;
        }
        let made = scene(4, 4, [layer])?;
        let text = made.to_json()?;
        let back = Scene::from_json(&text, Path::new(""))?;
        let mut group = scene(4, 4, [Layer::fill("#ffffff")?])?;
        for _ in 0..MAX_DEPTH {
            group = scene(4, 4, [Layer::scene(group)])?;
        }
        let nest = group.to_json()?;
        Ok(back == made && back.to_json()? == text && Scene::from_json(&nest, Path::new(""))? == group)
    })?;
    assert!(result?);
    Ok(())
}

#[test]
fn a_megabyte_of_layers_and_of_text_is_refused_or_read_without_trouble() -> Outcome {
    // a megabyte of whitespace and one layer
    let padded = format!("{}{}", " ".repeat(1 << 20), with(r##"{"fill":"#ffffff"}"##));
    assert_eq!(read(&padded)?.layers().len(), 1);
    // a megabyte of layers is refused by its count before one is built
    let layers = vec![r##"{"fill":"#ffffff"}"##; 60_000].join(",");
    assert!(layers.len() > 1 << 20);
    assert!(matches!(read(&with(&layers)), Err(Error::Format { pointer, .. }) if pointer == "/layers"));
    // a megabyte in one text
    let long = "x".repeat(1 << 20);
    let refused = read(&with(&format!(r#"{{"fill":"{long}"}}"#)));
    assert!(matches!(refused, Err(Error::Format { pointer, .. }) if pointer == "/layers/0/fill"));
    Ok(())
}

// --- properties ----------------------------------------------------------------------------------------------------

proptest! {
    /// An opacity written as the shortest decimal that reads back as the same 32-bit float reads back as the same one.
    #[test]
    fn an_opacity_comes_back_to_the_bit(opacity in 0.0_f32..=1.0) {
        let made = scene(2, 2, [Layer::fill("#ffffff")?.opacity(f64::from(opacity))?])?;
        let text = made.to_json()?;
        let back = read(&text)?;
        prop_assert!(back == made, "{}", text);
        prop_assert_eq!(back.parameters(), made.parameters());
    }

    /// Every number of a placement comes back exactly, however it is written.
    #[test]
    fn a_placement_comes_back_exactly(
        dx in -1e6_f64..1e6, dy in -1e6_f64..1e6, scale in 0.001_f64..1000.0, aspect in 0.001_f64..1000.0,
        rotate in 0.0_f64..360.0, skew in -89.0_f64..89.0, anchor in (0.0_f64..=1.0, 0.0_f64..=1.0), flip in any::<(bool, bool)>(),
    ) {
        let place = Place::new().dx(dx).dy(dy).scale(scale).aspect(aspect).rotate(Deg(rotate)).skew_x(Deg(skew)).skew_y(Deg(-skew))
            .flip_x(flip.0).flip_y(flip.1).anchor(anchor.0, anchor.1);
        let made = scene(2, 2, [Layer::fill("#ffffff")?.place(place)?])?;
        let text = made.to_json()?;
        let back = read(&text)?;
        prop_assert!(back == made, "{}", text);
        prop_assert_eq!(back.parameters(), made.parameters());
    }
}

#[test]
fn a_thousand_layers_round_trip() -> Outcome {
    let layers = (0..MAX_LAYERS).map(|at| Layer::fill(&format!("#{:06x}", at * 977 % 0x00ff_ffff))?.opacity(0.5));
    let made = scene(64, 64, layers.collect::<Result<Vec<_>, _>>()?)?;
    round_trips(&made, Path::new(""), true)
}
