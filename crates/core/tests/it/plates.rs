//! `Scene::plates`: one plane of ink coverage for each ink. The oracles are worked out outside the code under test: the
//! overprint rule of print (a channel is `paper * prod (1 - coverage * (1 - ink / paper))`, written in the test), a hand-made union of
//! two shapes, and the composite the scene draws itself.

use bullterrier_scene::{Error, Fit, Layer, Plates, Raster, Scene};
use serde_json::json;

use crate::common::{Outcome, made, scene};

/// `#f4efe4`, `#e8505b` and `#2a6fb5` as the levels they are.
const PAPER: [f32; 3] = [244.0 / 255.0, 239.0 / 255.0, 228.0 / 255.0];
const PINK: [f32; 3] = [232.0 / 255.0, 80.0 / 255.0, 91.0 / 255.0];
const BLUE: [f32; 3] = [42.0 / 255.0, 111.0 / 255.0, 181.0 / 255.0];

/// A halftone look that prints `inks` over `paper` as spot inks, with every print-realism option off (a speck of bare paper is
/// not on a plate, so the plates reproduce the picture only without them).
fn halftone(inks: &[&str], paper: &str) -> String {
    json!({"steps": [{"effect": "halftone", "params": {
        "separation": "spot", "inks": inks, "paper": paper, "cell": 4.0, "shape": "dot"
    }}]})
    .to_string()
}

/// A 24 by 16 picture whose colour changes across it, so that each ink has some coverage nearly everywhere.
fn photo() -> Result<Raster, Error> {
    made(24, 16, |x, y| {
        let (u, v) = (
            f32::from(u8::try_from(x).unwrap_or(0)) / 23.0,
            f32::from(u8::try_from(y).unwrap_or(0)) / 15.0,
        );
        [0.35 + 0.5 * u, 0.4 + 0.4 * v, 0.55 + 0.3 * (1.0 - u), 1.0]
    })
}

/// A scene that is the photo printed in two spot inks.
fn print() -> Result<Scene, Error> {
    scene(
        24,
        16,
        [Layer::picture(photo()?)
            .look(&halftone(&["#e8505b", "#2a6fb5"], "#f4efe4"))?
            .inks(["pink", "blue"])?],
    )
}

/// The plane `name` of `plates`.
fn plane<'a>(plates: &'a Plates, name: &str) -> Result<&'a [f32], Box<dyn std::error::Error>> {
    plates.plane(name).ok_or_else(|| format!("no plate {name}").into())
}

#[test]
fn plates_laid_over_the_paper_by_the_overprint_rule_are_the_composite() -> Outcome {
    let drawn = print()?;
    let composite = drawn.render(24)?;
    let plates = drawn.plates(24)?;
    assert_eq!(plates.names(), ["pink", "blue"]);
    assert_eq!(
        (plates.width(), plates.height()),
        (composite.width(), composite.height())
    );
    let (pink, blue) = (plane(&plates, "pink")?, plane(&plates, "blue")?);
    assert_eq!((pink.len(), blue.len()), (24 * 16, 24 * 16));
    let mut worst = 0.0_f32;
    let (mut pink_total, mut blue_total) = (0.0_f32, 0.0_f32);
    for (at, pixel) in composite.pixels().as_chunks::<4>().0.iter().enumerate() {
        for channel in 0..3 {
            // an ink over its own paper is the ink's colour: it lets the share `ink / paper` of the light through
            let through = |ink: [f32; 3]| ink[channel] / PAPER[channel];
            let expected =
                PAPER[channel] * (1.0 - pink[at] * (1.0 - through(PINK))) * (1.0 - blue[at] * (1.0 - through(BLUE)));
            worst = worst.max((pixel[channel] - expected).abs());
        }
        pink_total += pink[at];
        blue_total += blue[at];
    }
    // the inks of the two plates are in the picture: neither plate is empty, and neither is solid
    assert!(pink_total > 20.0 && blue_total > 20.0, "{pink_total} {blue_total}");
    assert!(pink_total < 24.0 * 16.0 - 20.0 && blue_total < 24.0 * 16.0 - 20.0);
    assert!(worst < 1e-4, "the largest difference from the overprint is {worst}");
    Ok(())
}

#[test]
fn a_layer_with_no_look_is_one_shape_in_one_ink_and_shapes_in_one_ink_overprint_as_a_union() -> Outcome {
    // a box from column 2 to 10 and row 2 to 6 at full strength, and one from 6 to 14 and 4 to 8 at half: where they meet the
    // coverage is 1 - (1 - 1)(1 - 0.5) = 1, and the half box alone is 0.5; a third shape of half strength over the half box
    // makes 1 - 0.5 * 0.5 = 0.75
    let drawn = scene(
        16,
        10,
        [
            Layer::fill("#123456")?
                .fit([2.0, 2.0, 8.0, 4.0], Fit::Stretch, [0.5, 0.5])?
                .inks(["black"])?,
            Layer::fill("#abcdef")?
                .fit([6.0, 4.0, 8.0, 4.0], Fit::Stretch, [0.5, 0.5])?
                .opacity(0.5)?
                .inks(["black"])?,
            Layer::fill("#abcdef")?
                .fit([12.0, 4.0, 2.0, 2.0], Fit::Stretch, [0.5, 0.5])?
                .opacity(0.5)?
                .inks(["red"])?,
            Layer::fill("#abcdef")?
                .fit([12.0, 4.0, 2.0, 2.0], Fit::Stretch, [0.5, 0.5])?
                .opacity(0.5)?
                .inks(["red"])?,
        ],
    )?
    .plates(16)?;
    assert_eq!(drawn.names(), ["black", "red"]);
    let black = plane(&drawn, "black")?;
    let at = |x: usize, y: usize| y * 16 + x;
    let expected = |x: usize, y: usize| -> f32 {
        let full = (2..10).contains(&x) && (2..6).contains(&y);
        let half = (6..14).contains(&x) && (4..8).contains(&y);
        match (full, half) {
            (true, _) => 1.0,
            (false, true) => 0.5,
            (false, false) => 0.0,
        }
    };
    for y in 0..10 {
        for x in 0..16 {
            assert!(
                (black[at(x, y)] - expected(x, y)).abs() < 1e-6,
                "black at ({x}, {y}): {}",
                black[at(x, y)]
            );
        }
    }
    let red = plane(&drawn, "red")?;
    for y in 0..10 {
        for x in 0..16 {
            let inside = (12..14).contains(&x) && (4..6).contains(&y);
            let want = if inside { 0.75 } else { 0.0 };
            assert!(
                (red[at(x, y)] - want).abs() < 1e-6,
                "red at ({x}, {y}): {}",
                red[at(x, y)]
            );
        }
    }
    Ok(())
}

#[test]
fn plates_and_the_composite_are_the_same_whichever_is_drawn_first() -> Outcome {
    let first = print()?;
    let composite = first.render(24)?;
    let plates = first.plates(24)?;
    // the other order, on a scene made apart (its own cache)
    let second = print()?;
    assert_eq!(second.plates(24)?, plates, "plates first");
    assert_eq!(
        second.render(24)?,
        composite,
        "then the composite, which the plates' draws must not have changed"
    );
    // and the same scene again, its cache full
    assert_eq!(first.plates(24)?, plates);
    assert_eq!(first.render(24)?, composite);
    Ok(())
}

#[test]
fn a_scene_that_asks_for_plates_wrongly_is_refused_naming_inks_and_the_layer() -> Outcome {
    let two = halftone(&["#e8505b", "#2a6fb5"], "#f4efe4");
    let grain = json!({"steps": [{"effect": "grain", "params": {"amount": 0.1}}]}).to_string();
    let rows: Vec<(&str, Scene, &str)> = vec![
        ("no layer names an ink", scene(4, 4, [Layer::fill("#ffffff")?])?, "inks"),
        (
            "three inks named and the look makes two planes",
            scene(24, 16, [Layer::picture(photo()?).look(&two)?.inks(["a", "b", "c"])?])?,
            "inks",
        ),
        (
            "two inks named by a layer with no look",
            scene(4, 4, [Layer::fill("#ffffff")?.inks(["a", "b"])?])?,
            "inks",
        ),
        (
            "a look that separates nothing",
            scene(24, 16, [Layer::picture(photo()?).look(&grain)?.inks(["a"])?])?,
            "inks",
        ),
        (
            "a light layer",
            scene(4, 4, [Layer::fill("#ffffff")?.as_light(true).inks(["a"])?])?,
            "inks",
        ),
    ];
    for (what, drawn, named) in rows {
        let error = drawn.plates(24).err();
        assert!(
            matches!(&error, Some(Error::Invalid { address, .. }) if address.contains(named)),
            "{what}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn ink_names_that_are_not_allowed_are_refused_naming_inks() {
    let rows: [(&str, Vec<&str>); 6] = [
        ("none", vec![]),
        ("nine", vec!["a", "b", "c", "d", "e", "f", "g", "h", "i"]),
        ("twice", vec!["a", "a"]),
        ("a slash", vec!["a/b"]),
        ("a leading digit", vec!["1a"]),
        ("empty", vec![""]),
    ];
    for (what, names) in rows {
        let error = Layer::fill("#ffffff").and_then(|layer| layer.inks(names)).err();
        assert!(
            matches!(&error, Some(Error::Invalid { address, .. }) if address == "inks"),
            "{what}: {error:?}"
        );
    }
}

#[test]
fn a_size_that_is_wrong_or_would_hold_too_many_values_is_refused_naming_size_before_drawing() -> Outcome {
    let drawn = print()?;
    for size in [0_i64, -5, 65_001, i64::MAX] {
        let error = drawn.plates(size).err();
        assert!(
            matches!(&error, Some(Error::Invalid { address, .. }) if address == "size"),
            "{size}: {error:?}"
        );
    }
    // 65,000 by 43,333 pixels is 2.8e9 values of one plate: past what plates hold, said before a pixel is drawn
    let wide = scene(65_000, 43_333, [Layer::fill("#000000")?.inks(["a"])?])?;
    let error = wide.plates(65_000).err();
    assert!(
        matches!(&error, Some(Error::Invalid { address, .. }) if address == "size"),
        "{error:?}"
    );
    Ok(())
}

// ---- the files ---------------------------------------------------------------------------------------------------------

use std::path::Path;

use bullterrier_scene::{BleedFill, PdfOptions};
use lopdf::{Document, Object};

use crate::common::physical;

/// A canvas of 4 by 2 with a `black` shape in column 0 and a half-strength `red` one in column 3, drawn at size 4 (a pixel
/// each): the plates are `black` 1 0 0 0 and `red` 0 0 0 0.5 on both rows.
fn two_columns() -> Result<Plates, Box<dyn std::error::Error>> {
    let shape =
        |at: f64| Layer::fill("#123456").and_then(|layer| layer.fit([at, 0.0, 1.0, 2.0], Fit::Stretch, [0.5, 0.5]));
    Ok(scene(
        4,
        2,
        [shape(0.0)?.inks(["black"])?, shape(3.0)?.opacity(0.5)?.inks(["red"])?],
    )?
    .plates(4)?)
}

/// 254 dots per inch is 10 pixels a millimetre; a bleed of 0.2 mm is 2 pixels.
fn bled(fill: BleedFill) -> Result<PdfOptions, Error> {
    Ok(PdfOptions::new(254.0)?.bleed_mm(0.2)?.bleed_fill(fill))
}

/// One row of the plates as the 8-bit grey a file holds (255 paper, 0 full ink), by hand. The bleed is 2 pixels: the places
/// -2 and -1 and 4 and 5 of a row of 4. Mirrored, they come from columns 1 0 and 3 2; repeated, from 0 0 and 3 3.
const BLACK_MIRRORED: [u8; 8] = [255, 0, 0, 255, 255, 255, 255, 255];
const BLACK_REPEATED: [u8; 8] = [0, 0, 0, 255, 255, 255, 255, 255];
const RED_MIRRORED: [u8; 8] = [255, 255, 255, 255, 255, 127, 127, 255];
const RED_REPEATED: [u8; 8] = [255, 255, 255, 255, 255, 127, 127, 127];

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a level rounded to 0..=255"
)]
/// The grey a PNG holds at each pixel (its colour type is checked to be grey, 8 bits) with its size.
fn grey(path: &Path) -> Result<(usize, usize, Vec<u8>), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    assert_eq!((bytes[25], bytes[24]), (0, 8), "{}: an 8-bit grey PNG", path.display());
    let image = bullterrier_image::read(path)?;
    let (width, height, step) = (image.width(), image.height(), image.channels());
    let values = image.into_values();
    assert_eq!(values.len(), width * height * step);
    Ok((
        width,
        height,
        values
            .chunks(step)
            .map(|pixel| (pixel[0] * 255.0).round() as u8)
            .collect(),
    ))
}

#[test]
fn a_plate_png_is_grey_with_ink_black_and_a_mirrored_bleed_folded_at_the_edge() -> Outcome {
    let folder = tempfile::tempdir()?;
    for (fill, black, red) in [
        (BleedFill::Mirror, BLACK_MIRRORED, RED_MIRRORED),
        (BleedFill::Repeat, BLACK_REPEATED, RED_REPEATED),
    ] {
        let written = two_columns()?.write_png(&folder.path().join("poster"), &bled(fill)?)?;
        let names: Vec<String> = written
            .iter()
            .map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(names, ["poster-black.png", "poster-red.png"]);
        for (path, row) in written.iter().zip([black, red]) {
            let (width, height, values) = grey(path)?;
            assert_eq!((width, height), (8, 6), "the 4 by 2 plate and 2 pixels of bleed a side");
            for (at, value) in values.iter().enumerate() {
                assert_eq!(*value, row[at % 8], "{fill:?} {}: pixel {at}", path.display());
            }
            // the same dpi the options say: 254 dots per inch is 10,000 pixels a metre
            assert_eq!(physical(path)?, Some((10_000, 10_000, 1)));
        }
    }
    Ok(())
}

/// The margin of the marks as the options leave it: 8 mm, which is 80 pixels at 254 dpi, outside the bleed.
fn marked() -> Result<PdfOptions, Error> {
    Ok(bled(BleedFill::Mirror)?.registration_marks(true))
}

#[test]
fn registration_marks_lie_at_the_same_pixels_on_every_plate_and_nowhere_else() -> Outcome {
    let folder = tempfile::tempdir()?;
    let plain = two_columns()?.write_png(&folder.path().join("plain"), &bled(BleedFill::Mirror)?)?;
    let with = two_columns()?.write_png(&folder.path().join("marked"), &marked()?)?;
    let side = 4 + 2 * (2 + 80);
    let rows = 2 + 2 * (2 + 80);
    let (black, red) = (grey(&with[0])?, grey(&with[1])?);
    assert_eq!(
        (black.0, black.1),
        (side, rows),
        "the plate, its bleed and 8 mm of margin on each side"
    );
    // every pixel outside the picture and its bleed is the same on both plates: the marks, and only the marks
    let inside = |at: usize| {
        let (x, y) = (at % side, at / side);
        (80..side - 80).contains(&x) && (80..rows - 80).contains(&y)
    };
    let mut dark = 0;
    for at in 0..side * rows {
        if !inside(at) {
            assert_eq!(black.2[at], red.2[at], "pixel {at} differs between plates");
            dark += usize::from(black.2[at] < 255);
        }
    }
    assert!(dark > 100, "{dark} pixels of mark");
    // the picture and its bleed are the plain plates' (the margin is added around it)
    let plain_black = grey(&plain[0])?;
    for y in 0..plain_black.1 {
        for x in 0..plain_black.0 {
            assert_eq!(black.2[(y + 80) * side + x + 80], plain_black.2[y * plain_black.0 + x]);
        }
    }
    // by hand: the middle of each side has its target centred 6 mm (60 pixels: 0.5 mm of bleed is 5, here 0.2 mm is 2, then 3 + 2.5
    // mm) from the trim: the left one at column 82 - 2 - 55 = 25 ... measured from the image's left edge, the trim is at column
    // 82; the target's centre is (0.2 + 5.5) mm = 57 pixels out, at column 25, and its cross is 25 pixels each way along the row
    // 82 - 1 + (rows is 166, the middle 83): a pixel on the arm 10 pixels from the centre is ink
    let centre_row = rows / 2;
    assert!(black.2[centre_row * side + 25 + 10] < 200 || black.2[(centre_row - 1) * side + 25 + 10] < 200);
    // the corner of the page, where no mark is, is paper
    assert_eq!(black.2[0], 255);
    assert_eq!(black.2[side * rows - 1], 255);
    Ok(())
}

/// A page of a PDF of plates: its image's width, height and grey, and the operations of its content.
type PdfPage = (usize, usize, Vec<u8>, Vec<lopdf::content::Operation>);

/// A PDF of plates read back: the page tree's image streams, as grey bytes, and the labels of the pages.
fn pdf_pages(path: &Path) -> Result<Vec<PdfPage>, Box<dyn std::error::Error>> {
    let document = Document::load(path)?;
    let mut found = Vec::new();
    for id in document.get_pages().values() {
        let page = document.get_dictionary(*id)?;
        let Object::Dictionary(resources) = document.dereference(page.get(b"Resources")?)?.1 else {
            return Err("no resources".into());
        };
        let Object::Dictionary(objects) = document.dereference(resources.get(b"XObject")?)?.1 else {
            return Err("no x-objects".into());
        };
        let (_, reference) = objects.iter().next().ok_or("no picture")?;
        let Object::Stream(image) = document.dereference(reference)?.1 else {
            return Err("not a stream".into());
        };
        assert_eq!(image.dict.get(b"ColorSpace")?.as_name()?, b"DeviceGray");
        assert_eq!(image.dict.get(b"BitsPerComponent")?.as_i64()?, 8);
        let side = |key: &[u8]| -> Result<usize, Box<dyn std::error::Error>> {
            Ok(usize::try_from(image.dict.get(key)?.as_i64()?)?)
        };
        let operations = lopdf::content::Content::decode(&document.get_page_content(*id))?.operations;
        found.push((
            side(b"Width")?,
            side(b"Height")?,
            image.decompressed_content()?,
            operations,
        ));
    }
    Ok(found)
}

/// The page labels of a PDF, in page order.
fn labels(path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let document = Document::load(path)?;
    let catalog = document.catalog()?;
    let Object::Dictionary(labels) = document.dereference(catalog.get(b"PageLabels")?)?.1 else {
        return Err("no page labels".into());
    };
    let Object::Array(numbers) = document.dereference(labels.get(b"Nums")?)?.1 else {
        return Err("no Nums".into());
    };
    let mut names = Vec::new();
    for (at, pair) in numbers.chunks(2).enumerate() {
        assert_eq!(pair[0].as_i64()?, i64::try_from(at)?, "page numbers run from 0");
        let Object::Dictionary(label) = &pair[1] else {
            return Err("a label is a dictionary".into());
        };
        names.push(String::from_utf8(label.get(b"P")?.as_str()?.to_vec())?);
    }
    Ok(names)
}

#[test]
fn a_plate_pdf_has_a_page_for_each_ink_named_by_it_with_the_grey_of_the_png_and_the_same_marks() -> Outcome {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("plates.pdf");
    two_columns()?.write_pdf(&path, &marked()?)?;
    assert_eq!(labels(&path)?, ["black", "red"]);
    let pages = pdf_pages(&path)?;
    assert_eq!(pages.len(), 2);
    for ((width, height, bytes, _), row) in pages.iter().zip([BLACK_MIRRORED, RED_MIRRORED]) {
        // the PDF image is the plate and its bleed (the marks are vector paths in the margin)
        assert_eq!((*width, *height), (8, 6));
        for (at, value) in bytes.iter().enumerate() {
            assert_eq!(*value, row[at % 8], "pixel {at}");
        }
    }
    // the marks are the same strokes at the same points on every page: 4 targets of a cross (2 lines) and a circle
    let strokes = |operations: &[lopdf::content::Operation]| -> Vec<String> {
        operations
            .iter()
            .filter(|operation| ["m", "l", "S"].contains(&operation.operator.as_str()))
            .map(|operation| format!("{} {:?}", operation.operator, operation.operands))
            .collect()
    };
    let (first, second) = (strokes(&pages[0].3), strokes(&pages[1].3));
    assert_eq!(first, second);
    assert_eq!(
        first.iter().filter(|line| line.starts_with('S')).count(),
        12,
        "three strokes at each of four targets"
    );
    Ok(())
}

#[test]
fn plate_files_are_refused_naming_the_argument_and_leave_no_file() -> Outcome {
    let folder = tempfile::tempdir()?;
    let plates = two_columns()?;
    let profile = PdfOptions::new(254.0)?.cmyk_profile("x.icc");
    let error = plates.write_pdf(&folder.path().join("a.pdf"), &profile).err();
    assert!(
        matches!(&error, Some(Error::Invalid { address, .. }) if address == "cmyk_profile"),
        "{error:?}"
    );
    let error = plates.write_png(&folder.path().join("a"), &profile).err();
    assert!(
        matches!(&error, Some(Error::Invalid { address, .. }) if address == "cmyk_profile"),
        "{error:?}"
    );
    // a page past what a reader takes: the dpi is named, as for a picture's PDF
    // 300 pixels at 1 dpi are 300 in, past the 200 in of a PDF page
    let wide = scene(300, 10, [Layer::fill("#000000")?.inks(["a"])?])?.plates(300)?;
    let error = wide
        .write_pdf(&folder.path().join("b.pdf"), &PdfOptions::new(1.0)?)
        .err();
    assert!(
        matches!(&error, Some(Error::Invalid { address, .. }) if address == "dpi"),
        "{error:?}"
    );
    let error = plates
        .write_png(&folder.path().join("nowhere").join("a"), &bled(BleedFill::Repeat)?)
        .err();
    assert!(matches!(&error, Some(Error::Write { .. })), "{error:?}");
    let error = plates
        .write_pdf(&folder.path().join("nowhere").join("a.pdf"), &bled(BleedFill::Repeat)?)
        .err();
    assert!(matches!(&error, Some(Error::Write { .. })), "{error:?}");
    assert_eq!(std::fs::read_dir(folder.path())?.count(), 0, "nothing is left");
    Ok(())
}

// ---- the white underbase ------------------------------------------------------------------------------------------------

use bullterrier_scene::Underbase;

/// The standard normal's cumulative distribution (Abramowitz and Stegun 7.1.26: an error under 1.5e-7): what a feather of
/// `sigma` makes of a step, worked out outside the blur.
fn normal_cdf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.327_591_1 * (x.abs() / std::f64::consts::SQRT_2));
    let poly =
        t * (0.254_829_592 + t * (-0.284_496_736 + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1.0 - poly * (-(x * x) / 2.0).exp();
    f64::midpoint(1.0, if x >= 0.0 { erf } else { -erf })
}

const GARMENT: &str = "#202020";

/// A garment of `GARMENT`, a red 5 by 5 box at (2, 2) of a 9 by 9 canvas, printed in the ink `red`.
fn red_box() -> Result<Scene, Error> {
    scene(
        9,
        9,
        [
            Layer::fill(GARMENT)?,
            Layer::fill("#ff0000")?
                .fit([2.0, 2.0, 5.0, 5.0], Fit::Stretch, [0.5, 0.5])?
                .inks(["red"])?,
        ],
    )
}

#[test]
fn the_underbase_is_the_union_of_the_inks_choked_by_a_width_and_comes_first() -> Outcome {
    // row 5 of a 10 pixel canvas with a box of ink at columns 2 to 7: a pixel of it is 1, 2, 3, 3, 2, 1 pixels from the nearest
    // bare pixel, and the ink's edge lies half a pixel nearer than that. The white is drawn in from the edge by the choke, so
    // it is clamp(distance - choke): a choke of 0.5 takes half off the rim, one pixel takes the rim whole
    let box_of_ink = || {
        scene(
            10,
            10,
            [
                Layer::fill(GARMENT)?,
                Layer::fill("#ff0000")?
                    .fit([2.0, 2.0, 6.0, 6.0], Fit::Stretch, [0.5, 0.5])?
                    .inks(["red"])?,
            ],
        )
    };
    let rows: [(f64, [f32; 10]); 5] = [
        (0.0, [0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0]),
        (0.25, [0.0, 0.0, 0.75, 1.0, 1.0, 1.0, 1.0, 0.75, 0.0, 0.0]),
        (0.5, [0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0, 0.5, 0.0, 0.0]),
        (1.0, [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]),
        (1.5, [0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 0.5, 0.0, 0.0, 0.0]),
    ];
    for (choke, want) in rows {
        let plates = box_of_ink()?.plates_with_underbase(10, &Underbase::new(GARMENT)?.choke(choke)?)?;
        assert_eq!(plates.names(), ["white", "red"]);
        let white = plane(&plates, "white")?;
        for (x, want) in want.iter().enumerate() {
            assert!(
                (white[5 * 10 + x] - want).abs() < 1e-6,
                "choke {choke}, column {x}: {}",
                white[5 * 10 + x]
            );
        }
    }
    let plates = box_of_ink()?.plates_with_underbase(10, &Underbase::new(GARMENT)?)?;
    // the inks are the plates the scene makes, unchanged
    assert_eq!(plane(&plates, "red")?, plane(&box_of_ink()?.plates(10)?, "red")?);
    // no choke: the underbase is the union itself; a choke past the shape leaves none
    assert_eq!(plane(&plates, "white")?, plane(&plates, "red")?);
    let gone = box_of_ink()?.plates_with_underbase(10, &Underbase::new(GARMENT)?.choke(10.0)?)?;
    assert!(plane(&gone, "white")?.iter().all(|value| *value == 0.0));
    Ok(())
}

/// A 12 by 6 canvas of `GARMENT` with a red box at columns 1 to 5 and a box of `dark` at columns 7 to 11, in the inks `red`
/// and `dark`.
fn two_boxes(dark: &str) -> Result<Scene, Error> {
    scene(
        12,
        6,
        [
            Layer::fill(GARMENT)?,
            Layer::fill("#ff0000")?
                .fit([1.0, 1.0, 4.0, 4.0], Fit::Stretch, [0.5, 0.5])?
                .inks(["red"])?,
            Layer::fill(dark)?
                .fit([7.0, 1.0, 4.0, 4.0], Fit::Stretch, [0.5, 0.5])?
                .inks(["dark"])?,
        ],
    )
}

#[test]
fn the_underbase_is_knocked_out_where_the_colour_is_the_garments_within_the_tolerance() -> Outcome {
    // the dark box prints the garment's own colour: nothing under it. (#242424 is 4 levels off in each channel: a distance of
    // 4 * sqrt(3) / 255 = 0.0272 of the 1.732 the cube's diagonal is, so 0.0157 on the unit scale)
    let exact = two_boxes(GARMENT)?.plates_with_underbase(12, &Underbase::new(GARMENT)?)?;
    let white = plane(&exact, "white")?;
    for y in 0..6 {
        for x in 0..12 {
            let want = if (1..5).contains(&x) && (1..5).contains(&y) {
                1.0
            } else {
                0.0
            };
            assert_eq!(white[y * 12 + x], want, "exact: ({x}, {y})");
        }
    }
    let near = two_boxes("#242424")?;
    let covered = |tolerance: f64| -> Result<f32, Box<dyn std::error::Error>> {
        let plates = near.plates_with_underbase(12, &Underbase::new(GARMENT)?.tolerance(tolerance)?)?;
        Ok(plane(&plates, "white")?[2 * 12 + 9])
    };
    assert_eq!(
        covered(0.01)?,
        1.0,
        "a tolerance under the distance keeps the underbase"
    );
    assert_eq!(covered(0.02)?, 0.0, "a tolerance over the distance knocks it out");
    Ok(())
}

#[test]
fn the_feather_softens_the_edge_of_the_knockout_as_a_blur_of_that_many_pixels() -> Outcome {
    // a red box from column 1 to 6 and a box of the garment's colour from 6 to 11: the knockout is a step at column 6, blurred by
    // sigma 1 pixel: the pixel centre 0.5 before the step has the share the normal curve gives 0.5 sigma before its middle
    let scene = scene(
        12,
        4,
        [
            Layer::fill(GARMENT)?,
            Layer::fill("#ff0000")?
                .fit([1.0, 0.0, 5.0, 4.0], Fit::Stretch, [0.5, 0.5])?
                .inks(["red"])?,
            Layer::fill(GARMENT)?
                .fit([6.0, 0.0, 5.0, 4.0], Fit::Stretch, [0.5, 0.5])?
                .inks(["dark"])?,
        ],
    )?;
    let plates = scene.plates_with_underbase(12, &Underbase::new(GARMENT)?.feather(1.0)?)?;
    let white = plane(&plates, "white")?;
    let row = |x: usize| f64::from(white[2 * 12 + x]);
    for x in [3_usize, 4, 5, 6, 7, 8] {
        // the knockout is a step up at 6.0 and the pixel's centre is at x + 0.5: the blur has taken the share the normal curve has
        // below that distance from the step, and the underbase is what is left
        let want = 1.0 - normal_cdf(f64::from(u8::try_from(x)?) + 0.5 - 6.0);
        assert!((row(x) - want).abs() < 0.05, "column {x}: {} against {want}", row(x));
    }
    Ok(())
}

#[test]
fn underbase_options_that_are_not_allowed_are_refused_naming_the_argument() -> Outcome {
    let rows: Vec<(&str, Result<Underbase, Error>, &str)> = vec![
        ("a colour that is not one", Underbase::new("garment"), "garment"),
        ("a choke below 0", Underbase::new(GARMENT)?.choke(-1.0), "choke"),
        ("a choke that is NaN", Underbase::new(GARMENT)?.choke(f64::NAN), "choke"),
        ("a choke of 1e300", Underbase::new(GARMENT)?.choke(1e300), "choke"),
        (
            "a tolerance over 1",
            Underbase::new(GARMENT)?.tolerance(1.5),
            "tolerance",
        ),
        (
            "a tolerance that is NaN",
            Underbase::new(GARMENT)?.tolerance(f64::NAN),
            "tolerance",
        ),
        ("a feather below 0", Underbase::new(GARMENT)?.feather(-0.1), "feather"),
        (
            "a feather of infinity",
            Underbase::new(GARMENT)?.feather(f64::INFINITY),
            "feather",
        ),
        ("a name with a slash", Underbase::new(GARMENT)?.name("a/b"), "name"),
    ];
    for (what, made, named) in rows {
        let error = made.err();
        assert!(
            matches!(&error, Some(Error::Invalid { address, .. }) if address == named),
            "{what}: {error:?}"
        );
    }
    // the underbase's name is an ink's: the scene is refused, as is one that names no ink
    let error = red_box()?
        .plates_with_underbase(9, &Underbase::new(GARMENT)?.name("red")?)
        .err();
    assert!(
        matches!(&error, Some(Error::Invalid { address, .. }) if address == "name"),
        "{error:?}"
    );
    let error = scene(4, 4, [Layer::fill(GARMENT)?])?
        .plates_with_underbase(4, &Underbase::new(GARMENT)?)
        .err();
    assert!(
        matches!(&error, Some(Error::Invalid { address, .. }) if address == "inks"),
        "{error:?}"
    );
    Ok(())
}
