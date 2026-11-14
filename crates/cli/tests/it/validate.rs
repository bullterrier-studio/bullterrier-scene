//! `bt-scene validate`: exit 0 for a scene that can be rendered, exit 1 with the library's message for one that cannot.

use crate::common::{Outcome, Project, bt, photo, text};

#[test]
fn a_good_scene_is_ok_and_writes_nothing() -> Outcome {
    let project = Project::new()?;
    let before = std::fs::read_dir(project.folder.path())?.count();
    let ran = bt(&["validate", text(&project.file("scene.json"))]);
    assert_eq!(
        (ran.code, ran.out.trim(), ran.err.as_str()),
        (0, "ok: 64 x 36, 3 layers", "")
    );
    assert_eq!(std::fs::read_dir(project.folder.path())?.count(), before);
    Ok(())
}

#[test]
fn a_scene_that_cannot_be_rendered_is_exit_1_with_the_reason() -> Outcome {
    let project = Project::new()?;
    let document = std::fs::read_to_string(project.file("scene.json"))?;
    let scene = text(&project.file("scene.json")).to_owned();

    // the order matters: each row starts from the good project and breaks one thing
    let broken: [(&str, String, &[&str]); 5] = [
        ("not JSON", "{".to_owned(), &["not JSON"]),
        (
            "a value out of range",
            document.replacen("\"opacity\":0.82", "\"opacity\":7", 1),
            &["/layers/1/opacity"],
        ),
        (
            "a version it does not read",
            document.replacen("\"version\":1", "\"version\":9", 1),
            &["version"],
        ),
        (
            "a key it does not know",
            document.replacen("\"layers\"", "\"layres\"", 1),
            &["layres"],
        ),
        (
            "a picture file that is gone",
            document.replace("photo.png", "gone.png"),
            &["gone.png"],
        ),
    ];
    for (what, text_of_scene, named) in broken {
        std::fs::write(project.file("broken.json"), &text_of_scene)?;
        let ran = bt(&[
            "validate",
            text(&project.file("broken.json")),
            "--base",
            text(project.folder.path()),
        ]);
        assert_eq!(ran.code, 1, "{what}: {}", ran.out);
        assert!(ran.err.starts_with("error: "), "{what}: {}", ran.err);
        for word in named {
            assert!(ran.err.contains(word), "{what}: `{word}` not in {}", ran.err);
        }
    }

    // the picture changed after the scene was saved: the digest says so
    photo(&project.file("photo.png"), (10, 10))?;
    let ran = bt(&["validate", &scene]);
    assert_eq!(ran.code, 1);
    assert!(
        ran.err.contains("sha256") && ran.err.contains("photo.png"),
        "{}",
        ran.err
    );

    let ran = bt(&["validate", text(&project.file("gone.json"))]);
    assert_eq!(ran.code, 1);
    assert!(ran.err.contains("gone.json"), "{}", ran.err);
    Ok(())
}

#[test]
fn relative_paths_are_relative_to_the_scene_file_unless_base_says_otherwise() -> Outcome {
    let project = Project::new()?;
    let elsewhere = tempfile::tempdir()?;
    let scene = text(&project.file("scene.json")).to_owned();
    let ran = bt(&["validate", &scene, "--base", text(elsewhere.path())]);
    assert_eq!(ran.code, 1);
    assert!(ran.err.contains("photo.png"), "{}", ran.err);
    Ok(())
}
