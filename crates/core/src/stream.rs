//! A picture written to a file as it is drawn, band after band, never held whole. The file is made under a temporary name
//! beside its place and renamed when the picture is complete, so a render that fails leaves no file (and one that is already
//! there stays as it was until the new one is whole).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::raster::{eight_bit, pixels_per_metre};
use crate::{Error, Raster};

/// The file `path` as [`Error::Write`] names it, with what went wrong.
pub(crate) fn failed(path: &Path, error: impl std::fmt::Display) -> Error {
    Error::Write {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}

/// A file being written under a temporary name: gone unless [`Part::commit`] renames it into place.
pub(crate) struct Part {
    target: PathBuf,
    temp: PathBuf,
    committed: bool,
}

impl Part {
    /// The temporary file of `target`, created.
    ///
    /// # Errors
    /// [`Error::Write`] naming `target` for a folder that is not there or a path that cannot be written.
    pub(crate) fn create(target: &Path) -> Result<(Self, File), Error> {
        Self::open(target, "part", File::options().write(true))
    }

    /// A scratch file beside `target` that holds `kind` while the target is made: open for writing and for reading back, and
    /// gone when the returned `Part` is dropped (it is never committed).
    ///
    /// # Errors
    /// As [`Part::create`].
    pub(crate) fn scratch(target: &Path, kind: &str) -> Result<(Self, File), Error> {
        Self::open(target, &format!("{kind}.part"), File::options().read(true).write(true))
    }

    /// The file `.<name of target>.<suffix>` beside `target`, created (emptied if it was there) with `options`.
    fn open(target: &Path, suffix: &str, options: &mut std::fs::OpenOptions) -> Result<(Self, File), Error> {
        let name = target
            .file_name()
            .ok_or_else(|| failed(target, "the path has no file name"))?;
        let mut temp_name = std::ffi::OsString::from(".");
        temp_name.push(name);
        temp_name.push(".");
        temp_name.push(suffix);
        let temp = target.with_file_name(temp_name);
        let file = options
            .create(true)
            .truncate(true)
            .open(&temp)
            .map_err(|error| failed(target, error))?;
        Ok((
            Self {
                target: target.to_path_buf(),
                temp,
                committed: false,
            },
            file,
        ))
    }

    /// The file under its own name, replacing one that was there.
    ///
    /// # Errors
    /// [`Error::Write`] naming the target if it cannot be put in place.
    pub(crate) fn commit(mut self) -> Result<(), Error> {
        std::fs::rename(&self.temp, &self.target).map_err(|error| failed(&self.target, error))?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        if !self.committed {
            // nothing can be done about a file that is already gone or cannot be removed: the render's own error is the answer
            let _ = std::fs::remove_file(&self.temp);
        }
    }
}

/// An 8-bit RGBA PNG written band by band.
pub(crate) struct PngStream {
    sink: png::StreamWriter<'static, BufWriter<File>>,
    part: Part,
    /// The bytes of the band being written, kept from band to band.
    bytes: Vec<u8>,
}

impl PngStream {
    /// A PNG of `columns` by `rows` pixels at `path`.
    ///
    /// # Errors
    /// [`Error::Write`] naming `path` for a path that cannot be written.
    pub(crate) fn create(path: &Path, columns: usize, rows: usize, dpi: Option<f64>) -> Result<Self, Error> {
        let side = |count: usize| u32::try_from(count).map_err(|error| failed(path, error));
        let (part, file) = Part::create(path)?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), side(columns)?, side(rows)?);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Balanced);
        encoder.set_filter(png::Filter::Adaptive);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        if let Some(dpi) = dpi {
            let per_metre = pixels_per_metre(dpi);
            encoder.set_pixel_dims(Some(png::PixelDimensions {
                xppu: per_metre,
                yppu: per_metre,
                unit: png::Unit::Meter,
            }));
        }
        let sink = encoder
            .write_header()
            .and_then(png::Writer::into_stream_writer)
            .map_err(|error| failed(path, error))?;
        Ok(Self {
            sink,
            part,
            bytes: Vec::new(),
        })
    }

    /// The next band of rows.
    ///
    /// # Errors
    /// [`Error::Write`] naming the file if it cannot be written.
    pub(crate) fn push(&mut self, band: &Raster) -> Result<(), Error> {
        self.bytes.clear();
        self.bytes.extend(band.pixels().iter().map(|&level| eight_bit(level)));
        self.sink
            .write_all(&self.bytes)
            .map_err(|error| failed(&self.part.target, error))
    }

    /// The end of the picture: the file is complete and in place.
    ///
    /// # Errors
    /// [`Error::Write`] naming the file if it cannot be finished (too few rows were written).
    pub(crate) fn finish(self) -> Result<(), Error> {
        let Self { sink, part, .. } = self;
        sink.finish().map_err(|error| failed(&part.target, error))?;
        part.commit()
    }
}
