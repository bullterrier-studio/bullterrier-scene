//! A saved scene's files given as bytes (`Files::given().with(..)`): the oracle is the same document read from a folder (the
//! same look, the same saved text), a table of files left out (each a read error at load naming the path as written), and a
//! digest that is not the given bytes' (a read error at render naming both).

use std::path::Path;

use bullterrier_scene::{Error, Files, LayerKinds, Raster, Scene};

use crate::common::{Outcome, made, rectangle, same_look, varied};
use crate::saved::write_picture;

/// `marks.ttf` of vector's tests, made by a fontTools script there: `B` is a 400 square on 1000 to the em.
const MARKS: &[u8] = include_bytes!("fonts/marks.ttf");
const NIGHT_TOML: &str = "[[step]]\neffect = \"grade\"\nexposure = 1.0\n";

/// A scene naming one file of each kind: a picture masked by its map, a drawing with a look file, a text in a font file.
const EVERY_KIND: &str = r##"{"version":1,"canvas":{"width":40,"height":20},"layers":[
    {"picture":{"path":"assets/photo.png"},"maps":{"matte":{"path":"assets/matte.png"}},
     "fit":{"box":[0,0,40,20],"how":"stretch"},"masks":[{"map":"matte"}]},
    {"drawing":"sheet.json","looks":["night.toml"],"opacity":0.7},
    {"text":"B","font":{"path":"marks.ttf"},"color":"#00ff00","options":{"size":12},"place":{"dx":3,"dy":16}}
]}"##;

/// Files by the path as a document writes it, with their bytes.
type Named = Vec<(&'static str, Vec<u8>)>;

/// The files `EVERY_KIND` names, written into `folder` (with `assets/`): their paths as written and their bytes.
fn every_file(folder: &Path) -> Result<Named, Box<dyn std::error::Error>> {
    std::fs::create_dir(folder.join("assets"))?;
    write_picture(&folder.join("assets/photo.png"), &varied(12, 8)?)?;
    let grey = made(12, 8, |x, _| {
        let level = f32::from(u8::try_from(x * 21).unwrap_or(0)) / 255.0;
        [level, level, level, 1.0]
    })?;
    write_picture(&folder.join("assets/matte.png"), &grey)?;
    std::fs::write(
        folder.join("sheet.json"),
        rectangle((40, 20), (5.0, 2.0), (30.0, 12.0), "#ff0000"),
    )?;
    std::fs::write(folder.join("night.toml"), NIGHT_TOML)?;
    std::fs::write(folder.join("marks.ttf"), MARKS)?;
    [
        "assets/photo.png",
        "assets/matte.png",
        "sheet.json",
        "night.toml",
        "marks.ttf",
    ]
    .into_iter()
    .map(|path| Ok((path, std::fs::read(folder.join(path))?)))
    .collect()
}

fn given(files: &[(&str, Vec<u8>)]) -> Files {
    files.iter().fold(Files::given(), |given, (path, bytes)| {
        given.with(*path, bytes.as_slice())
    })
}

fn from_given(text: &str, files: &Files) -> Result<Scene, Error> {
    Scene::from_json_with(text, files, &LayerKinds::new())
}

#[test]
fn a_scene_read_from_given_bytes_looks_and_saves_as_the_one_read_from_the_folder() -> Outcome {
    let folder = tempfile::tempdir()?;
    let bytes = every_file(folder.path())?;
    let from_folder = Scene::from_json(EVERY_KIND, folder.path())?;
    let from_bytes = from_given(EVERY_KIND, &given(&bytes))?;
    assert!(same_look(&from_bytes.render(80)?, &from_folder.render(80)?));
    // saved: the paths as written and the digests of the bytes, as the folder's scene writes them
    let saved = from_bytes.to_json()?;
    assert_eq!(saved, from_folder.to_json()?);
    let document: serde_json::Value = serde_json::from_str(&saved)?;
    assert_eq!(document["layers"][0]["picture"]["path"], "assets/photo.png");
    assert_eq!(document["layers"][0]["maps"]["matte"]["path"], "assets/matte.png");
    assert_eq!(document["layers"][2]["font"]["path"], "marks.ttf");
    Ok(())
}

#[test]
fn a_file_the_given_files_lack_is_a_read_error_at_load_naming_the_path_as_written() {
    let layer = |layer: &str| format!(r#"{{"version":1,"canvas":{{"width":4,"height":4}},"layers":[{layer}]}}"#);
    let cases = [
        (
            "picture",
            layer(r#"{"picture":{"path":"assets/dog.png"}}"#),
            "assets/dog.png",
        ),
        (
            "map",
            layer(r#"{"picture":{"path":"dog.png"},"maps":{"depth":{"path":"assets/depth.png"}}}"#),
            "assets/depth.png",
        ),
        (
            "font",
            layer(r#"{"text":"B","font":{"path":"marks.ttf"}}"#),
            "marks.ttf",
        ),
        (
            "look",
            layer(r##"{"fill":"#ff0000","looks":["night.toml"]}"##),
            "night.toml",
        ),
        ("drawing", layer(r#"{"drawing":"sheet.json"}"#), "sheet.json"),
        (
            "not normalised",
            layer(r#"{"picture":{"path":"./dog.png"}}"#),
            "./dog.png",
        ),
    ];
    let files = Files::given().with("dog.png", [0_u8; 4].as_slice());
    for (what, text, written) in cases {
        let refused = from_given(&text, &files);
        assert!(
            matches!(&refused, Err(Error::Read { path, message })
                if path == written && message.contains("no file of this name was given")),
            "{what}: {refused:?}"
        );
    }
}

#[test]
fn given_bytes_that_are_not_the_saved_digest_are_a_read_error_at_render_naming_both() -> Outcome {
    let folder = tempfile::tempdir()?;
    write_picture(&folder.path().join("photo.png"), &varied(12, 8)?)?;
    let files = Files::given().with("photo.png", std::fs::read(folder.path().join("photo.png"))?);
    let text = r#"{"version":1,"canvas":{"width":12,"height":8},"layers":[{"picture":{"path":"photo.png"}}]}"#;
    let saved: serde_json::Value = serde_json::from_str(&from_given(text, &files)?.to_json()?)?;
    let digest = saved["layers"][0]["picture"]["sha256"]
        .as_str()
        .ok_or("no digest")?
        .to_owned();
    let wrong = "0".repeat(64);
    let tampered = text.replace(r#""photo.png""#, &format!(r#""photo.png","sha256":"{wrong}""#));
    let refused = from_given(&tampered, &files)?.render(12);
    assert!(
        matches!(&refused, Err(Error::Read { path, message })
            if path == "photo.png" && message.contains(&wrong) && message.contains(&digest)),
        "{refused:?}"
    );
    let kept = text.replace(r#""photo.png""#, &format!(r#""photo.png","sha256":"{digest}""#));
    let drawn: Raster = from_given(&kept, &files)?.render(12)?;
    assert!(same_look(&drawn, &varied(12, 8)?));
    Ok(())
}

/// A look file is read by image's one rule, given or in a folder: TOML whatever its name, named after the file when the
/// recipe names itself not; JSON text is refused both ways.
#[test]
fn a_given_look_file_is_read_as_the_same_file_in_a_folder_is() -> Outcome {
    let folder = tempfile::tempdir()?;
    let json = r#"{"steps":[{"effect":"grade","params":{"exposure":1.0}}]}"#;
    std::fs::write(folder.path().join("night.toml"), NIGHT_TOML)?;
    std::fs::write(folder.path().join("night.json"), json)?;
    let files = Files::given()
        .with("night.toml", NIGHT_TOML.as_bytes())
        .with("night.json", json.as_bytes());
    let with_look = |look: &str| {
        format!(
            r##"{{"version":1,"canvas":{{"width":8,"height":8}},"layers":[{{"fill":"#406080","looks":["{look}"]}}]}}"##
        )
    };
    let by_folder = Scene::from_json(&with_look("night.toml"), folder.path())?;
    let by_bytes = from_given(&with_look("night.toml"), &files)?;
    let saved = by_bytes.to_json()?;
    assert_eq!(saved, by_folder.to_json()?);
    let document: serde_json::Value = serde_json::from_str(&saved)?;
    assert_eq!(document["layers"][0]["looks"][0]["recipe"]["name"], "night");
    let refused = |read: Result<Scene, Error>| matches!(read, Err(Error::Invalid { address, .. }) if address == "look");
    assert!(refused(Scene::from_json(&with_look("night.json"), folder.path())));
    assert!(
        refused(from_given(&with_look("night.json"), &files)),
        "JSON text in a look file is refused from a folder"
    );
    Ok(())
}
