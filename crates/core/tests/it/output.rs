//! `Raster::write`: a picture as a file. The expected bytes are worked out by hand from the rounding image's writer documents
//! (`level * 255 + 0.5` cut to a whole number); the decoder is image's reader.

use std::path::Path;

use bullterrier_scene::{Error, Fit, Layer, Raster, WriteOptions};

use crate::common::{Outcome, made, physical, scene, varied};

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

/// The JFIF density of a JPEG, read by hand: the `APP0` segment after the start marker is `JFIF\0`, a version, the unit
/// (1 is dots per inch) and two 16-bit densities.
fn density(path: &Path) -> Result<(u8, u16, u16), std::io::Error> {
    let bytes = std::fs::read(path)?;
    assert_eq!(&bytes[..4], &[0xff, 0xd8, 0xff, 0xe0], "a JFIF start");
    assert_eq!(&bytes[6..11], b"JFIF\0");
    Ok((
        bytes[13],
        u16::from_be_bytes([bytes[14], bytes[15]]),
        u16::from_be_bytes([bytes[16], bytes[17]]),
    ))
}

#[test]
fn a_png_says_its_dpi_in_pixels_per_metre_and_without_one_says_nothing() -> Outcome {
    let folder = tempfile::tempdir()?;
    let picture = varied(6, 4)?;
    let plain = folder.path().join("plain.png");
    picture.write(&plain)?;
    assert_eq!(physical(&plain)?, None);

    let printed = folder.path().join("printed.png");
    picture.write_with(&printed, &WriteOptions::new().dpi(300.0)?)?;
    // 300 dots per inch over 0.0254 m an inch is 11811.02 pixels a metre
    assert_eq!(physical(&printed)?, Some((11811, 11811, 1)));
    // the pixels are the same
    assert_eq!(decoded(&printed)?, decoded(&plain)?);
    Ok(())
}

#[test]
fn a_jpeg_says_its_dpi_as_a_jfif_density() -> Outcome {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("printed.jpg");
    varied(16, 16)?.write_with(&path, &WriteOptions::new().dpi(300.0)?)?;
    assert_eq!(density(&path)?, (1, 300, 300));
    Ok(())
}

#[test]
fn a_streamed_png_says_the_same_dpi_as_a_written_one() -> Outcome {
    let folder = tempfile::tempdir()?;
    let drawn = scene(8, 6, [Layer::fill("#3a7bd5")?])?;
    let options = WriteOptions::new().dpi(254.0)?;
    let streamed = folder.path().join("streamed.png");
    drawn.render_to_png_with(48, &streamed, &options)?;
    let written = folder.path().join("written.png");
    drawn.render(48)?.write_with(&written, &options)?;
    // 254 dots per inch is exactly 10000 pixels a metre
    assert_eq!(physical(&streamed)?, Some((10000, 10000, 1)));
    assert_eq!(physical(&streamed)?, physical(&written)?);
    let bare = folder.path().join("bare.png");
    drawn.render_to_png(48, &bare)?;
    assert_eq!(physical(&bare)?, None);
    Ok(())
}

#[test]
fn a_dpi_that_is_not_a_number_from_1_to_65535_is_refused_naming_dpi() {
    for value in [
        0.0,
        -300.0,
        0.99,
        65536.0,
        1e300,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        let error = WriteOptions::new().dpi(value).err();
        assert!(
            matches!(&error, Some(Error::Invalid { address, .. }) if address == "dpi"),
            "{value}: {error:?}"
        );
    }
    assert!(WriteOptions::new().dpi(1.0).is_ok() && WriteOptions::new().dpi(65535.0).is_ok());
}

#[test]
fn a_webp_holds_no_resolution_and_a_dpi_for_it_is_refused_with_no_file() -> Outcome {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("printed.webp");
    let error = varied(4, 4)?.write_with(&path, &WriteOptions::new().dpi(300.0)?).err();
    assert!(matches!(&error, Some(Error::Write { .. })), "{error:?}");
    assert!(!path.exists());
    Ok(())
}
