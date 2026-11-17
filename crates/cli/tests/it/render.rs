//! `bt-scene render`: the picture it writes is the one the core renders, and a refusal is one line and exit 1.

use bullterrier_scene::{BleedFill, Layer, PdfOptions, Raster, Scene, Value, Values, WriteOptions};

use crate::common::{Outcome, Project, bt, decoded, text};

/// The bytes the core's own writer makes of `raster`: what a render of it must be, byte for byte.
fn written(raster: &Raster, path: &std::path::Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    raster.write(path)?;
    Ok(std::fs::read(path)?)
}

#[test]
fn it_writes_the_picture_the_core_renders_within_the_rounding_of_8_bits() -> Outcome {
    let project = Project::new()?;
    let out = project.file("out.png");
    let ran = bt(&["render", text(&project.file("scene.json")), text(&out)]);
    assert_eq!((ran.code, ran.out.trim(), ran.err.as_str()), (0, text(&out), ""));
    let expected = project.scene()?.render(64)?;
    let (width, height, bytes) = decoded(&out)?;
    assert_eq!((width, height), (expected.width(), expected.height()));
    let worst = expected
        .pixels()
        .iter()
        .zip(&bytes)
        .map(|(level, byte)| (level * 255.0 - f32::from(*byte)).abs())
        .fold(0.0, f32::max);
    assert!(worst <= 0.5 + 1e-3, "the largest difference is {worst} levels");
    Ok(())
}

#[test]
fn dpi_on_a_png_is_the_core_writers_dpi() -> Outcome {
    let project = Project::new()?;
    let scene = text(&project.file("scene.json")).to_owned();
    let out = project.file("dpi.png");
    let ran = bt(&["render", scene.as_str(), text(&out), "--dpi", "300"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let expected = project.file("expected.png");
    project
        .scene()?
        .render(64)?
        .write_with(&expected, &WriteOptions::new().dpi(300.0)?)?;
    assert_eq!(
        std::fs::read(&out)?,
        std::fs::read(&expected)?,
        "the core's file with its dpi"
    );
    Ok(())
}

#[test]
fn size_is_the_long_side() -> Outcome {
    let project = Project::new()?;
    let out = project.file("small.png");
    let ran = bt(&["render", text(&project.file("scene.json")), text(&out), "--size", "32"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let (width, height, _) = decoded(&out)?;
    assert_eq!((width, height), (32, 18));
    Ok(())
}

#[test]
fn set_gives_the_picture_of_with_values() -> Outcome {
    let project = Project::new()?;
    let scene = project.scene()?;
    let plain = written(&scene.render(64)?, &project.file("plain.png"))?;
    let rows: [(&str, &str, Value); 6] = [
        ("a number", "band.opacity=0.2", 0.2.into()),
        ("a colour", "title.color=#ff0000", "#ff0000".into()),
        ("a text", "title.text=Pilot", "Pilot".into()),
        ("a bool", "band.light=true", true.into()),
        ("a choice", "band.blend=multiply", "multiply".into()),
        (
            "a picture file",
            "photo.picture=other.png",
            project.file("other.png").into(),
        ),
    ];
    for (what, assignment, value) in rows {
        let address = assignment.split_once('=').map_or(assignment, |(address, _)| address);
        let expected = written(
            &scene.with_values(&Values::new().set(address, value))?.render(64)?,
            &project.file("expected.png"),
        )?;
        let out = project.file("out.png");
        let ran = bt(&[
            "render",
            text(&project.file("scene.json")),
            text(&out),
            "--set",
            assignment,
            "--base",
            text(project.folder.path()),
        ]);
        // a file given by a relative path is relative to the folder the command runs in: the test gives it whole
        let ran = if what == "a picture file" {
            let whole = format!("photo.picture={}", text(&project.file("other.png")));
            bt(&["render", text(&project.file("scene.json")), text(&out), "--set", &whole])
        } else {
            ran
        };
        assert_eq!(ran.code, 0, "{what}: {}", ran.err);
        assert!(
            std::fs::read(&out)? == expected,
            "{what}: not the picture with_values gives"
        );
        assert!(expected != plain, "{what}: the value changed nothing");
    }
    Ok(())
}

#[test]
fn set_twice_takes_the_last_and_two_addresses_both() -> Outcome {
    let project = Project::new()?;
    let scene = project.scene()?;
    let values = Values::new().set("band.opacity", 0.3).set("title.color", "#00ff00");
    let expected = written(&scene.with_values(&values)?.render(64)?, &project.file("expected.png"))?;
    let out = project.file("out.png");
    let ran = bt(&[
        "render",
        text(&project.file("scene.json")),
        text(&out),
        "--set",
        "band.opacity=0.9",
        "--set",
        "title.color=#00ff00",
        "--set",
        "band.opacity=0.3",
    ]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(std::fs::read(&out)? == expected, "the last value of an address wins");
    Ok(())
}

#[test]
fn a_refusal_is_one_error_line_naming_the_value_and_exit_1_and_no_file() -> Outcome {
    let project = Project::new()?;
    let scene = text(&project.file("scene.json")).to_owned();
    let rows: [(&str, Vec<&str>, &[&str]); 11] = [
        (
            "an unknown address",
            vec!["--set", "band.opacty=0.5"],
            &["band.opacty", "did you mean opacity"],
        ),
        (
            "a layer that is not there",
            vec!["--set", "banner.opacity=0.5"],
            &["banner", "band"],
        ),
        (
            "a word for a number",
            vec!["--set", "band.opacity=high"],
            &["band.opacity", "a number", "high"],
        ),
        (
            "a number out of range",
            vec!["--set", "band.opacity=7"],
            &["band.opacity", "7"],
        ),
        ("NaN", vec!["--set", "band.opacity=NaN"], &["band.opacity"]),
        (
            "NaN for an angle that wraps",
            vec!["--set", "title.rotate=NaN"],
            &["title.rotate"],
        ),
        (
            "a word for a bool",
            vec!["--set", "band.light=yes"],
            &["band.light", "true"],
        ),
        (
            "a picture that is not there",
            vec!["--set", "photo.picture=gone.png"],
            &["gone.png"],
        ),
        ("a size of 0", vec!["--size", "0"], &["size"]),
        ("a size over the limit", vec!["--size", "100000"], &["size"]),
        ("a negative size", vec!["--size=-5"], &["size"]),
    ];
    for (what, extra, named) in rows {
        let out = project.file("never.png");
        let mut args = vec!["render", scene.as_str(), text(&out)];
        args.extend(extra);
        let ran = bt(&args);
        assert_eq!(ran.code, 1, "{what}: {}", ran.err);
        assert_eq!(ran.out, "", "{what}");
        assert!(
            ran.err.starts_with("error: ") && ran.err.trim_end().lines().count() == 1,
            "{what}: {}",
            ran.err
        );
        for word in named {
            assert!(ran.err.contains(word), "{what}: `{word}` not in {}", ran.err);
        }
        assert!(!out.exists(), "{what}: a file was made");
    }
    Ok(())
}

#[test]
fn a_file_that_cannot_be_read_or_written_is_exit_1_naming_it() -> Outcome {
    let project = Project::new()?;
    let missing = project.file("gone.json");
    let ran = bt(&["render", text(&missing), text(&project.file("out.png"))]);
    assert_eq!(ran.code, 1);
    assert!(
        ran.err.starts_with("error: ") && ran.err.contains("gone.json"),
        "{}",
        ran.err
    );

    for name in ["nowhere/out.png", "out.bmp", "out"] {
        let out = project.file(name);
        let ran = bt(&["render", text(&project.file("scene.json")), text(&out)]);
        assert_eq!(ran.code, 1, "{name}");
        assert!(
            ran.err.starts_with("error: ") && ran.err.contains(name),
            "{name}: {}",
            ran.err
        );
        assert!(!out.exists(), "{name}");
    }
    Ok(())
}

#[test]
fn hostile_input_is_refused_in_one_short_line() -> Outcome {
    let project = Project::new()?;
    let out = text(&project.file("out.png")).to_owned();
    let scene = text(&project.file("scene.json")).to_owned();

    // a megabyte of padding around a good document is read and drawn
    let padded = project.file("padded.json");
    std::fs::write(
        &padded,
        format!("{}{}", " ".repeat(1 << 20), std::fs::read_to_string(&scene)?),
    )?;
    let ran = bt(&["render", text(&padded), &out, "--base", text(project.folder.path())]);
    assert_eq!(ran.code, 0, "{}", ran.err);

    // a megabyte of nesting and of noise is not a scene, and costs neither the stack nor a long message
    for (what, document) in [
        ("nesting", "[".repeat(1 << 20)),
        ("noise", "x".repeat(1 << 20)),
        ("nothing", String::new()),
    ] {
        let bad = project.file("bad.json");
        std::fs::write(&bad, document)?;
        let ran = bt(&["render", text(&bad), &out]);
        assert_eq!(ran.code, 1, "{what}");
        assert!(
            ran.err.starts_with("error: ") && ran.err.len() < 600,
            "{what}: {} bytes",
            ran.err.len()
        );
    }

    // an address and a value of a megabyte are cut in the message
    for assignment in [
        format!("{}=1", "a".repeat(1 << 20)),
        format!("band.opacity={}", "9".repeat(1 << 20)),
    ] {
        let ran = bt(&["render", &scene, &out, "--set", &assignment]);
        assert_eq!(ran.code, 1);
        assert!(
            ran.err.starts_with("error: ") && ran.err.len() < 3000,
            "{} bytes",
            ran.err.len()
        );
    }
    Ok(())
}

#[test]
fn a_pdf_extension_writes_the_core_pdf_of_the_flags_and_defaults_to_300_dpi() -> Outcome {
    let project = Project::new()?;
    let scene = text(&project.file("scene.json")).to_owned();
    let picture = project.scene()?.render(64)?;
    let rows: [(&str, Vec<&str>, PdfOptions); 5] = [
        ("no flags", vec![], PdfOptions::new(300.0)?),
        ("dpi", vec!["--dpi", "150"], PdfOptions::new(150.0)?),
        (
            "bleed and crop marks",
            vec!["--bleed", "3", "--crop-marks"],
            PdfOptions::new(300.0)?.bleed_mm(3.0)?.crop_marks(true),
        ),
        ("flatten", vec!["--flatten"], PdfOptions::new(300.0)?.flatten(true)),
        (
            "a mirrored bleed and registration marks",
            vec!["--bleed", "3", "--bleed-fill", "mirror", "--registration-marks"],
            PdfOptions::new(300.0)?
                .bleed_mm(3.0)?
                .bleed_fill(BleedFill::Mirror)
                .registration_marks(true),
        ),
    ];
    for (what, flags, options) in rows {
        let (out, expected) = (project.file("out.PDF"), project.file("expected.pdf"));
        picture.write_pdf(&expected, &options)?;
        let mut args = vec!["render", scene.as_str(), text(&out)];
        args.extend(flags);
        let ran = bt(&args);
        assert_eq!(
            (ran.code, ran.out.trim(), ran.err.as_str()),
            (0, text(&out), ""),
            "{what}"
        );
        assert!(
            std::fs::read(&out)? == std::fs::read(&expected)?,
            "{what}: not the core's PDF"
        );
    }
    // --size is the long side in pixels: 32 pixels at 300 dpi are 7.68 pt
    let (out, expected) = (project.file("small.pdf"), project.file("small-expected.pdf"));
    project
        .scene()?
        .render(32)?
        .write_pdf(&expected, &PdfOptions::new(300.0)?)?;
    let ran = bt(&["render", scene.as_str(), text(&out), "--size", "32"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        std::fs::read(&out)? == std::fs::read(&expected)?,
        "the core's PDF at the size"
    );
    Ok(())
}

#[test]
fn a_pdf_flag_on_a_picture_that_is_not_a_pdf_and_a_bad_pdf_value_are_exit_1_naming_the_flag_and_no_file() -> Outcome {
    let project = Project::new()?;
    let scene = text(&project.file("scene.json")).to_owned();
    let rows: [(&str, &str, Vec<&str>, &str); 12] = [
        ("dpi 0 on a PNG", "never.png", vec!["--dpi", "0"], "dpi"),
        ("dpi 65536 on a PNG", "never.png", vec!["--dpi", "65536"], "dpi"),
        ("bleed on a PNG", "never.png", vec!["--bleed", "3"], "bleed"),
        ("crop marks on a PNG", "never.png", vec!["--crop-marks"], "crop-marks"),
        (
            "bleed fill on a PNG",
            "never.png",
            vec!["--bleed-fill", "mirror"],
            "bleed-fill",
        ),
        (
            "registration marks on a PNG",
            "never.png",
            vec!["--registration-marks"],
            "registration-marks",
        ),
        ("flatten on a PNG", "never.png", vec!["--flatten"], "flatten"),
        (
            "a profile on a PNG",
            "never.png",
            vec!["--cmyk-profile", "x.icc"],
            "cmyk-profile",
        ),
        ("dpi 0", "never.pdf", vec!["--dpi", "0"], "dpi"),
        ("dpi NaN", "never.pdf", vec!["--dpi", "NaN"], "dpi"),
        ("a negative bleed", "never.pdf", vec!["--bleed=-1"], "bleed_mm"),
        (
            "a profile that is not there",
            "never.pdf",
            vec!["--cmyk-profile", "gone.icc"],
            "gone.icc",
        ),
    ];
    for (what, name, extra, named) in rows {
        let out = project.file(name);
        let mut args = vec!["render", scene.as_str(), text(&out)];
        args.extend(extra);
        let ran = bt(&args);
        assert_eq!(ran.code, 1, "{what}: {}", ran.err);
        assert!(
            ran.err.starts_with("error: ") && ran.err.contains(named),
            "{what}: {}",
            ran.err
        );
        assert!(!out.exists(), "{what}: a file was made");
    }
    let out = project.file("nowhere/out.pdf");
    let ran = bt(&["render", scene.as_str(), text(&out)]);
    assert_eq!(ran.code, 1);
    assert!(ran.err.contains("nowhere"), "{}", ran.err);
    Ok(())
}

#[test]
fn a_picture_past_a_band_is_a_png_or_refused_whatever_the_size_with_no_file() -> Outcome {
    // 4100 x 4100 is more than the 2^24 pixels written band by band, which only a PNG can be; a plain colour keeps it cheap
    let project = Project::new()?;
    let big = Scene::new(4100, 4100)?.add([Layer::fill("#336699")?])?;
    std::fs::write(project.file("big.json"), big.to_json()?)?;
    let scene = text(&project.file("big.json")).to_owned();
    for (name, flags) in [
        ("big.jpg", vec!["--dpi", "300"]),
        ("big.jpg", vec![]),
        ("big.webp", vec!["--dpi", "300"]),
        ("big.webp", vec![]),
    ] {
        let out = project.file(name);
        let mut args = vec!["render", scene.as_str(), text(&out)];
        args.extend(&flags);
        let ran = bt(&args);
        assert_eq!(ran.code, 1, "{name} {flags:?}: {}", ran.err);
        assert!(ran.err.contains(name) && ran.err.contains("PNG"), "{name}: {}", ran.err);
        assert!(!out.exists(), "{name}: a file was made");
    }
    let out = project.file("big.png");
    let ran = bt(&["render", scene.as_str(), text(&out), "--dpi", "300"]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let head = std::fs::read(&out)?;
    assert_eq!(&head[1..4], b"PNG");
    Ok(())
}

#[test]
fn a_dpi_for_a_webp_is_refused_at_any_size() -> Outcome {
    let project = Project::new()?;
    let out = project.file("small.webp");
    let ran = bt(&["render", text(&project.file("scene.json")), text(&out), "--dpi", "300"]);
    assert_eq!(ran.code, 1, "{}", ran.err);
    assert!(!out.exists());
    Ok(())
}
