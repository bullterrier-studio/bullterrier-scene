//! `Raster::write_pdf`: a picture as a one-page PDF at a physical size. The adapter of the PDF writer (`pdf-writer`), the
//! Flate encoder (`miniz_oxide`) and the colour transform to a caller's CMYK profile (`moxcms`); the file's objects are written
//! here, so that the two image streams go from disk to the file and are never held.
//!
//! Layout, in PDF points (1/72 in) from the page's lower left corner: the page (`MediaBox`) is the bleed box plus, with crop
//! marks, a margin; the bleed box (`BleedBox`) is the trim box grown by the bleed on every side; the trim box (`TrimBox`) is
//! the picture at its physical size. The picture is drawn with its bleed (the edge pixels repeated outward, whole pixels) and
//! clipped to the bleed box.

use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use miniz_oxide::MZFlush;
use miniz_oxide::deflate::core::{CompressorOxide, create_comp_flags_from_zip_params};
use miniz_oxide::deflate::stream::deflate;
use moxcms::{ColorProfile, DataColorSpace, Layout, Transform8BitExecutor, TransformOptions};
use pdf_writer::Content;

use crate::geometry::MAX_PIXELS;
use crate::ranges::{BLEED_MM, DPI};
use crate::raster::eight_bit;
use crate::stream::{Part, failed};
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

/// `value` in points, as the writer takes it.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a page of at most 14,400 pt fits a float"
)]
fn points(value: f64) -> f32 {
    value as f32
}

/// The zlib level of the image streams: miniz's default.
const FLATE_LEVEL: i32 = 6;

/// A Flate (zlib) stream being written: bytes go in as they are made and the compressed bytes go to a scratch file, so that
/// none is held.
struct Flate {
    compressor: Box<CompressorOxide>,
    spill: BufWriter<File>,
    /// The compressed bytes written so far.
    length: u64,
    /// Room for one call's output.
    room: Vec<u8>,
}

impl Flate {
    fn new(spill: File) -> Self {
        Self {
            // a window of more than 0 bits writes the zlib header
            compressor: Box::new(CompressorOxide::new(create_comp_flags_from_zip_params(
                FLATE_LEVEL,
                1,
                0,
            ))),
            spill: BufWriter::new(spill),
            length: 0,
            room: vec![0; 1 << 16],
        }
    }

    /// Compresses `input`, which follows what came before.
    fn push(&mut self, mut input: &[u8]) -> io::Result<()> {
        while !input.is_empty() {
            let step = deflate(&mut self.compressor, input, &mut self.room, MZFlush::None);
            self.keep(step.bytes_written)?;
            input = &input[step.bytes_consumed..];
        }
        Ok(())
    }

    /// The compressed stream, ended: the scratch file read from its start, and its length.
    fn finish(mut self) -> io::Result<(File, u64)> {
        loop {
            let step = deflate(&mut self.compressor, &[], &mut self.room, MZFlush::Finish);
            self.keep(step.bytes_written)?;
            // `Ok(StreamEnd)` ends the stream; an error cannot come, the room is never empty
            if step.status != Ok(miniz_oxide::MZStatus::Ok) {
                let mut file = self.spill.into_inner().map_err(io::IntoInnerError::into_error)?;
                file.seek(SeekFrom::Start(0))?;
                return Ok((file, self.length));
            }
        }
    }

    /// Writes the first `count` bytes of the room to the scratch file.
    fn keep(&mut self, count: usize) -> io::Result<()> {
        self.length += count as u64;
        self.spill.write_all(&self.room[..count])
    }
}

/// A one-page PDF being written row by row: the picture's rows go in a band at a time, as one Flate stream of colour (and one
/// of alpha) that goes to a scratch file beside the PDF, so that what is held is a row and the codecs' buffers, never the
/// picture, its 8-bit planes or the compressed streams. The scratch files are gone when the PDF is in place, or when this is
/// dropped. [`Raster::write_pdf`] is one push of the whole picture.
pub(crate) struct PdfStream {
    path: PathBuf,
    options: PdfOptions,
    layout: Geometry,
    /// The CMYK profile file's bytes, and the conversion to it.
    profile: Option<(Vec<u8>, std::sync::Arc<Transform8BitExecutor>)>,
    width: usize,
    height: usize,
    colour: Flate,
    alpha: Flate,
    /// The scratch files the two streams are written to: removed when dropped.
    _scratch: [Part; 2],
    /// Whether some pixel is not opaque, so that the alpha is written.
    translucent: bool,
    /// The rows pushed so far.
    rows: usize,
    /// The row being made: `width` pixels of RGB, and of the colour space of the PDF.
    rgb: Vec<u8>,
    device: Vec<u8>,
    opacity: Vec<u8>,
    /// The row as it is written, bleed included.
    colour_row: Vec<u8>,
    alpha_row: Vec<u8>,
}

impl PdfStream {
    /// A PDF of a picture of `width` by `height` pixels written to `path`, under `options`; `limit` is the most pixels the image
    /// (the picture and its bleed) may have.
    ///
    /// # Errors
    /// As [`Raster::write_pdf`], for a profile and a size, and [`Error::Write`] naming `path` for a folder that is not there.
    pub(crate) fn create(
        path: &Path,
        (width, height): (usize, usize),
        options: &PdfOptions,
        limit: usize,
    ) -> Result<Self, Error> {
        let parsed = read_profile(options)?;
        let layout = geometry((width, height), options, limit)?;
        let profile = parsed
            .map(|(bytes, cmyk)| {
                ColorProfile::new_srgb()
                    .create_transform_8bit(Layout::Rgb, &cmyk, Layout::Rgba, TransformOptions::default())
                    .map(|transform| (bytes, transform))
            })
            .transpose()
            .map_err(|error| {
                Error::invalid(
                    "cmyk_profile",
                    format!("no conversion from sRGB to the profile: {error}"),
                )
            })?;
        let channels = if profile.is_some() { 4 } else { 3 };
        let (colour_part, colour_file) = Part::scratch(path, "colour")?;
        let (alpha_part, alpha_file) = Part::scratch(path, "alpha")?;
        Ok(Self {
            path: path.to_path_buf(),
            options: options.clone(),
            layout,
            profile,
            width,
            height,
            colour: Flate::new(colour_file),
            alpha: Flate::new(alpha_file),
            _scratch: [colour_part, alpha_part],
            translucent: false,
            rows: 0,
            rgb: vec![0; width * 3],
            device: vec![0; width * channels],
            opacity: Vec::with_capacity(width),
            colour_row: Vec::new(),
            alpha_row: Vec::new(),
        })
    }

    /// The next rows of the picture.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `cmyk_profile` if the conversion fails, [`Error::Write`] naming the path if a stream cannot be
    /// kept.
    pub(crate) fn push(&mut self, band: &Raster) -> Result<(), Error> {
        let flatten = self.options.flatten;
        let channels = if self.profile.is_some() { 4 } else { 3 };
        let bleed = self.layout.bleed_pixels;
        for line in band.pixels().chunks_exact(self.width * 4) {
            self.opacity.clear();
            for (pixel, out) in line
                .as_chunks::<4>()
                .0
                .iter()
                .zip(self.rgb.as_chunks_mut::<3>().0.iter_mut())
            {
                let covered = pixel[3];
                self.translucent |= covered < 1.0;
                for (level, slot) in pixel[..3].iter().zip(out) {
                    *slot = eight_bit(if flatten {
                        level * covered + (1.0 - covered)
                    } else {
                        *level
                    });
                }
                self.opacity.push(eight_bit(covered));
            }
            match &self.profile {
                Some((_, convert)) => {
                    convert
                        .transform(&self.rgb, &mut self.device)
                        .map_err(|error| Error::invalid("cmyk_profile", format!("the conversion failed: {error}")))?;
                }
                None => self.device.copy_from_slice(&self.rgb),
            }
            // the picture's edge pixels are repeated outward: a row at the left and right ends of a line, and the first and last
            // rows above and below
            let last = (self.width - 1) * channels;
            self.colour_row.clear();
            self.colour_row.extend(self.device[..channels].repeat(bleed));
            self.colour_row.extend_from_slice(&self.device);
            self.colour_row.extend(self.device[last..].repeat(bleed));
            self.alpha_row.clear();
            self.alpha_row.extend(std::iter::repeat_n(self.opacity[0], bleed));
            self.alpha_row.extend_from_slice(&self.opacity);
            self.alpha_row
                .extend(std::iter::repeat_n(self.opacity[self.width - 1], bleed));
            let times = 1 + bleed * (usize::from(self.rows == 0) + usize::from(self.rows + 1 == self.height));
            for _ in 0..times {
                self.colour
                    .push(&self.colour_row)
                    .map_err(|error| failed(&self.path, error))?;
                if !flatten {
                    self.alpha
                        .push(&self.alpha_row)
                        .map_err(|error| failed(&self.path, error))?;
                }
            }
            self.rows += 1;
        }
        Ok(())
    }

    /// The end of the picture: the PDF is written, and in place.
    ///
    /// # Errors
    /// [`Error::Write`] naming the path if it cannot be written.
    pub(crate) fn finish(self) -> Result<(), Error> {
        let Self {
            path,
            options,
            layout,
            profile,
            colour,
            alpha,
            translucent,
            ..
        } = self;
        let colour = colour.finish().map_err(|error| failed(&path, error))?;
        let alpha = (translucent && !options.flatten)
            .then(|| alpha.finish())
            .transpose()
            .map_err(|error| failed(&path, error))?;
        let (part, file) = Part::create(&path)?;
        let mut file = BufWriter::new(file);
        document(
            &mut file,
            &layout,
            &options,
            profile.as_ref().map(|(bytes, _)| bytes.as_slice()),
            colour,
            alpha,
        )
        .and_then(|()| file.flush())
        .map_err(|error| failed(&path, error))?;
        drop(file);
        part.commit()
    }
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

/// The layout of a picture of `width` by `height` pixels under `options`, its image (the picture and its bleed) of at most
/// `limit` pixels.
///
/// # Errors
/// As [`Raster::write_pdf`] for a size.
fn geometry((width, height): (usize, usize), options: &PdfOptions, limit: usize) -> Result<Geometry, Error> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a bleed of at most 50 mm at 2400 dpi is under 4,725 pixels"
    )]
    // rounded to a millionth of a pixel first, so that 0.2 mm at 254 dpi, which is 2.0000000000000004 pixels in doubles, is 2
    let bleed_pixels = ((options.bleed_mm / MM_PER_INCH * options.dpi * 1e6).round() / 1e6).ceil() as usize;
    let image = (width + 2 * bleed_pixels, height + 2 * bleed_pixels);
    if image.0.saturating_mul(image.1) > limit {
        return Err(Error::invalid(
            if bleed_pixels == 0 { "raster" } else { "bleed_mm" },
            format!(
                "the image of {} x {} pixels has more than {limit} pixels (2^27)",
                image.0, image.1
            ),
        ));
    }
    let per_pixel = POINTS_PER_INCH / options.dpi;
    let trim_size = (float(width) * per_pixel, float(height) * per_pixel);
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

/// A writer that counts the bytes it is given: where each object starts, for the cross-reference table.
struct Counted<W> {
    inner: W,
    at: u64,
}

impl<W: Write> Write for Counted<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.inner.write(bytes)?;
        self.at += count as u64;
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// The start of indirect object `offsets.len() + 1`: its offset is kept, its number written.
fn begin(out: &mut Counted<impl Write>, offsets: &mut Vec<u64>) -> io::Result<()> {
    offsets.push(out.at);
    writeln!(out, "{} 0 obj", offsets.len())
}

/// A stream object: the dictionary `entries` (lines), then `length` bytes read from `data`.
fn stream(
    out: &mut Counted<impl Write>,
    offsets: &mut Vec<u64>,
    entries: &str,
    mut data: impl Read,
    length: u64,
) -> io::Result<()> {
    begin(out, offsets)?;
    write!(out, "<<\n{entries}/Length {length}\n>>\nstream\n")?;
    io::copy(&mut data, out)?;
    out.write_all(b"\nendstream\nendobj\n")
}

/// `[left bottom right top]` in points, as a PDF array.
fn rectangle([left, bottom, right, top]: [f64; 4]) -> String {
    format!(
        "[{} {} {} {}]",
        points(left),
        points(bottom),
        points(right),
        points(top)
    )
}

/// Writes the PDF of an image whose compressed colour is `colour` and alpha `alpha` (if it has one), each a stream and its
/// length, laid out as `layout` says, with the ICC profile bytes `profile` of a CMYK image: the header, the objects, the
/// cross-reference table and the trailer. The streams are copied from where they are, so the PDF is never held.
///
/// The objects are numbered from 1 in the order they are written: the catalog, the page tree, the page, its content, the image,
/// then the image's soft mask and the profile if there are any.
///
/// # Errors
/// What `out` or a stream's file refuses.
fn document(
    out: impl Write,
    layout: &Geometry,
    options: &PdfOptions,
    profile: Option<&[u8]>,
    colour: (File, u64),
    alpha: Option<(File, u64)>,
) -> io::Result<()> {
    let mut out = Counted { inner: out, at: 0 };
    let mut offsets = Vec::new();
    out.write_all(b"%PDF-1.7\n%\x80\x80\x80\x80\n")?;

    begin(&mut out, &mut offsets)?;
    write!(out, "<<\n/Type /Catalog\n/Pages 2 0 R\n>>\nendobj\n")?;
    begin(&mut out, &mut offsets)?;
    write!(out, "<<\n/Type /Pages\n/Kids [3 0 R]\n/Count 1\n>>\nendobj\n")?;
    begin(&mut out, &mut offsets)?;
    write!(
        out,
        "<<\n/Type /Page\n/Parent 2 0 R\n/MediaBox {}\n/BleedBox {}\n/TrimBox {}\n/Contents 4 0 R\n\
         /Resources <<\n/XObject <<\n/Picture 5 0 R\n>>\n>>\n>>\nendobj\n",
        rectangle(layout.page),
        rectangle(layout.bleed_box),
        rectangle(layout.trim)
    )?;

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
        .x_object(pdf_writer::Name(b"Picture"))
        .restore_state();
    if options.crop_marks {
        crop_marks(&mut content, layout.trim, layout.bleed_box);
    }
    let content = content.finish();
    stream(&mut out, &mut offsets, "", &*content, content.len() as u64)?;

    let (width, height) = layout.image;
    let (mask, profile_id) = (6, 6 + usize::from(alpha.is_some()));
    let space = if profile.is_some() {
        format!("[/ICCBased {profile_id} 0 R]")
    } else {
        "/DeviceRGB".to_string()
    };
    let soft_mask = if alpha.is_some() {
        format!("/SMask {mask} 0 R\n")
    } else {
        String::new()
    };
    let image = format!(
        "/Type /XObject\n/Subtype /Image\n/Filter /FlateDecode\n/Width {width}\n/Height {height}\n/BitsPerComponent 8\n\
         /ColorSpace {space}\n{soft_mask}"
    );
    stream(&mut out, &mut offsets, &image, colour.0, colour.1)?;
    if let Some((coverage, length)) = alpha {
        let gray = format!(
            "/Type /XObject\n/Subtype /Image\n/Filter /FlateDecode\n/Width {width}\n/Height {height}\n/BitsPerComponent 8\n\
             /ColorSpace /DeviceGray\n"
        );
        stream(&mut out, &mut offsets, &gray, coverage, length)?;
    }
    if let Some(bytes) = profile {
        stream(
            &mut out,
            &mut offsets,
            "/N 4\n/Alternate /DeviceCMYK\n",
            bytes,
            bytes.len() as u64,
        )?;
    }

    let table = out.at;
    let count = offsets.len() + 1;
    write!(out, "xref\n0 {count}\n0000000000 65535 f \n")?;
    for offset in offsets {
        writeln!(out, "{offset:010} 00000 n ")?;
    }
    write!(
        out,
        "trailer\n<<\n/Size {count}\n/Root 1 0 R\n>>\nstartxref\n{table}\n%%EOF\n"
    )
}

/// `raster` written to `path` as a PDF.
///
/// # Errors
/// As [`Raster::write_pdf`].
pub(crate) fn write(raster: &Raster, path: &Path, options: &PdfOptions) -> Result<(), Error> {
    let mut stream = PdfStream::create(path, (raster.width(), raster.height()), options, MAX_PIXELS)?;
    stream.push(raster)?;
    stream.finish()
}
