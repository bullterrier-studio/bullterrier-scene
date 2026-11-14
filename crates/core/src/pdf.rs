//! `Raster::write_pdf`: a picture as a one-page PDF at a physical size. The adapter of the PDF writer (`pdf-writer`), the
//! Flate encoder (`miniz_oxide`) and the colour transform to a caller's CMYK profile (`moxcms`).
//!
//! Layout, in PDF points (1/72 in) from the page's lower left corner: the page (`MediaBox`) is the bleed box plus, with crop
//! marks, a margin; the bleed box (`BleedBox`) is the trim box grown by the bleed on every side; the trim box (`TrimBox`) is
//! the picture at its physical size. The picture is drawn with its bleed (the edge pixels repeated outward, whole pixels) and
//! clipped to the bleed box.

use std::path::{Path, PathBuf};

use moxcms::{ColorProfile, DataColorSpace, Layout, TransformOptions};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};

use crate::geometry::MAX_PIXELS;
use crate::ranges::{BLEED_MM, DPI};
use crate::{Error, Raster};

/// Millimetres in an inch (exact, by definition).
const MM_PER_INCH: f64 = 25.4;
/// Points in an inch (the PDF unit, exact by definition).
const POINTS_PER_INCH: f64 = 72.0;
/// The largest page a PDF reader takes: 14,400 units, 200 in (PDF 1.7, Annex C, implementation limits).
const MAX_PAGE_POINTS: f64 = 14_400.0;
/// Crop marks start this far outside the bleed box, so no mark touches the bleed (common print practice: Adobe's default offset
/// is 3 mm).
const MARK_GAP_MM: f64 = 3.0;
/// Each crop mark is this long.
const MARK_LENGTH_MM: f64 = 5.0;
/// The line width of a crop mark, in points (Adobe's default).
const MARK_WIDTH_POINTS: f64 = 0.25;

/// What `Raster::write_pdf` is asked for: the resolution that fixes the physical size, a bleed, crop marks, whether to
/// flatten transparency, and a CMYK profile. Built by `new` and the setters, each checked where it is given.
///
/// ```text
/// PdfOptions::new(300.0)?.bleed_mm(3.0)?.crop_marks(true)
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct PdfOptions {
    dpi: f64,
    bleed_mm: f64,
    crop_marks: bool,
    flatten: bool,
    cmyk_profile: Option<PathBuf>,
}

impl PdfOptions {
    /// Options for a picture printed at `dpi` pixels per inch: a 3600-pixel side at 300 is 12 in. No bleed, no crop marks,
    /// transparency kept (an image mask), RGB.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `dpi` unless it is a number from 1 to 2400.
    pub fn new(dpi: f64) -> Result<Self, Error> {
        Ok(Self {
            dpi: DPI.check("dpi", dpi)?,
            bleed_mm: 0.0,
            crop_marks: false,
            flatten: false,
            cmyk_profile: None,
        })
    }

    /// Extends the page by `mm` millimetres on every side with the picture's own edge, repeated outward: the bleed. It is
    /// rounded up to whole pixels of the picture (to a millionth of a pixel); the page's `BleedBox` is exactly `mm`. A scene's content stops at its canvas,
    /// so there is nothing beyond the edge to show: the bleed is the edge pixels stretched, which suits a background that runs
    /// off the page and not a detail at the edge.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `bleed_mm` unless it is a number from 0 to 50.
    pub fn bleed_mm(mut self, mm: f64) -> Result<Self, Error> {
        self.bleed_mm = BLEED_MM.check("bleed_mm", mm)?;
        Ok(self)
    }

    /// Draws crop marks (thin black lines, 0.25 pt wide and 5 mm long, as vector paths) at the four corners of the trim box,
    /// starting 3 mm outside the bleed box: the page grows by 8 mm on every side to hold them. Off by default.
    #[must_use]
    pub fn crop_marks(mut self, on: bool) -> Self {
        self.crop_marks = on;
        self
    }

    /// Puts the picture over white paper (straight alpha, in sRGB values, in floats) instead of writing its alpha as a soft
    /// mask. Off by default: a picture with a pixel that is not opaque keeps its alpha.
    #[must_use]
    pub fn flatten(mut self, on: bool) -> Self {
        self.flatten = on;
        self
    }

    /// Writes the image in the CMYK colour of the ICC profile file `path` (a CMYK output profile the caller holds; none is
    /// bundled) and embeds the profile, as an `ICCBased` colour space. The colours go from sRGB through the profile's table
    /// for the perceptual intent; the file is read when the PDF is written.
    #[must_use]
    pub fn cmyk_profile(mut self, path: impl Into<PathBuf>) -> Self {
        self.cmyk_profile = Some(path.into());
        self
    }
}

/// The picture, one byte a channel, in the colour space of the PDF.
struct Planes {
    /// The colour channels (3 for RGB, 4 for CMYK) of each pixel, the bleed included.
    colour: Vec<u8>,
    /// The alpha of each pixel, when some pixel is not opaque and the caller did not flatten.
    alpha: Option<Vec<u8>>,
}

/// `level` (0 to 1) as an 8-bit number, rounded as image's writer rounds a PNG (`level * 255 + 0.5` cut to a whole number).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a level is 0..1, so the number is 0..255"
)]
fn byte(level: f32) -> u8 {
    (level * 255.0 + 0.5) as u8
}

/// `value` in points, as the writer takes it.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a page of at most 14,400 pt fits a float"
)]
fn points(value: f64) -> f32 {
    value as f32
}

/// The planes of `raster` with `bleed` pixels of its edge repeated on every side, in `profile`'s colour if there is one.
fn planes(raster: &Raster, bleed: usize, flatten: bool, profile: Option<&ColorProfile>) -> Result<Planes, Error> {
    let (width, height) = (raster.width(), raster.height());
    let (across, down) = (width + 2 * bleed, height + 2 * bleed);
    let with_alpha = !flatten && raster.pixels().as_chunks::<4>().0.iter().any(|pixel| pixel[3] < 1.0);
    let transform = profile
        .map(|cmyk| {
            ColorProfile::new_srgb().create_transform_8bit(Layout::Rgb, cmyk, Layout::Rgba, TransformOptions::default())
        })
        .transpose()
        .map_err(|error| {
            Error::invalid(
                "cmyk_profile",
                format!("no conversion from sRGB to the profile: {error}"),
            )
        })?;
    let channels = if transform.is_some() { 4 } else { 3 };
    let mut colour: Vec<u8> = Vec::with_capacity(across * down * channels);
    let mut alpha: Vec<u8> = Vec::with_capacity(if with_alpha { across * down } else { 0 });
    let (mut rgb, mut device) = (vec![0_u8; width * 3], vec![0_u8; width * channels]);
    let mut previous = None;
    for row in 0..down {
        let source = row.saturating_sub(bleed).min(height - 1);
        if previous == Some(source) {
            // the rows of a bleed above and below repeat the edge row: copy what was written
            let start = (row - 1) * across * channels;
            colour.extend_from_within(start..start + across * channels);
            if with_alpha {
                let start = (row - 1) * across;
                alpha.extend_from_within(start..start + across);
            }
            continue;
        }
        previous = Some(source);
        let line = &raster.pixels()[source * width * 4..(source + 1) * width * 4];
        let mut opacity = Vec::with_capacity(width);
        for (pixel, out) in line
            .as_chunks::<4>()
            .0
            .iter()
            .zip(rgb.as_chunks_mut::<3>().0.iter_mut())
        {
            let covered = pixel[3];
            for (level, slot) in pixel[..3].iter().zip(out) {
                *slot = byte(if flatten {
                    level * covered + (1.0 - covered)
                } else {
                    *level
                });
            }
            opacity.push(byte(covered));
        }
        match &transform {
            Some(convert) => {
                convert
                    .transform(&rgb, &mut device)
                    .map_err(|error| Error::invalid("cmyk_profile", format!("the conversion failed: {error}")))?;
            }
            None => device.copy_from_slice(&rgb),
        }
        colour.extend(device[..channels].repeat(bleed));
        colour.extend_from_slice(&device);
        colour.extend(device[(width - 1) * channels..].repeat(bleed));
        if with_alpha {
            alpha.extend(std::iter::repeat_n(opacity[0], bleed));
            alpha.extend_from_slice(&opacity);
            alpha.extend(std::iter::repeat_n(opacity[width - 1], bleed));
        }
    }
    Ok(Planes {
        colour,
        alpha: with_alpha.then_some(alpha),
    })
}

/// The crop marks: a horizontal and a vertical line at each corner of `trim`, outside `bleed_box`. Both are `[left, bottom,
/// right, top]`.
fn crop_marks(content: &mut Content, trim: [f64; 4], bleed_box: [f64; 4]) {
    let gap = MARK_GAP_MM / MM_PER_INCH * POINTS_PER_INCH;
    let length = MARK_LENGTH_MM / MM_PER_INCH * POINTS_PER_INCH;
    content.set_stroke_gray(0.0).set_line_width(points(MARK_WIDTH_POINTS));
    for (edge, outward) in [(bleed_box[0], -1.0), (bleed_box[2], 1.0)] {
        for y in [trim[1], trim[3]] {
            let (near, far) = (edge + outward * gap, edge + outward * (gap + length));
            content
                .move_to(points(near), points(y))
                .line_to(points(far), points(y))
                .stroke();
        }
    }
    for (edge, outward) in [(bleed_box[1], -1.0), (bleed_box[3], 1.0)] {
        for x in [trim[0], trim[2]] {
            let (near, far) = (edge + outward * gap, edge + outward * (gap + length));
            content
                .move_to(points(x), points(near))
                .line_to(points(x), points(far))
                .stroke();
        }
    }
}

/// Where everything lies, in whole pixels and PDF points; each box is `[left, bottom, right, top]` from the page's lower left.
struct Geometry {
    /// Pixels of edge repeated on each side: the bleed rounded up.
    bleed_pixels: usize,
    /// The size of the image with its bleed, in pixels.
    image: (usize, usize),
    /// Points a pixel covers.
    per_pixel: f64,
    page: [f64; 4],
    bleed_box: [f64; 4],
    trim: [f64; 4],
}

/// `count` as a double: a side is under 2^27 pixels once the image is checked, which a double holds exactly.
#[expect(
    clippy::cast_precision_loss,
    reason = "a side of an image under 2^27 pixels is exact in a double"
)]
fn float(count: usize) -> f64 {
    count as f64
}

/// The layout of `raster` under `options`.
///
/// # Errors
/// As [`Raster::write_pdf`] for a size.
fn geometry(raster: &Raster, options: &PdfOptions) -> Result<Geometry, Error> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a bleed of at most 50 mm at 2400 dpi is under 4,725 pixels"
    )]
    // rounded to a millionth of a pixel first, so that 0.2 mm at 254 dpi, which is 2.0000000000000004 pixels in doubles, is 2
    let bleed_pixels = ((options.bleed_mm / MM_PER_INCH * options.dpi * 1e6).round() / 1e6).ceil() as usize;
    let image = (raster.width() + 2 * bleed_pixels, raster.height() + 2 * bleed_pixels);
    if image.0.saturating_mul(image.1) > MAX_PIXELS {
        return Err(Error::invalid(
            if bleed_pixels == 0 { "raster" } else { "bleed_mm" },
            format!(
                "the image of {} x {} pixels has more than {MAX_PIXELS} pixels (2^27)",
                image.0, image.1
            ),
        ));
    }
    let per_pixel = POINTS_PER_INCH / options.dpi;
    let trim_size = (float(raster.width()) * per_pixel, float(raster.height()) * per_pixel);
    let bleed = options.bleed_mm / MM_PER_INCH * POINTS_PER_INCH;
    let margin = if options.crop_marks {
        (MARK_GAP_MM + MARK_LENGTH_MM) / MM_PER_INCH * POINTS_PER_INCH
    } else {
        0.0
    };
    let page = [
        0.0,
        0.0,
        trim_size.0 + 2.0 * (bleed + margin),
        trim_size.1 + 2.0 * (bleed + margin),
    ];
    if page[2].max(page[3]) > MAX_PAGE_POINTS {
        return Err(Error::invalid(
            "dpi",
            format!(
                "the page would be {:.0} x {:.0} pt, and a PDF reader takes at most {MAX_PAGE_POINTS} pt (200 in); a higher dpi makes it smaller",
                page[2], page[3]
            ),
        ));
    }
    Ok(Geometry {
        bleed_pixels,
        image,
        per_pixel,
        page,
        bleed_box: [margin, margin, page[2] - margin, page[3] - margin],
        trim: [
            margin + bleed,
            margin + bleed,
            margin + bleed + trim_size.0,
            margin + bleed + trim_size.1,
        ],
    })
}

/// The CMYK profile `options` name: its bytes and as parsed.
///
/// # Errors
/// As [`Raster::write_pdf`] for a profile.
fn read_profile(options: &PdfOptions) -> Result<Option<(Vec<u8>, ColorProfile)>, Error> {
    let Some(file) = &options.cmyk_profile else {
        return Ok(None);
    };
    let bytes = std::fs::read(file).map_err(|error| Error::Read {
        path: file.display().to_string(),
        message: error.to_string(),
    })?;
    let parsed = ColorProfile::new_from_slice(&bytes)
        .map_err(|error| Error::invalid("cmyk_profile", format!("not an ICC profile: {error}")))?;
    if parsed.color_space != DataColorSpace::Cmyk {
        return Err(Error::invalid(
            "cmyk_profile",
            format!("a CMYK profile is wanted, this one is {:?}", parsed.color_space),
        ));
    }
    Ok(Some((bytes, parsed)))
}

/// `raster` written to `path` as a PDF.
///
/// # Errors
/// As [`Raster::write_pdf`].
pub(crate) fn write(raster: &Raster, path: &Path, options: &PdfOptions) -> Result<(), Error> {
    let profile = read_profile(options)?;
    let layout = geometry(raster, options)?;
    let planes = planes(
        raster,
        layout.bleed_pixels,
        options.flatten,
        profile.as_ref().map(|(_, parsed)| parsed),
    )?;
    let rect = |[x1, y1, x2, y2]: [f64; 4]| Rect::new(points(x1), points(y1), points(x2), points(y2));

    // the objects: catalog, page tree, page, image, soft mask, content, profile
    let (catalog_id, tree_id, page_id, image_id, mask_id, content_id, profile_id) = (
        Ref::new(1),
        Ref::new(2),
        Ref::new(3),
        Ref::new(4),
        Ref::new(5),
        Ref::new(6),
        Ref::new(7),
    );
    let name = Name(b"Picture");
    let mut pdf = Pdf::new();
    pdf.catalog(catalog_id).pages(tree_id);
    pdf.pages(tree_id).kids([page_id]).count(1);
    let mut page = pdf.page(page_id);
    page.media_box(rect(layout.page));
    page.bleed_box(rect(layout.bleed_box));
    page.trim_box(rect(layout.trim));
    page.parent(tree_id);
    page.contents(content_id);
    page.resources().x_objects().pair(name, image_id);
    page.finish();

    let compressed = |bytes: &[u8]| miniz_oxide::deflate::compress_to_vec_zlib(bytes, 6);
    // a side is under 2^27, so it fits the 32-bit number of the PDF
    let size = |side: usize| i32::try_from(side).map_err(|_| Error::invalid("raster", "a side is too long"));
    let (image_width, image_height) = (size(layout.image.0)?, size(layout.image.1)?);
    let colour = compressed(&planes.colour);
    let mut image = pdf.image_xobject(image_id, &colour);
    image.filter(Filter::FlateDecode);
    image.width(image_width);
    image.height(image_height);
    image.bits_per_component(8);
    match &profile {
        Some(_) => image.color_space().icc_based(profile_id),
        None => {
            image.color_space().device_rgb();
        }
    }
    if planes.alpha.is_some() {
        image.s_mask(mask_id);
    }
    image.finish();
    if let Some(alpha) = &planes.alpha {
        let coverage = compressed(alpha);
        let mut mask = pdf.image_xobject(mask_id, &coverage);
        mask.filter(Filter::FlateDecode);
        mask.width(image_width);
        mask.height(image_height);
        mask.color_space().device_gray();
        mask.bits_per_component(8);
        mask.finish();
    }
    if let Some((bytes, _)) = &profile {
        pdf.icc_profile(profile_id, bytes).n(4).alternate().device_cmyk();
    }

    // the image's lower left corner is the trim box's, moved out by the bleed pixels; the clip is the bleed box
    let outward = float(layout.bleed_pixels) * layout.per_pixel;
    let [left, bottom, right, top] = layout.bleed_box;
    let mut content = Content::new();
    content
        .save_state()
        .rect(points(left), points(bottom), points(right - left), points(top - bottom))
        .clip_nonzero()
        .end_path()
        .transform([
            points(float(layout.image.0) * layout.per_pixel),
            0.0,
            0.0,
            points(float(layout.image.1) * layout.per_pixel),
            points(layout.trim[0] - outward),
            points(layout.trim[1] - outward),
        ])
        .x_object(name)
        .restore_state();
    if options.crop_marks {
        crop_marks(&mut content, layout.trim, layout.bleed_box);
    }
    pdf.stream(content_id, &content.finish());

    std::fs::write(path, pdf.finish()).map_err(|error| Error::Write {
        path: path.display().to_string(),
        message: error.to_string(),
    })
}
