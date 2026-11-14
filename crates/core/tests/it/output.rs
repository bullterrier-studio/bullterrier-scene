//! `Raster::write`: a picture as a file. The expected bytes are worked out by hand from the rounding image's writer documents
//! (`level * 255 + 0.5` cut to a whole number); the decoder is image's reader.

use std::path::Path;

use bullterrier_scene::{Error, Fit, Layer, Raster};

use crate::common::{Outcome, made, scene, varied};

/// `level` (a multiple of 1/255 or near it) as its 8-bit number.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a level is 0..1, so the number is 0..255"
)]
fn byte(level: f32) -> u8 {
    (level * 255.0).round() as u8
}

/// The 8-bit values the PNG at `path` holds (red, green, blue, alpha of each pixel) and its size, read by image's reader.
fn decoded(path: &Path) -> Result<(usize, usize, Vec<u8>), Box<dyn std::error::Error>> {
    let image = bullterrier_image::read(path)?;
    assert_eq!(image.channels(), 4, "alpha is kept");
    let (width, height) = (image.width(), image.height());
    let bytes = image.into_values().iter().map(|level| byte(*level)).collect();
    Ok((width, height, bytes))
}

/// The colour type and bit depth of a PNG, from its header chunk: (type, depth).
fn header(path: &Path) -> Result<(u8, u8), std::io::Error> {
    let bytes = std::fs::read(path)?;
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    Ok((bytes[25], bytes[24]))
}

#[test]
fn levels_are_rounded_to_the_nearest_and_alpha_stays_straight() -> Outcome {
    // 3 wide, 2 high: a swapped width and height would not read back
    #[rustfmt::skip]
    let pixels = vec![
        0.0, 0.0, 0.0, 0.0,           1.0, 1.0, 1.0, 1.0,                0.5, 0.5, 0.5, 0.5,
        1.0 / 255.0, 2.0 / 255.0, 254.0 / 255.0, 0.25,   0.499, 0.501, 0.2, 0.8,   1.0, 0.0, 0.5, 0.0,
    ];
    #[rustfmt::skip]
    let expected: [u8; 24] = [
        0, 0, 0, 0,        255, 255, 255, 255,       128, 128, 128, 128,
        1, 2, 254, 64,     127, 128, 51, 204,        255, 0, 128, 0,
    ];
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("levels.png");
    Raster::rgba(3, 2, pixels)?.write(&path)?;
    assert_eq!(header(&path)?, (6, 8), "8-bit RGBA");
    assert_eq!(decoded(&path)?, (3, 2, expected.to_vec()));
    Ok(())
}

#[test]
fn a_rendered_picture_reads_back_within_half_a_level() -> Outcome {
    let drawn = scene(
        40,
        30,
        [
            Layer::fill("#3a7bd5")?,
            Layer::fill("#f2994a")?
                .opacity(0.37)?
                .fit([5.0, 4.0, 20.0, 15.0], Fit::Stretch, [0.5, 0.5])?,
        ],
    )?
    .render(64)?;
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("drawn.png");
    drawn.write(&path)?;
    let (width, height, bytes) = decoded(&path)?;
    assert_eq!((width, height), (drawn.width(), drawn.height()));
    let worst = drawn
        .pixels()
        .iter()
        .zip(&bytes)
        .map(|(level, byte)| (level * 255.0 - f32::from(*byte)).abs())
        .fold(0.0, f32::max);
    assert!(worst <= 0.5 + 1e-3, "the largest error is {worst} levels");
    Ok(())
}

#[test]
fn a_one_pixel_picture_and_a_large_one_are_written() -> Outcome {
    let folder = tempfile::tempdir()?;
    let one = folder.path().join("one.png");
    Raster::rgba(1, 1, vec![0.0, 1.0, 0.0, 1.0])?.write(&one)?;
    assert_eq!(decoded(&one)?, (1, 1, vec![0, 255, 0, 255]));

    // every pixel differs, the levels whole multiples of 1/255: they come back as they went
    let large = varied(1600, 700)?;
    let path = folder.path().join("large.png");
    large.write(&path)?;
    let (width, height, bytes) = decoded(&path)?;
    assert_eq!((width, height, bytes.len()), (1600, 700, 1600 * 700 * 4));
    let first_difference = large
        .pixels()
        .iter()
        .zip(&bytes)
        .position(|(level, got)| byte(*level) != *got);
    assert_eq!(first_difference, None);
    Ok(())
}

#[test]
fn the_same_picture_gives_the_same_bytes() -> Outcome {
    let folder = tempfile::tempdir()?;
    let picture = made(9, 5, |x, y| {
        [
            f32::from(u8::try_from(x).unwrap_or(0)) / 8.0,
            f32::from(u8::try_from(y).unwrap_or(0)) / 4.0,
            0.5,
            1.0,
        ]
    })?;
    let (first, second) = (folder.path().join("a.png"), folder.path().join("b.png"));
    picture.write(&first)?;
    picture.write(&second)?;
    assert_eq!(std::fs::read(first)?, std::fs::read(second)?);
    Ok(())
}

#[test]
fn a_file_that_cannot_be_written_is_a_write_error_naming_it_and_no_file_is_made() -> Outcome {
    let folder = tempfile::tempdir()?;
    let translucent = Raster::rgba(1, 1, vec![0.0, 0.0, 0.0, 0.5])?;
    let rows = [
        ("an extension it does not write", "out.bmp"),
        ("no extension", "out"),
        ("a folder that is not there", "missing/out.png"),
        ("a JPEG, which cannot hold alpha", "out.jpg"),
    ];
    for (what, name) in rows {
        let path = folder.path().join(name);
        let error = translucent.write(&path).err();
        assert!(
            matches!(&error, Some(Error::Write { path: named, .. }) if *named == path.display().to_string()),
            "{what}: {error:?}"
        );
        assert!(!path.exists(), "{what}: a file was made");
    }
    Ok(())
}
