//! `Raster::write_pdf`: a picture as a one-page PDF at a physical size. The adapter of the PDF writer (`pdf-writer`), the
//! Flate encoder (`miniz_oxide`) and the colour transform to a caller's CMYK profile (`moxcms`); the file's objects are written
//! here, so that the two image streams go from disk to the file and are never held.
//!
//! Layout, in PDF points (1/72 in) from the page's lower left corner: the page (`MediaBox`) is the bleed box plus, with crop
//! marks, a margin; the bleed box (`BleedBox`) is the trim box grown by the bleed on every side; the trim box (`TrimBox`) is
//! the picture at its physical size. The picture is drawn with its bleed (the edge pixels repeated outward, whole pixels) and
//! clipped to the bleed box.

use std::fs::File;
use std::collections::VecDeque;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use miniz_oxide::MZFlush;
use miniz_oxide::deflate::core::{CompressorOxide, create_comp_flags_from_zip_params};
use miniz_oxide::deflate::stream::deflate;
use moxcms::{ColorProfile, DataColorSpace, Layout, Transform8BitExecutor, TransformOptions};
use pdf_writer::Content;

use crate::geometry::MAX_PIXELS;
use crate::marks::{self, MARGIN_MM, MM_PER_INCH, POINTS_PER_INCH, WIDTH_POINTS, Which};
use crate::ranges::{BLEED_MM, DPI};
use crate::raster::eight_bit;
use crate::stream::{Part, failed};
use crate::{Error, Raster};

/// The largest page a PDF reader takes: 14,400 units, 200 in (PDF 1.7, Annex C, implementation limits).
const MAX_PAGE_POINTS: f64 = 14_400.0;

/// What fills the bleed, the strip past the trim that the knife may cut into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BleedFill {
    /// The edge pixels, repeated outward: a stretched edge, which suits a flat background.
    #[default]
    Repeat,
    /// The picture folded at its edge: the pixel just past the edge is the edge pixel, the next the one inside it, and so on
    /// (a picture narrower than its bleed folds back and forth). Keeps the grain, the dots and the tone of the edge, and
    /// shows no stretched line; a detail at the edge shows twice.
    Mirror,
}

impl BleedFill {
    /// The pixel of a line of `length` pixels that fills the place `at`, which is outside the line when negative or `length` and
    /// more (the bleed's pixels are `-bleed..0` and `length..length + bleed`).
    pub(crate) fn source(self, at: isize, length: usize) -> usize {
        let length = isize::try_from(length).unwrap_or(isize::MAX);
        let within = match self {
            Self::Repeat => at.clamp(0, length - 1),
            Self::Mirror => {
                let turn = at.rem_euclid(2 * length);
                if turn < length { turn } else { 2 * length - 1 - turn }
            }
        };
        usize::try_from(within).unwrap_or(0)
    }
}

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
    bleed_fill: BleedFill,
    crop_marks: bool,
    registration_marks: bool,
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
            bleed_fill: BleedFill::Repeat,
            crop_marks: false,
            registration_marks: false,
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

    /// How the bleed is filled: [`BleedFill::Repeat`] (the default) or [`BleedFill::Mirror`]. Has no effect without a bleed.
    #[must_use]
    pub fn bleed_fill(mut self, fill: BleedFill) -> Self {
        self.bleed_fill = fill;
        self
    }

    /// Draws crop marks (thin black lines, 0.25 pt wide and 5 mm long, as vector paths) at the four corners of the trim box,
    /// starting 3 mm outside the bleed box: the page grows by 8 mm on every side to hold them. Off by default.
    #[must_use]
    pub fn crop_marks(mut self, on: bool) -> Self {
        self.crop_marks = on;
        self
    }

    /// Draws registration marks (a circle crossed by two lines, black, 0.25 pt wide and 5 mm across) centred in the margin on the
    /// middle of each side of the trim, outside the bleed box: the page grows by 8 mm on every side to hold them, as for crop
    /// marks (both may be on). Every plate of [`Plates`](crate::Plates) is marked at the same places, so that printed plates
    /// can be lined up on them. Off by default.
    #[must_use]
    pub fn registration_marks(mut self, on: bool) -> Self {
        self.registration_marks = on;
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

impl PdfOptions {
    /// The resolution.
    pub(crate) fn dpi(&self) -> f64 {
        self.dpi
    }

    /// The bleed, in millimetres.
    pub(crate) fn bleed_width(&self) -> f64 {
        self.bleed_mm
    }

    /// What fills the bleed.
    pub(crate) fn fill(&self) -> BleedFill {
        self.bleed_fill
    }

    /// Whether a CMYK profile is asked for.
    pub(crate) fn has_profile(&self) -> bool {
        self.cmyk_profile.is_some()
    }

    /// The marks these options ask for.
    pub(crate) fn marks(&self) -> Which {
        Which {
            crop: self.crop_marks,
            registration: self.registration_marks,
        }
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
    /// Whether the image is one channel of grey (a plate).
    grey: bool,
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
    /// The first and the last `min(bleed, rows)` rows as they are written (bleed columns included), held so that the bleed above
    /// and below can be made of them: the last ones with the number of the row each follows.
    first_rows: Vec<Written>,
    last_rows: VecDeque<Written>,
}

/// A row as it is written: its colour and its alpha, the bleed columns included.
type Written = (Vec<u8>, Vec<u8>);

impl PdfStream {
    /// A PDF of a picture of `width` by `height` pixels written to `path`, under `options`; `limit` is the most pixels the image
    /// (the picture and its bleed) may have.
    ///
    /// # Errors
    /// As [`Raster::write_pdf`], for a profile and a size, and [`Error::Write`] naming `path` for a folder that is not there.
    pub(crate) fn create(
        path: &Path,
        size: (usize, usize),
        options: &PdfOptions,
        limit: usize,
    ) -> Result<Self, Error> {
        Self::open(path, 0, false, size, options, limit)
    }

    /// Page `page` (from 0) of a PDF of plates: one channel of grey, the red of what is pushed. Its scratch files are named for
    /// the page, so that the pages of one PDF can be made one after the other and held together.
    ///
    /// # Errors
    /// As [`PdfStream::create`], and [`Error::Invalid`] naming `cmyk_profile` for options that ask for a profile: a plate is one ink.
    pub(crate) fn plate(
        path: &Path,
        page: usize,
        size: (usize, usize),
        options: &PdfOptions,
        limit: usize,
    ) -> Result<Self, Error> {
        if options.cmyk_profile.is_some() {
            return Err(Error::invalid(
                "cmyk_profile",
                "a plate is one ink in grey: a CMYK profile has nothing to convert",
            ));
        }
        Self::open(path, page, true, size, options, limit)
    }

    fn open(
        path: &Path,
        page: usize,
        grey: bool,
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
        let channels = channels(profile.is_some(), grey);
        let tag = |kind: &str| if page == 0 { kind.to_owned() } else { format!("{kind}{page}") };
        let (colour_part, colour_file) = Part::scratch(path, &tag("colour"))?;
        let (alpha_part, alpha_file) = Part::scratch(path, &tag("alpha"))?;
        Ok(Self {
            grey,
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
            first_rows: Vec::new(),
            last_rows: VecDeque::new(),
        })
    }

    /// The next rows of the picture.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `cmyk_profile` if the conversion fails, [`Error::Write`] naming the path if a stream cannot be
    /// kept.
    pub(crate) fn push(&mut self, band: &Raster) -> Result<(), Error> {
        let flatten = self.options.flatten;
        let channels = channels(self.profile.is_some(), self.grey);
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
                None if self.grey => {
                    for (grey, rgb) in self.device.iter_mut().zip(self.rgb.as_chunks::<3>().0) {
                        *grey = rgb[0];
                    }
                }
                None => self.device.copy_from_slice(&self.rgb),
            }
            // the bleed's columns are made of the row's own pixels, as `bleed_fill` says
            let fill = self.options.bleed_fill;
            let pixels = self.width;
            let columns = (-signed(bleed)..0)
                .chain(0..signed(pixels))
                .chain(signed(pixels)..signed(pixels + bleed))
                .map(|at| fill.source(at, pixels));
            self.colour_row.clear();
            self.alpha_row.clear();
            for column in columns {
                self.colour_row
                    .extend_from_slice(&self.device[column * channels..(column + 1) * channels]);
                self.alpha_row.push(self.opacity[column]);
            }
            self.keep_row()?;
            self.rows += 1;
        }
        Ok(())
    }

    /// Writes the row just made (`colour_row`, `alpha_row`) in its place among the rows: the rows above it first when it is the
    /// last of the top bleed's source, the bleed below when it is the picture's last row.
    ///
    /// # Errors
    /// [`Error::Write`] naming the path if a stream cannot be kept.
    fn keep_row(&mut self) -> Result<(), Error> {
        let (bleed, height) = (self.layout.bleed_pixels, self.height);
        let row = (std::mem::take(&mut self.colour_row), std::mem::take(&mut self.alpha_row));
        let held = bleed.min(height);
        if bleed == 0 {
            self.write_row(&row)?;
        } else {
            if self.rows < held {
                self.first_rows.push(row.clone());
            }
            if self.rows + 1 == held {
                // the top bleed is made of the rows 0..held, now all here, and then those rows themselves go
                let fill = self.options.bleed_fill;
                for above in -signed(bleed)..0 {
                    let source = self.first_rows[fill.source(above, height)].clone();
                    self.write_row(&source)?;
                }
                for first in std::mem::take(&mut self.first_rows) {
                    self.write_row(&first)?;
                }
            } else if self.rows >= held {
                self.write_row(&row)?;
            }
            self.last_rows.push_back(row);
            if self.last_rows.len() > held {
                self.last_rows.pop_front();
            }
            if self.rows + 1 == height {
                let fill = self.options.bleed_fill;
                for below in signed(height)..signed(height + bleed) {
                    let source = self.last_rows[fill.source(below, height) - (height - held)].clone();
                    self.write_row(&source)?;
                }
            }
        }
        Ok(())
    }

    /// One row of the image into the two streams.
    ///
    /// # Errors
    /// [`Error::Write`] naming the path if a stream cannot be kept.
    fn write_row(&mut self, (colour, alpha): &Written) -> Result<(), Error> {
        self.colour.push(colour).map_err(|error| failed(&self.path, error))?;
        if !self.options.flatten {
            self.alpha.push(alpha).map_err(|error| failed(&self.path, error))?;
        }
        Ok(())
    }

    /// The end of the picture: its streams closed, ready to go in a PDF.
    ///
    /// # Errors
    /// [`Error::Write`] naming the path if a stream cannot be closed.
    pub(crate) fn into_page(self, label: Option<String>) -> Result<Page, Error> {
        let Self {
            grey,
            path,
            options,
            layout,
            colour,
            alpha,
            translucent,
            _scratch,
            ..
        } = self;
        let colour = colour.finish().map_err(|error| failed(&path, error))?;
        let alpha = (translucent && !options.flatten)
            .then(|| alpha.finish())
            .transpose()
            .map_err(|error| failed(&path, error))?;
        Ok(Page {
            layout,
            grey,
            label,
            colour,
            alpha,
            _scratch,
        })
    }

    /// The end of the picture: the PDF is written, and in place.
    ///
    /// # Errors
    /// [`Error::Write`] naming the path if it cannot be written.
    pub(crate) fn finish(self) -> Result<(), Error> {
        let (path, options) = (self.path.clone(), self.options.clone());
        let profile = self.profile.as_ref().map(|(bytes, _)| bytes.clone());
        let page = self.into_page(None)?;
        write_document(&path, &options, profile.as_deref(), vec![page])
    }
}

/// A page of a PDF with its image written: the image's streams are in scratch files that go when the page is dropped.
pub(crate) struct Page {
    layout: Geometry,
    grey: bool,
    /// The name the page goes by in a PDF reader (a plate's ink).
    label: Option<String>,
    colour: (File, u64),
    alpha: Option<(File, u64)>,
    _scratch: [Part; 2],
}

/// The PDF of `pages` written to `path`, in place when whole.
///
/// # Errors
/// [`Error::Write`] naming `path` if it cannot be written.
pub(crate) fn write_document(
    path: &Path,
    options: &PdfOptions,
    profile: Option<&[u8]>,
    pages: Vec<Page>,
) -> Result<(), Error> {
    let (part, file) = Part::create(path)?;
    let mut file = BufWriter::new(file);
    document(&mut file, options, profile, pages)
        .and_then(|()| file.flush())
        .map_err(|error| failed(path, error))?;
    drop(file);
    part.commit()
}

/// The channels of the image: four through a CMYK profile, one for grey, else three.
fn channels(profiled: bool, grey: bool) -> usize {
    if profiled {
        4
    } else if grey {
        1
    } else {
        3
    }
}

/// `count` as a signed place of a line.
fn signed(count: usize) -> isize {
    isize::try_from(count).unwrap_or(isize::MAX)
}

/// The marks `which` of a page, each a stroked polyline, outside the bleed of `bleed_mm`.
fn draw_marks(content: &mut Content, trim: [f64; 4], bleed_mm: f64, which: Which) {
    content.set_stroke_gray(0.0).set_line_width(points(WIDTH_POINTS));
    for mark in marks::lines(trim, bleed_mm, POINTS_PER_INCH / MM_PER_INCH, which) {
        let mut corners = mark.iter();
        if let Some(first) = corners.next() {
            content.move_to(points(first[0]), points(first[1]));
        }
        for corner in corners {
            content.line_to(points(corner[0]), points(corner[1]));
        }
        content.stroke();
    }
}

/// Where everything lies, in whole pixels and PDF points; each box is `[left, bottom, right, top]` from the page's lower left.
pub(crate) struct Geometry {
    /// Pixels of edge repeated on each side: the bleed rounded up.
    pub(crate) bleed_pixels: usize,
    /// Pixels of margin for the marks outside the bleed: the margin rounded up (0 with no marks).
    pub(crate) margin_pixels: usize,
    /// The size of the image with its bleed, in pixels.
    pub(crate) image: (usize, usize),
    /// Points a pixel covers.
    pub(crate) per_pixel: f64,
    pub(crate) page: [f64; 4],
    pub(crate) bleed_box: [f64; 4],
    pub(crate) trim: [f64; 4],
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
pub(crate) fn geometry((width, height): (usize, usize), options: &PdfOptions, limit: usize) -> Result<Geometry, Error> {
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
    let margin = if options.marks().crop || options.marks().registration {
        MARGIN_MM / MM_PER_INCH * POINTS_PER_INCH
    } else {
        0.0
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a margin of 8 mm at 2400 dpi is under 760 pixels"
    )]
    let margin_pixels = ((margin / POINTS_PER_INCH * options.dpi * 1e6).round() / 1e6).ceil() as usize;
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
        margin_pixels,
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

/// Writes the PDF of `pages`, each an image whose compressed colour and alpha (if it has one) are streams with their lengths,
/// laid out as its `layout` says, with the ICC profile bytes `profile` of a CMYK image: the header, the objects, the
/// cross-reference table and the trailer. The streams are copied from where they are, so the PDF is never held. A page that
/// has a label is named by it in a PDF reader (`PageLabels`).
///
/// The objects are numbered from 1 in the order they are written: the catalog, the page tree, then for each page the page, its
/// content, its image and the image's soft mask if there is one, and last the profile if there is one.
///
/// # Errors
/// What `out` or a stream's file refuses.
fn document(out: impl Write, options: &PdfOptions, profile: Option<&[u8]>, pages: Vec<Page>) -> io::Result<()> {
    let mut out = Counted { inner: out, at: 0 };
    let mut offsets = Vec::new();
    out.write_all(b"%PDF-1.7\n%\x80\x80\x80\x80\n")?;

    // the number of each page's first object, known before any is written
    let mut first = Vec::new();
    let mut next = 3;
    for page in &pages {
        first.push(next);
        next += 3 + usize::from(page.alpha.is_some());
    }
    let profile_id = next;

    let labels: String = pages
        .iter()
        .enumerate()
        .filter_map(|(at, page)| page.label.as_ref().map(|label| format!("{at} << /P ({label}) >>\n")))
        .collect();
    let labels = if labels.is_empty() {
        String::new()
    } else {
        format!("/PageLabels << /Nums [\n{labels}] >>\n")
    };
    begin(&mut out, &mut offsets)?;
    write!(out, "<<\n/Type /Catalog\n/Pages 2 0 R\n{labels}>>\nendobj\n")?;
    let kids: Vec<String> = first.iter().map(|id| format!("{id} 0 R")).collect();
    begin(&mut out, &mut offsets)?;
    write!(
        out,
        "<<\n/Type /Pages\n/Kids [{}]\n/Count {}\n>>\nendobj\n",
        kids.join(" "),
        pages.len()
    )?;

    for (page, id) in pages.into_iter().zip(first) {
        let layout = &page.layout;
        begin(&mut out, &mut offsets)?;
        write!(
            out,
            "<<\n/Type /Page\n/Parent 2 0 R\n/MediaBox {}\n/BleedBox {}\n/TrimBox {}\n/Contents {} 0 R\n\
             /Resources <<\n/XObject <<\n/Picture {} 0 R\n>>\n>>\n>>\nendobj\n",
            rectangle(layout.page),
            rectangle(layout.bleed_box),
            rectangle(layout.trim),
            id + 1,
            id + 2,
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
        if options.marks().crop || options.marks().registration {
            draw_marks(&mut content, layout.trim, options.bleed_mm, options.marks());
        }
        let content = content.finish();
        stream(&mut out, &mut offsets, "", &*content, content.len() as u64)?;

        let (width, height) = layout.image;
        let space = if profile.is_some() {
            format!("[/ICCBased {profile_id} 0 R]")
        } else if page.grey {
            "/DeviceGray".to_string()
        } else {
            "/DeviceRGB".to_string()
        };
        let soft_mask = if page.alpha.is_some() {
            format!("/SMask {} 0 R\n", id + 3)
        } else {
            String::new()
        };
        let image = format!(
            "/Type /XObject\n/Subtype /Image\n/Filter /FlateDecode\n/Width {width}\n/Height {height}\n/BitsPerComponent 8\n\
             /ColorSpace {space}\n{soft_mask}"
        );
        stream(&mut out, &mut offsets, &image, page.colour.0, page.colour.1)?;
        if let Some((coverage, length)) = page.alpha {
            let gray = format!(
                "/Type /XObject\n/Subtype /Image\n/Filter /FlateDecode\n/Width {width}\n/Height {height}\n/BitsPerComponent 8\n\
                 /ColorSpace /DeviceGray\n"
            );
            stream(&mut out, &mut offsets, &gray, coverage, length)?;
        }
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
