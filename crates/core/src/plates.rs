//! Plates: one plane of ink coverage for each ink a scene prints in.

use std::path::{Path, PathBuf};

use crate::blend::Blend;
use crate::distance::{distance_to_bare, float};
use crate::feather;
use crate::geometry::MAX_PIXELS;
use crate::layer::{Base, check_name};
use crate::marks::{self, MM_PER_INCH, POINTS_PER_INCH, WIDTH_POINTS, stroke};
use crate::pdf::{Geometry, PdfStream, geometry, write_document};
use crate::ranges::{CHOKE, FEATHER, TOLERANCE};
use crate::raster::eight_bit;
use crate::source::fill::Fill;
use crate::stream::{failed, write_grey_png};
use crate::{Error, Layer, PdfOptions, Raster, Scene, render};

/// The ink coverage of a picture, one plane for each ink, at the size a scene was drawn at.
#[derive(Clone, PartialEq)]
pub struct Plates {
    columns: usize,
    rows: usize,
    planes: Vec<(String, Vec<f32>)>,
}

impl std::fmt::Debug for Plates {
    /// The size and the inks, never the values.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Plates {{ columns: {}, rows: {}, inks: {:?} }}",
            self.columns,
            self.rows,
            self.names()
        )
    }
}

impl Plates {
    /// The number of columns of every plane.
    #[must_use]
    pub fn width(&self) -> usize {
        self.columns
    }

    /// The number of rows of every plane.
    #[must_use]
    pub fn height(&self) -> usize {
        self.rows
    }

    /// The inks, in the order the layers name them.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.planes.iter().map(|(name, _)| name.as_str()).collect()
    }

    /// How much of ink `name` lies on each pixel, 0 to 1: `width * height` values, row after row from the top.
    #[must_use]
    pub fn plane(&self, name: &str) -> Option<&[f32]> {
        self.planes
            .iter()
            .find(|(found, _)| found == name)
            .map(|(_, values)| values.as_slice())
    }
}

/// A white plate for the inks to print on, for a dark garment or film: where ink is, a flood of white is laid first so that the
/// inks show their own colours, except where the garment's own colour is what the picture means to show. Built by `new` and the
/// setters, each checked where it is given.
///
/// ```text
/// Underbase::new("#1b1b1b")?.choke(1.5)?.tolerance(0.04)?.feather(1.0)?
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Underbase {
    garment: [f32; 3],
    choke: f64,
    tolerance: f64,
    feather: f64,
    name: String,
}

impl Underbase {
    /// An underbase for a garment of the colour `garment` (`#rrggbb`): under all the ink, no choke, knocked out only where the
    /// picture is exactly the garment's colour, no feather, the plate named `white`.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `garment` for a colour that is not `#rrggbb`.
    pub fn new(garment: &str) -> Result<Self, Error> {
        let rgba = Fill::parse(garment).map_err(|error| error.about("garment"))?.rgba();
        if garment.len() != "#rrggbb".len() {
            return Err(Error::invalid(
                "garment",
                format!("a garment colour is `#rrggbb`, with no alpha, got {garment:?}"),
            ));
        }
        Ok(Self {
            garment: [rgba[0], rgba[1], rgba[2]],
            choke: 0.0,
            tolerance: 0.0,
            feather: 0.0,
            name: "white".to_owned(),
        })
    }

    /// The white is drawn in from the edge of the ink by `px` design pixels, so that no white shows past the ink where the
    /// plates are a little out of register: a pixel of the white is how far its centre lies from the nearest bare pixel's, less
    /// `px`, held between 0 and 1 (the ink's edge is half a pixel from a covered centre, so an edge pixel is whole at 0). The ink is where the plates' union is at least half; the edge of the
    /// canvas is not bare, so art that runs off the page is not choked there. 0 is no choke.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `choke` unless it is a number from 0 to 10,000.
    pub fn choke(mut self, px: f64) -> Result<Self, Error> {
        self.choke = CHOKE.check("choke", px)?;
        Ok(self)
    }

    /// Where the picture, laid over the garment, is within `value` of the garment's colour there is no white: the garment shows.
    /// The distance is the straight one between two sRGB colours, as a share of the longest there is (black to white), so 0
    /// is the exact colour, 0.02 about five levels in each channel and 1 everything. Default 0.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `tolerance` unless it is a number from 0 to 1.
    pub fn tolerance(mut self, value: f64) -> Result<Self, Error> {
        self.tolerance = TOLERANCE.check("tolerance", value)?;
        Ok(self)
    }

    /// The edge of the knocked-out part is blurred by `px` design pixels (the standard deviation of the blur, as for a mask's
    /// `feather`), so the white fades out where it meets the garment's colour. 0 is a hard edge.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `feather` unless it is a number from 0 to 1,000,000.
    pub fn feather(mut self, px: f64) -> Result<Self, Error> {
        self.feather = FEATHER.check("feather", px)?;
        Ok(self)
    }

    /// The plate is called `name` (default `white`): an ink's name, so it is letters, digits and `_`, starting with a letter.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `name` for one that is not allowed.
    pub fn name(mut self, name: &str) -> Result<Self, Error> {
        check_name("name", name)?;
        name.clone_into(&mut self.name);
        Ok(self)
    }
}

impl Scene {
    /// The [`plates`](Scene::plates) of the scene with a white plate before them, first, the one printed first: its coverage is
    /// the union of the inks' (where it is at least half) choked by the underbase's width, times one minus the knockout, where
    /// the picture of the scene laid over the garment's colour is within the tolerance of that colour, blurred by the feather.
    /// A scene that is the garment's colour, as a layer, and the art over it has the picture to compare; with no layer for the
    /// garment, what the scene leaves transparent is the garment's colour.
    ///
    /// # Errors
    /// As [`plates`](Scene::plates), counting the white plate in the values; [`Error::Invalid`] naming `name` for an underbase
    /// named as one of the inks, and `feather` for a blur whose working plane would hold more than 2^27 values.
    pub fn plates_with_underbase(&self, size: impl Into<i128>, underbase: &Underbase) -> Result<Plates, Error> {
        let size = size.into();
        if ink_names(self.drawn()).contains(&underbase.name.as_str()) {
            return Err(Error::invalid(
                "name",
                format!(
                    "{:?} is the name of an ink of the scene as well as of the underbase",
                    underbase.name
                ),
            ));
        }
        let mut plates = make_with(self, size, 1)?;
        let (columns, rows) = (plates.columns, plates.rows);
        let scale = float(columns.max(rows)) / f64::from(self.width().max(self.height()));
        let sigma = underbase.feather * scale;
        feather::fits(
            "feather",
            (columns, rows),
            (feather::reach(sigma), feather::reach(sigma)),
        )?;
        let picture = self.render(size)?;
        let mut union = vec![1.0_f32; columns * rows];
        for (_, plane) in &plates.planes {
            for (value, coverage) in union.iter_mut().zip(plane) {
                *value *= 1.0 - coverage;
            }
        }
        let covered: Vec<bool> = union.iter().map(|rest| 1.0 - rest >= 0.5).collect();
        let reach = underbase.choke * scale;
        let mut white = distance_to_bare((columns, rows), &covered);
        for (value, covered) in white.iter_mut().zip(&covered) {
            *value = if *covered {
                #[expect(clippy::cast_possible_truncation, reason = "a share from 0 to 1 is held as a float")]
                {
                    (f64::from(*value) - reach).clamp(0.0, 1.0) as f32
                }
            } else {
                0.0
            };
        }
        let garment = underbase.garment.map(f64::from);
        let mut knock: Vec<f32> = picture
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| {
                let alpha = f64::from(pixel[3]);
                let apart: f64 = (0..3)
                    .map(|channel| {
                        let laid = f64::from(pixel[channel]) * alpha + garment[channel] * (1.0 - alpha);
                        (laid - garment[channel]).powi(2)
                    })
                    .sum();
                // the longest distance between two colours is the cube's diagonal, the square root of 3
                f32::from(u8::from(apart.sqrt() / 3.0_f64.sqrt() <= underbase.tolerance))
            })
            .collect();
        if sigma > 0.0 {
            feather::blur(&mut knock, (columns, rows), (sigma, sigma));
        }
        for (value, knocked) in white.iter_mut().zip(&knock) {
            *value *= 1.0 - knocked;
        }
        plates.planes.insert(0, (underbase.name.clone(), white));
        Ok(plates)
    }
}

impl Plates {
    /// Writes each plate as a grey PNG, `<stem>-<ink>.png` for each ink (`out/poster` gives `out/poster-pink.png`), and gives the
    /// paths in plate order. Ink is black on white paper (255 is paper, 0 is full ink), as a film positive is. A plate is its
    /// plane with the bleed of `options` (filled as `options` fill it, repeated or mirrored), then, with `crop_marks` or
    /// `registration_marks`, 8 mm of margin on every side with the marks, black, at the same pixels on every plate; the file says
    /// the `dpi` of `options` (the same as a [`PdfOptions`] page of these plates). `flatten` has no meaning for a plate and is
    /// ignored. A plate is held while it is written, one at a time. The files are written one after the other: one that cannot
    /// be written leaves those before it.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `cmyk_profile` for options that ask for a profile (a plate is one ink in grey), `dpi` for a page
    /// past 14,400 pt, `bleed_mm` for a plate and its bleed and margin of more than 2^27 pixels; [`Error::Write`] naming the file
    /// for a path that cannot be written.
    pub fn write_png(&self, stem: &Path, options: &PdfOptions) -> Result<Vec<PathBuf>, Error> {
        let layout = self.layout(options)?;
        let name = stem
            .file_name()
            .ok_or_else(|| failed(stem, "the path has no file name"))?
            .to_string_lossy()
            .into_owned();
        let mut written = Vec::with_capacity(self.planes.len());
        for (ink, plane) in &self.planes {
            let path = stem.with_file_name(format!("{name}-{ink}.png"));
            let (size, bytes) = self.page(plane, options, &layout);
            write_grey_png(&path, size, &bytes, Some(options.dpi()))?;
            written.push(path);
        }
        Ok(written)
    }

    /// Writes the plates to `path` as one PDF with a page for each, in plate order, named by its ink (the page labels a PDF reader
    /// shows). A page is the page [`Raster::write_pdf`](crate::Raster::write_pdf) makes of a picture with these `options`
    /// (the physical size at `dpi`, the bleed, the marks as vector paths, the boxes), with the plate as one 8-bit grey image,
    /// black for ink and white for paper, so the marks are at the same points on every page. `flatten` has no meaning for a plate and is ignored. The file is
    /// made whole or not at all, and the images go through scratch files beside it, as for a single PDF.
    ///
    /// # Errors
    /// As [`write_png`](Plates::write_png) for the options; [`Error::Write`] naming `path` for a path that cannot be written.
    pub fn write_pdf(&self, path: &Path, options: &PdfOptions) -> Result<(), Error> {
        self.layout(options)?;
        let band_rows = render::default_band_rows(self.columns, self.rows);
        let mut pages = Vec::with_capacity(self.planes.len());
        for (at, (ink, plane)) in self.planes.iter().enumerate() {
            let mut stream = PdfStream::plate(path, at, (self.columns, self.rows), options, MAX_PIXELS)?;
            for band in plane.chunks(band_rows * self.columns) {
                let paper = band
                    .iter()
                    .flat_map(|coverage| {
                        let grey = f32::from(255 - eight_bit(*coverage)) / 255.0;
                        [grey, grey, grey, 1.0]
                    })
                    .collect();
                stream.push(&Raster::made(self.columns, band.len() / self.columns, paper))?;
            }
            pages.push(stream.into_page(Some(ink.clone()))?);
        }
        write_document(path, options, None, pages)
    }

    /// The layout of a page of these plates under `options`, refused if the options cannot be a plate's.
    ///
    /// # Errors
    /// As [`write_png`](Plates::write_png).
    fn layout(&self, options: &PdfOptions) -> Result<Geometry, Error> {
        options.for_plate()?;
        let layout = geometry((self.columns, self.rows), options, MAX_PIXELS)?;
        let pad = layout.bleed_pixels + layout.margin_pixels;
        if (self.columns + 2 * pad).saturating_mul(self.rows + 2 * pad) > MAX_PIXELS {
            return Err(Error::invalid(
                "bleed_mm",
                format!(
                    "the plate with its bleed and margin of {} x {} pixels has more than {MAX_PIXELS} pixels (2^27)",
                    self.columns + 2 * pad,
                    self.rows + 2 * pad
                ),
            ));
        }
        Ok(layout)
    }

    /// The page of one plate as a PNG holds it: its size and the grey of each pixel.
    #[expect(
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        reason = "a side of a plate is under 2^27 pixels: exact in a double and in a signed place"
    )]
    fn page(&self, plane: &[f32], options: &PdfOptions, layout: &Geometry) -> ((usize, usize), Vec<u8>) {
        let pad = layout.bleed_pixels + layout.margin_pixels;
        let (width, height) = (self.columns + 2 * pad, self.rows + 2 * pad);
        let mut grey = vec![255_u8; width * height];
        let (bleed, margin) = (layout.bleed_pixels, layout.margin_pixels);
        let fill = options.fill();
        let sources = |count: usize| -> Vec<usize> {
            (0..count + 2 * bleed)
                .map(|at| fill.source(at as isize - bleed as isize, count))
                .collect()
        };
        let (across, down) = (sources(self.columns), sources(self.rows));
        for (y, from_y) in down.iter().enumerate() {
            let line = &mut grey[(margin + y) * width + margin..(margin + y) * width + margin + across.len()];
            for (value, from_x) in line.iter_mut().zip(&across) {
                *value = 255 - eight_bit(plane[from_y * self.columns + from_x]);
            }
        }
        // the marks, in pixels from the page's lower left (y up), drawn black
        let per_mm = options.dpi() / MM_PER_INCH;
        let trim = [
            pad as f64,
            pad as f64,
            (pad + self.columns) as f64,
            (pad + self.rows) as f64,
        ];
        let half = WIDTH_POINTS / POINTS_PER_INCH * options.dpi() / 2.0;
        for mark in marks::lines(trim, options.bleed_width(), per_mm, options.marks()) {
            for segment in mark.windows(2) {
                let flipped = |point: [f64; 2]| [point[0], height as f64 - point[1]];
                stroke(&mut grey, width, flipped(segment[0]), flipped(segment[1]), half);
            }
        }
        ((width, height), grey)
    }
}

/// The most values the planes of plates hold together: 2^27 floats, 512 MiB.
const MAX_VALUES: usize = 1 << 27;

/// The inks the layers of `drawn` name, in the order they first appear.
fn ink_names(drawn: &[Layer]) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    for name in drawn.iter().flat_map(|layer| &layer.join.inks) {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
    names
}

/// `layer` as ink `ink` draws it, or `None` for a layer that does not print in it: the layer with its last look making the
/// plane of that ink (or the shape of its alpha), joined normally.
///
/// # Errors
/// [`Error::Invalid`] naming `<layer>.inks` for a layer that is light, or that cannot hand its inks on.
fn for_ink(layer: &Layer, at: usize, ink: &str) -> Result<Option<Layer>, Error> {
    let Some(index) = layer.join.inks.iter().position(|name| name == ink) else {
        return Ok(None);
    };
    let label = layer.name.clone().unwrap_or_else(|| format!("layer{}", at + 1));
    let refused = |error: Error| error.in_layer(&label);
    if layer.join.light {
        return Err(refused(Error::invalid(
            "inks",
            "a light layer makes its own alpha from its colour, so it has no shape to print",
        )));
    }
    let mut plate = layer.clone();
    let Base::Body(body) = &mut plate.base else {
        return Err(refused(Error::invalid("inks", "the layer was not resolved by a scene")));
    };
    body.finish = body.finish.for_plate(index, layer.join.inks.len()).map_err(refused)?;
    plate.join.blend = Blend::Normal;
    Ok(Some(plate))
}

/// The plates of `scene` at `size`.
///
/// # Errors
/// As [`Scene::plates`].
pub(crate) fn make(scene: &Scene, size: i128) -> Result<Plates, Error> {
    make_with(scene, size, 0)
}

/// The plates of `scene` at `size`, with room counted for `extra` more planes that the caller adds.
///
/// # Errors
/// As [`Scene::plates`].
pub(crate) fn make_with(scene: &Scene, size: i128, extra: usize) -> Result<Plates, Error> {
    let (columns, rows) = scene.picture_size(size)?;
    let names = ink_names(scene.drawn());
    if names.is_empty() {
        return Err(Error::invalid(
            "inks",
            "no layer names an ink: `Layer::inks` says what each layer prints in",
        ));
    }
    if columns.saturating_mul(rows).saturating_mul(names.len() + extra) > MAX_VALUES {
        return Err(Error::invalid(
            "size",
            format!(
                "{columns} x {rows} pixels in {} plates are more than the {MAX_VALUES} values plates hold",
                names.len() + extra
            ),
        ));
    }
    let mut planes = Vec::with_capacity(names.len());
    for ink in names {
        let layers = scene
            .drawn()
            .iter()
            .enumerate()
            .filter_map(|(at, layer)| for_ink(layer, at, ink).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        let picture = scene.drawing(layers).render(size)?;
        let coverage = picture
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[3])
            .collect();
        planes.push((ink.to_owned(), coverage));
    }
    Ok(Plates { columns, rows, planes })
}
