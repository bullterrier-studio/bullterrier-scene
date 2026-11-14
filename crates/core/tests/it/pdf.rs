//! `Raster::write_pdf`. The reader is lopdf, independent of the writer (pdf-writer); every expected number is worked out by
//! hand from the dpi (a point is 1/72 in, a millimetre 72/25.4 pt) and the rounding of a PNG (`level * 255 + 0.5`, cut).

use std::path::Path;

use bullterrier_scene::{Error, PdfOptions, Raster};
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId, Stream};

use crate::common::{Outcome, pixel, varied};

/// `level` as the 8-bit number a PNG would hold.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a level is 0..1, so the number is 0..255"
)]
fn byte(level: f32) -> u8 {
    (f64::from(level) * 255.0 + 0.5).floor() as u8
}

/// Points in a millimetre.
const POINTS_PER_MM: f64 = 72.0 / 25.4;

/// A PDF read back: the document and its one page.
struct Page {
    document: Document,
    id: ObjectId,
}

/// What an image of the page holds.
struct Image {
    width: usize,
    height: usize,
    /// The colour space: `DeviceRGB`, or `ICCBased` with the profile's channel count and bytes.
    space: String,
    colour: Vec<u8>,
    mask: Option<Vec<u8>>,
    profile: Option<(i64, Vec<u8>)>,
}

fn number(object: &Object) -> f64 {
    match object {
        Object::Integer(value) => f64::from(i32::try_from(*value).unwrap_or(0)),
        Object::Real(value) => f64::from(*value),
        other => unreachable!("not a number: {other:?}"),
    }
}

impl Page {
    fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let document = Document::load(path)?;
        let pages = document.get_pages();
        assert_eq!(pages.len(), 1, "one page");
        let id = *pages.values().next().ok_or("no page")?;
        Ok(Self { document, id })
    }

    /// A box of the page (`MediaBox`, `TrimBox`, `BleedBox`) in points: left, bottom, right, top.
    fn boxed(&self, key: &str) -> Result<[f64; 4], Box<dyn std::error::Error>> {
        let page = self.document.get_dictionary(self.id)?;
        let Object::Array(values) = self.document.dereference(page.get(key.as_bytes())?)?.1 else {
            return Err(format!("{key} is not an array").into());
        };
        let mut found = [0.0; 4];
        for (slot, value) in found.iter_mut().zip(values) {
            *slot = number(value);
        }
        Ok(found)
    }

    fn stream<'a>(&'a self, object: &'a Object) -> Result<&'a Stream, Box<dyn std::error::Error>> {
        match self.document.dereference(object)?.1 {
            Object::Stream(stream) => Ok(stream),
            other => Err(format!("not a stream: {other:?}").into()),
        }
    }

    /// The one image the page draws.
    fn image(&self) -> Result<Image, Box<dyn std::error::Error>> {
        let page = self.document.get_dictionary(self.id)?;
        let Object::Dictionary(resources) = self.document.dereference(page.get(b"Resources")?)?.1 else {
            return Err("no resources".into());
        };
        let Object::Dictionary(objects) = self.document.dereference(resources.get(b"XObject")?)?.1 else {
            return Err("no x-objects".into());
        };
        assert_eq!(objects.len(), 1, "one picture");
        let (_, reference) = objects.iter().next().ok_or("no picture")?;
        let stream = self.stream(reference)?;
        let side = |key: &[u8]| -> Result<usize, Box<dyn std::error::Error>> {
            Ok(usize::try_from(stream.dict.get(key)?.as_i64()?)?)
        };
        let (space, profile) = match self.document.dereference(stream.dict.get(b"ColorSpace")?)?.1 {
            Object::Name(name) => (String::from_utf8(name.clone())?, None),
            Object::Array(items) => {
                let Object::Name(name) = &items[0] else {
                    return Err("a colour space array starts with a name".into());
                };
                let profile = self.stream(&items[1])?;
                (
                    String::from_utf8(name.clone())?,
                    Some((profile.dict.get(b"N")?.as_i64()?, profile.content.clone())),
                )
            }
            other => return Err(format!("colour space {other:?}").into()),
        };
        assert_eq!(stream.dict.get(b"BitsPerComponent")?.as_i64()?, 8);
        let mask = match stream.dict.get(b"SMask") {
            Ok(reference) => {
                let mask = self.stream(reference)?;
                assert_eq!(mask.dict.get(b"BitsPerComponent")?.as_i64()?, 8);
                Some(mask.decompressed_content()?)
            }
            Err(_) => None,
        };
        Ok(Image {
            width: side(b"Width")?,
            height: side(b"Height")?,
            space,
            colour: stream.decompressed_content()?,
            mask,
            profile,
        })
    }

    /// The operations of the page's content stream.
    fn operations(&self) -> Result<Vec<Operation>, Box<dyn std::error::Error>> {
        Ok(Content::decode(&self.document.get_page_content(self.id))?.operations)
    }
}

/// A raster of `width` by `height` whose pixels differ, translucent where `alpha` says.
fn picture(width: usize, height: usize, alpha: impl Fn(usize, usize) -> f32) -> Result<Raster, Error> {
    let opaque = varied(width, height)?;
    let mut values = opaque.into_pixels();
    for (index, pixel) in values.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        pixel[3] = alpha(index % width, index / width);
    }
    Raster::rgba(width, height, values)
}

fn close(got: [f64; 4], expected: [f64; 4]) -> bool {
    got.iter()
        .zip(expected)
        .all(|(got, expected)| (got - expected).abs() < 2e-3)
}

#[test]
fn the_page_is_the_picture_at_its_physical_size_and_the_image_is_the_picture() -> Outcome {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("page.pdf");
    // 3600 pixels at 300 dpi are 12 in = 864 pt
    let wide = picture(3600, 2, |_, _| 1.0)?;
    wide.write_pdf(&path, &PdfOptions::new(300.0)?)?;
    let page = Page::open(&path)?;
    let expected = [0.0, 0.0, 864.0, 0.48];
    for key in ["MediaBox", "TrimBox", "BleedBox"] {
        assert!(close(page.boxed(key)?, expected), "{key}: {:?}", page.boxed(key)?);
    }
    let image = page.image()?;
    assert_eq!(
        (image.width, image.height, image.space.as_str()),
        (3600, 2, "DeviceRGB")
    );
    assert!(image.mask.is_none(), "an opaque picture has no soft mask");
    let expected: Vec<u8> = wide
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| [byte(pixel[0]), byte(pixel[1]), byte(pixel[2])])
        .collect();
    assert_eq!(image.colour.len(), expected.len());
    assert_eq!(
        image.colour.iter().zip(&expected).position(|(got, want)| got != want),
        None
    );
    Ok(())
}

#[test]
fn a_bleed_repeats_the_edge_pixels_in_whole_pixels_and_grows_the_bleed_box_by_the_millimetres() -> Outcome {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("bleed.pdf");
    let picture = varied(40, 30)?;
    picture.write_pdf(&path, &PdfOptions::new(300.0)?.bleed_mm(3.0)?)?;
    let page = Page::open(&path)?;
    // 3 mm at 300 dpi is 35.43 pixels: 36 whole pixels a side; 3 mm is 8.504 pt
    let (bleed, trim_width, trim_height) = (3.0 * POINTS_PER_MM, 40.0 * 72.0 / 300.0, 30.0 * 72.0 / 300.0);
    assert!(close(
        page.boxed("TrimBox")?,
        [bleed, bleed, bleed + trim_width, bleed + trim_height]
    ));
    let around = [0.0, 0.0, trim_width + 2.0 * bleed, trim_height + 2.0 * bleed];
    assert!(close(page.boxed("BleedBox")?, around));
    assert!(
        close(page.boxed("MediaBox")?, around),
        "without crop marks the page is the bleed box"
    );
    // the picture is clipped to the bleed box (it is a pixel wider than the 3 mm), so none lands in a crop-mark margin
    let operations = page.operations()?;
    let clip = operations
        .iter()
        .position(|operation| operation.operator == "re")
        .ok_or("no clip rectangle")?;
    assert_eq!(operations[clip + 1].operator, "W", "the rectangle is a clip");
    let drawn: Vec<f64> = operations[clip].operands.iter().map(number).collect();
    assert!(
        close([drawn[0], drawn[1], drawn[0] + drawn[2], drawn[1] + drawn[3]], around),
        "the clip is the bleed box: {drawn:?}"
    );
    let image = page.image()?;
    assert_eq!((image.width, image.height), (40 + 72, 30 + 72));
    for y in 0..image.height {
        for x in 0..image.width {
            let (from_x, from_y) = (x.saturating_sub(36).min(39), y.saturating_sub(36).min(29));
            let want = pixel(&picture, from_x, from_y).map(byte);
            let at = (y * image.width + x) * 3;
            assert_eq!(
                image.colour[at..at + 3],
                want[..3],
                "pixel ({x}, {y}) repeats ({from_x}, {from_y})"
            );
        }
    }
    Ok(())
}

/// The corner points (`m`, `l`) of the content stream's paths, in pairs: each stroked line is `(from, to)`.
fn lines(operations: &[Operation]) -> Vec<([f64; 2], [f64; 2])> {
    let mut found = Vec::new();
    let mut from = [0.0, 0.0];
    for operation in operations {
        let at = || [number(&operation.operands[0]), number(&operation.operands[1])];
        match operation.operator.as_str() {
            "m" => from = at(),
            "l" => found.push((from, at())),
            _ => {}
        }
    }
    found
}

#[test]
fn crop_marks_are_drawn_only_when_asked_and_lie_outside_the_bleed_box() -> Outcome {
    let folder = tempfile::tempdir()?;
    let picture = varied(40, 30)?;
    let plain = folder.path().join("plain.pdf");
    picture.write_pdf(&plain, &PdfOptions::new(300.0)?.bleed_mm(3.0)?)?;
    assert_eq!(lines(&Page::open(&plain)?.operations()?), []);

    let marked = folder.path().join("marked.pdf");
    picture.write_pdf(&marked, &PdfOptions::new(300.0)?.bleed_mm(3.0)?.crop_marks(true))?;
    let page = Page::open(&marked)?;
    let (trim, bleed_box, media) = (page.boxed("TrimBox")?, page.boxed("BleedBox")?, page.boxed("MediaBox")?);
    // the bleed box starts 8 mm inside the page: 3 mm of gap and 5 mm of mark
    let margin = 8.0 * POINTS_PER_MM;
    assert!(close(bleed_box, [margin, margin, media[2] - margin, media[3] - margin]));
    assert!(close(
        media,
        [
            0.0,
            0.0,
            9.6 + 2.0 * (3.0 + 8.0) * POINTS_PER_MM,
            7.2 + 2.0 * (3.0 + 8.0) * POINTS_PER_MM
        ]
    ));
    let marks = lines(&page.operations()?);
    assert_eq!(
        marks.len(),
        8,
        "a horizontal and a vertical line at each of four corners"
    );
    for (from, to) in &marks {
        let horizontal = (from[1] - to[1]).abs() < 1e-3;
        // the line runs along a trim edge, from 3 mm to 8 mm outside the bleed box
        let (along, across) = if horizontal { (0, 1) } else { (1, 0) };
        let edges = if horizontal {
            [trim[1], trim[3]]
        } else {
            [trim[0], trim[2]]
        };
        assert!(
            edges.iter().any(|edge| (edge - from[across]).abs() < 1e-3),
            "{from:?} lies on a trim edge"
        );
        let (low, high) = (bleed_box[along], bleed_box[along + 2]);
        for end in [from[along], to[along]] {
            assert!(
                end < low - 3.0 * POINTS_PER_MM + 1e-3 || end > high + 3.0 * POINTS_PER_MM - 1e-3,
                "{end} is clear of the bleed"
            );
        }
        assert!(
            ((from[along] - to[along]).abs() - 5.0 * POINTS_PER_MM).abs() < 1e-2,
            "5 mm long"
        );
    }
    // one at each corner: each of the four x and y trim positions is used twice
    for edge in [trim[0], trim[2]] {
        assert_eq!(
            marks
                .iter()
                .filter(|(from, to)| (from[0] - to[0]).abs() < 1e-3 && (from[0] - edge).abs() < 1e-3)
                .count(),
            2
        );
    }
    Ok(())
}

#[test]
fn alpha_is_a_soft_mask_only_where_the_picture_has_some_and_flattening_puts_it_over_white() -> Outcome {
    let folder = tempfile::tempdir()?;
    // the first pixels of the row: (colour, alpha) = (0.0, 0.5), (0.2, 0.5), (1.0, 0.25), (0.4, 0.0); worked by hand:
    // over white 0.0 * 0.5 + 0.5 = 0.5 -> 128, 0.2 * 0.5 + 0.5 = 0.6 -> 153, 1.0, 0.4 * 0 + 1 = 1 -> 255
    let raster = Raster::rgba(
        4,
        1,
        vec![
            0.0, 0.0, 0.0, 0.5, //
            0.2, 0.2, 0.2, 0.5, //
            1.0, 1.0, 1.0, 0.25, //
            0.4, 0.4, 0.4, 0.0,
        ],
    )?;
    let kept = folder.path().join("kept.pdf");
    raster.write_pdf(&kept, &PdfOptions::new(72.0)?)?;
    let image = Page::open(&kept)?.image()?;
    assert_eq!(image.mask, Some(vec![128, 128, 64, 0]));
    assert_eq!(
        image.colour,
        [0, 0, 0, 51, 51, 51, 255, 255, 255, 102, 102, 102],
        "the colour stays straight"
    );

    let flat = folder.path().join("flat.pdf");
    raster.write_pdf(&flat, &PdfOptions::new(72.0)?.flatten(true))?;
    let image = Page::open(&flat)?.image()?;
    assert_eq!(image.mask, None);
    assert_eq!(
        image.colour,
        [128, 128, 128, 153, 153, 153, 255, 255, 255, 255, 255, 255]
    );

    // a bleed repeats the alpha too
    let bled = folder.path().join("bled.pdf");
    picture(5, 1, |x, _| if x == 0 { 0.5 } else { 1.0 })?.write_pdf(&bled, &PdfOptions::new(254.0)?.bleed_mm(0.2)?)?;
    let image = Page::open(&bled)?.image()?;
    // 0.2 mm at 254 dpi is 2 pixels (not 3, though doubles make it 2.0000000000000004): 9 columns and 5 rows, each row two
    // bleed pixels and the first column's 128, then 255
    assert_eq!(image.mask, Some([vec![128; 3], vec![255; 6]].concat().repeat(5)));
    Ok(())
}

#[test]
fn options_out_of_range_are_refused_naming_the_argument() {
    let rows: [(&str, Result<PdfOptions, Error>, &str); 8] = [
        ("dpi 0", PdfOptions::new(0.0), "dpi"),
        ("dpi below the range", PdfOptions::new(0.5), "dpi"),
        ("dpi above the range", PdfOptions::new(2400.5), "dpi"),
        ("dpi NaN", PdfOptions::new(f64::NAN), "dpi"),
        ("dpi infinite", PdfOptions::new(f64::INFINITY), "dpi"),
        (
            "a negative bleed",
            PdfOptions::new(300.0).and_then(|options| options.bleed_mm(-0.1)),
            "bleed_mm",
        ),
        (
            "a bleed of 51 mm",
            PdfOptions::new(300.0).and_then(|options| options.bleed_mm(51.0)),
            "bleed_mm",
        ),
        (
            "a bleed that is NaN",
            PdfOptions::new(300.0).and_then(|options| options.bleed_mm(f64::NAN)),
            "bleed_mm",
        ),
    ];
    for (what, result, field) in rows {
        assert!(
            matches!(&result, Err(Error::Invalid { address, .. }) if address == field),
            "{what}: {result:?}"
        );
    }
    assert!(
        PdfOptions::new(1.0).is_ok() && PdfOptions::new(2400.0).is_ok(),
        "the ends are in"
    );
}

#[test]
fn a_page_or_image_too_large_a_file_that_cannot_be_written_and_a_bad_profile_are_refused_and_no_file_is_made() -> Outcome
{
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("never.pdf");
    let rows = [
        // 20,000 pixels at 1 dpi are 1,440,000 pt, past the 14,400 of a PDF reader
        (
            "a page past 14,400 pt",
            Raster::rgba(20_000, 1, vec![0.5; 80_000])?,
            PdfOptions::new(1.0)?,
            "dpi",
        ),
        // 8,000 x 1,000 pixels with 4,725 more on every side are 17,450 x 10,450 > 2^27
        (
            "an image past 2^27 pixels",
            Raster::rgba(8000, 1000, vec![0.5; 32_000_000])?,
            PdfOptions::new(2400.0)?.bleed_mm(50.0)?,
            "bleed_mm",
        ),
    ];
    for (what, raster, options, field) in rows {
        let error = raster.write_pdf(&path, &options).err();
        assert!(
            matches!(&error, Some(Error::Invalid { address, .. }) if address == field),
            "{what}: {error:?}"
        );
        assert!(!path.exists(), "{what}: no file is made");
    }
    let one = Raster::rgba(1, 1, vec![0.5; 4])?;
    let missing = folder.path().join("missing").join("out.pdf");
    let error = one.write_pdf(&missing, &PdfOptions::new(300.0)?).err();
    assert!(
        matches!(&error, Some(Error::Write { path: named, .. }) if *named == missing.display().to_string()),
        "{error:?}"
    );

    let not_icc = folder.path().join("not.icc");
    std::fs::write(&not_icc, b"this is not a profile")?;
    let error = one
        .write_pdf(&path, &PdfOptions::new(300.0)?.cmyk_profile(&not_icc))
        .err();
    assert!(
        matches!(&error, Some(Error::Invalid { address, .. }) if address == "cmyk_profile"),
        "{error:?}"
    );
    let error = one
        .write_pdf(
            &path,
            &PdfOptions::new(300.0)?.cmyk_profile(folder.path().join("none.icc")),
        )
        .err();
    assert!(matches!(&error, Some(Error::Read { .. })), "{error:?}");
    assert!(!path.exists());
    Ok(())
}

#[test]
fn a_one_pixel_picture_and_a_large_one_are_written_and_the_same_picture_gives_the_same_bytes() -> Outcome {
    let folder = tempfile::tempdir()?;
    let one = folder.path().join("one.pdf");
    Raster::rgba(1, 1, vec![0.0, 1.0, 0.0, 1.0])?
        .write_pdf(&one, &PdfOptions::new(300.0)?.bleed_mm(1.0)?.crop_marks(true))?;
    let image = Page::open(&one)?.image()?;
    // 1 mm at 300 dpi is 11.8 pixels: 12 a side
    assert_eq!((image.width, image.height), (25, 25));
    assert!(
        image.colour.as_chunks::<3>().0.iter().all(|rgb| *rgb == [0, 255, 0]),
        "a bleed of one colour is that colour"
    );

    let large = varied(1600, 700)?;
    let (first, second) = (folder.path().join("a.pdf"), folder.path().join("b.pdf"));
    let options = PdfOptions::new(300.0)?.bleed_mm(3.0)?;
    large.write_pdf(&first, &options)?;
    large.write_pdf(&second, &options)?;
    assert!(std::fs::read(&first)? == std::fs::read(&second)?, "the same bytes");
    let page = Page::open(&first)?;
    assert_eq!((page.image()?.width, page.image()?.height), (1600 + 72, 700 + 72));
    Ok(())
}

#[test]
fn the_file_is_a_whole_pdf_every_object_resolves_and_it_has_no_date_or_id() -> Outcome {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("whole.pdf");
    picture(8, 6, |x, _| if x == 0 { 0.5 } else { 1.0 })?.write_pdf(&path, &PdfOptions::new(96.0)?.crop_marks(true))?;
    let document = Document::load(&path)?;
    assert!(!document.objects.is_empty());
    for id in document.objects.keys() {
        assert!(document.get_object(*id).is_ok(), "object {id:?} resolves");
    }
    assert!(document.trailer.get(b"Info").is_err() && document.trailer.get(b"ID").is_err());
    let bytes = std::fs::read(&path)?;
    assert!(bytes.starts_with(b"%PDF-1."));
    assert!(bytes.windows(5).any(|window| window == b"%%EOF"));
    Ok(())
}

/// A CMYK output profile (ICC v2, Lab connection space) made by hand, so that no third-party profile is kept: its table from
/// Lab to CMYK is affine in the encoded coordinates, so interpolation reproduces it exactly. With `x` the encoded lightness
/// from 0 (black) to 1 (the white of the encoding): cyan 0, magenta `(1 - x) / 2`, yellow 0, black `1 - x`; it ignores a and b.
fn cmyk_profile() -> Vec<u8> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a value of 0 to 1 times 65536 is a small whole number"
    )]
    fn fixed(value: f64) -> [u8; 4] {
        // s15Fixed16: the value times 65536, a whole number
        ((value * 65536.0).round() as i32).to_be_bytes()
    }
    fn tag(signature: [u8; 4], body: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut padded = body.to_vec();
        padded.resize(body.len().next_multiple_of(4), 0);
        (signature.to_vec(), padded)
    }
    // lut16Type, 3 inputs (L, a, b), 4 outputs, a grid of 2 points a side; identity matrix, input and output curves
    let mut lut = b"mft2\0\0\0\0".to_vec();
    lut.extend([3, 4, 2, 0]);
    for value in [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        lut.extend(fixed(value));
    }
    lut.extend(2_u16.to_be_bytes());
    lut.extend(2_u16.to_be_bytes());
    for _ in 0..3 {
        lut.extend([0, 0, 0xFF, 0xFF]);
    }
    for lightness in [0_u16, 0xFFFF] {
        for _ in 0..4 {
            // the grid runs L, then a, then b, the last fastest: four corners share each lightness
            let black = 0xFFFF - lightness;
            for channel in [0, black / 2, 0, black] {
                lut.extend(u16::to_be_bytes(channel));
            }
        }
    }
    for _ in 0..4 {
        lut.extend([0, 0, 0xFF, 0xFF]);
    }
    let mut white_point = b"XYZ \0\0\0\0".to_vec();
    for value in [0.9642, 1.0, 0.8249] {
        white_point.extend(fixed(value));
    }
    let mut description = b"desc\0\0\0\0".to_vec();
    description.extend(5_u32.to_be_bytes());
    description.extend(b"test\0");
    description.extend([0; 4 + 4 + 2 + 1 + 67]);
    let mut copyright = b"text\0\0\0\0".to_vec();
    copyright.extend(b"none\0");
    let tags = [
        tag(*b"desc", &description),
        tag(*b"wtpt", &white_point),
        tag(*b"cprt", &copyright),
        tag(*b"B2A0", &lut),
    ];
    let table_end = 128 + 4 + 12 * tags.len();
    let mut table = u32::try_from(tags.len()).unwrap_or(0).to_be_bytes().to_vec();
    let mut body: Vec<u8> = Vec::new();
    for (signature, data) in &tags {
        table.extend(signature);
        table.extend(u32::try_from(table_end + body.len()).unwrap_or(0).to_be_bytes());
        table.extend(u32::try_from(data.len()).unwrap_or(0).to_be_bytes());
        body.extend(data);
    }
    let mut header = vec![0_u8; 128];
    header[8..12].copy_from_slice(&[2, 0x40, 0, 0]);
    header[12..24].copy_from_slice(b"prtrCMYKLab ");
    header[36..40].copy_from_slice(b"acsp");
    for (at, value) in [0.9642, 1.0, 0.8249].into_iter().enumerate() {
        header[68 + 4 * at..72 + 4 * at].copy_from_slice(&fixed(value));
    }
    let mut profile = header;
    profile.extend(table);
    profile.extend(body);
    let size = u32::try_from(profile.len()).unwrap_or(0).to_be_bytes();
    profile[..4].copy_from_slice(&size);
    profile
}

#[test]
fn a_cmyk_profile_gives_a_four_channel_image_in_that_profile_with_the_profile_embedded() -> Outcome {
    let folder = tempfile::tempdir()?;
    let profile_path = folder.path().join("test.icc");
    let profile = cmyk_profile();
    std::fs::write(&profile_path, &profile)?;
    // white, black and pure red, as sRGB
    let raster = Raster::rgba(3, 1, vec![1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0])?;
    let path = folder.path().join("cmyk.pdf");
    raster.write_pdf(&path, &PdfOptions::new(300.0)?.cmyk_profile(&profile_path))?;
    let image = Page::open(&path)?.image()?;
    assert_eq!(image.space, "ICCBased");
    assert_eq!(
        image.profile,
        Some((4, profile)),
        "the caller's bytes, with four channels"
    );
    assert_eq!(image.colour.len(), 3 * 4, "four bytes a pixel");
    // lightness of each colour, from the sRGB primaries' D50 luminance (Y of white 1, red 0.2225): L* = 116 cbrt(Y) - 16;
    // the table's black is 1 - L/100 of the encoding's white (0xFF00 of 0xFFFF)
    let black = |lightness: f64| 255.0 * (1.0 - lightness / 100.0 * 65280.0 / 65535.0);
    let red = 116.0 * 0.2225_f64.cbrt() - 16.0;
    for (pixel, lightness) in [(0, 100.0), (1, 0.0), (2, red)] {
        let got = &image.colour[pixel * 4..pixel * 4 + 4];
        let want = black(lightness);
        assert_eq!(got[0], 0, "cyan of pixel {pixel}");
        assert_eq!(got[2], 0, "yellow of pixel {pixel}");
        assert!(
            (f64::from(got[3]) - want).abs() <= 2.0,
            "black of pixel {pixel} is {} for {want}",
            got[3]
        );
        assert!(
            (f64::from(got[1]) - want / 2.0).abs() <= 2.0,
            "magenta of pixel {pixel} is {}",
            got[1]
        );
    }
    Ok(())
}
