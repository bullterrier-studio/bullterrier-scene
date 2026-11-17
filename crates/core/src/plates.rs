//! Plates: one plane of ink coverage for each ink a scene prints in.

use std::path::{Path, PathBuf};

use crate::blend::Blend;
use crate::geometry::MAX_PIXELS;
use crate::layer::Base;
use crate::marks::{self, MM_PER_INCH, POINTS_PER_INCH, WIDTH_POINTS};
use crate::pdf::{Geometry, PdfStream, geometry, write_document};
use crate::raster::eight_bit;
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
            for (first, band) in plane.chunks(band_rows * self.columns).enumerate() {
                let _ = first;
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
        if options.has_profile() {
            return Err(Error::invalid(
                "cmyk_profile",
                "a plate is one ink in grey: a CMYK profile has nothing to convert",
            ));
        }
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
        let trim = [pad as f64, pad as f64, (pad + self.columns) as f64, (pad + self.rows) as f64];
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

/// Black over `grey` (`width` pixels a row) along the segment from `from` to `to`, `half` pixels either side: each pixel gets
/// the share of ink that its distance from the segment leaves it (a pixel `half` away has half, one `half + 0.5` away none).
fn stroke(grey: &mut [u8], width: usize, from: [f64; 2], to: [f64; 2], half: f64) {
    let reach = half + 1.0;
    let height = grey.len() / width;
    let low = |a: f64, b: f64| (a.min(b) - reach).floor().max(0.0) as usize;
    let high = |a: f64, b: f64, limit: usize| ((a.max(b) + reach).ceil().max(0.0) as usize).min(limit);
    let direction = [to[0] - from[0], to[1] - from[1]];
    let length_squared = direction[0] * direction[0] + direction[1] * direction[1];
    for y in low(from[1], to[1])..high(from[1], to[1], height) {
        for x in low(from[0], to[0])..high(from[0], to[0], width) {
            let centre = [x as f64 + 0.5, y as f64 + 0.5];
            let along = if length_squared == 0.0 {
                0.0
            } else {
                (((centre[0] - from[0]) * direction[0] + (centre[1] - from[1]) * direction[1]) / length_squared).clamp(0.0, 1.0)
            };
            let nearest = [from[0] + along * direction[0], from[1] + along * direction[1]];
            let distance = (centre[0] - nearest[0]).hypot(centre[1] - nearest[1]);
            let ink = eight_bit((half + 0.5 - distance).clamp(0.0, 1.0) as f32);
            let pixel = &mut grey[y * width + x];
            *pixel = (*pixel).min(255 - ink);
        }
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
fn for_ink(layer: &Layer, place: usize, ink: &str) -> Result<Option<Layer>, Error> {
    let Some(index) = layer.join.inks.iter().position(|name| name == ink) else {
        return Ok(None);
    };
    let label = layer
        .name
        .clone()
        .unwrap_or_else(|| format!("layer{}", place + 1));
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
    body.finish = body
        .finish
        .for_plate(index, layer.join.inks.len())
        .map_err(refused)?;
    plate.join.blend = Blend::Normal;
    Ok(Some(plate))
}

/// The plates of `scene` at `size`.
///
/// # Errors
/// As [`Scene::plates`].
pub(crate) fn make(scene: &Scene, size: i128) -> Result<Plates, Error> {
    let (columns, rows) = scene.picture_size(size)?;
    let names = ink_names(scene.drawn());
    if names.is_empty() {
        return Err(Error::invalid(
            "inks",
            "no layer names an ink: `Layer::inks` says what each layer prints in",
        ));
    }
    if columns.saturating_mul(rows).saturating_mul(names.len()) > MAX_VALUES {
        return Err(Error::invalid(
            "size",
            format!(
                "{columns} x {rows} pixels in {} plates are more than the {MAX_VALUES} values plates hold",
                names.len()
            ),
        ));
    }
    let mut planes = Vec::with_capacity(names.len());
    for ink in names {
        let layers = scene
            .drawn()
            .iter()
            .enumerate()
            .filter_map(|(place, layer)| for_ink(layer, place, ink).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        let picture = scene.drawing(layers).render(size)?;
        let coverage = picture.pixels().as_chunks::<4>().0.iter().map(|pixel| pixel[3]).collect();
        planes.push((ink.to_owned(), coverage));
    }
    Ok(Plates { columns, rows, planes })
}
