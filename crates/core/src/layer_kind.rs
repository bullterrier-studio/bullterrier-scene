//! A kind of layer the caller supplies: the plugin seam. The scene asks it one question, "draw this rectangle of the canvas at
//! this scale", and joins, places, masks and caches what it answers as it does for any layer.

use serde_json::Value;

use crate::{Error, Raster};

/// The rectangle that is the rows `first..end` of the `total` rows `area` is divided into: what a kind that draws by position
/// is asked for to draw those rows alone.
pub(crate) fn rows_area(area: [f64; 4], total: usize, first: usize, end: usize) -> [f64; 4] {
    // rows of at most 2^27: exact in a float
    #[expect(clippy::cast_precision_loss, reason = "a count of rows of at most 2^27")]
    let (pitch, first, rows) = (area[3] / total as f64, first as f64, (end - first) as f64);
    [area[0], area[1] + first * pitch, area[2], rows * pitch]
}

/// Checks the promise of [`LayerKind::by_position`] for `kind` on `area` at `scale`: the rows of what it draws for the rectangle
/// are, within the one tolerance, what it draws when asked for those rows alone, for bands of one row, of two and of a third of the rows. Run it
/// in the tests of a kind that says yes, on a rectangle and a scale that are not whole numbers.
///
/// # Errors
/// [`Error::Invalid`] naming `by_position`, with the first rows that differ, or what the kind refuses.
pub fn check_by_position(kind: &dyn LayerKind, area: [f64; 4], scale: [f64; 2]) -> Result<(), Error> {
    let whole = kind.draw(area, scale)?;
    let (columns, total) = (whole.width(), whole.height());
    for rows in [1, 2, total.div_ceil(3)] {
        for first in (0..total).step_by(rows) {
            let end = (first + rows).min(total);
            let part = kind.draw(rows_area(area, total, first, end), scale)?;
            let want = &whole.pixels()[first * columns * 4..end * columns * 4];
            if part.width() != columns
                || part.height() != end - first
                || part
                    .pixels()
                    .iter()
                    .map(|value| value.to_bits())
                    .ne(want.iter().map(|value| value.to_bits()))
            {
                return Err(Error::invalid(
                    "by_position",
                    format!(
                        "the kind {:?} drew rows {first} to {end} of {total} differently when asked for them alone: its pixels depend on the rectangle asked",
                        kind.name()
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// A kind of layer that something outside the scene draws (a generator, a simulation, a decoder): give one to
/// [`Layer::make`](crate::Layer::make).
///
/// **Identity.** What a kind draws depends on its [`name`](LayerKind::name), its [`options`](LayerKind::options), the rectangle and the
/// scale, and on nothing else: the same four give the same raster, whatever else has happened (the clock, a counter, a file
/// that changed). That is the promise that lets the scene keep what a kind drew and ask it again only for a rectangle or a
/// scale it has not seen, so a kind with a hidden input must put it in its options. Two kinds with one name and equal options
/// are the same layer: `==` on scenes says so, and so does the cache.
pub trait LayerKind: Send + Sync {
    /// Whether each pixel this kind draws depends on its place on the canvas, the scale and the options alone: the rectangle
    /// asked for only says which pixels. Then the rows of what it draws for a rectangle are what it draws for the rectangle of
    /// those rows (the part of the rectangle that the rows divide it into: [`check_by_position`] checks it), and a render in bands
    /// asks for the rows a band reads instead of the whole rectangle, so the layer is never held whole. A kind whose pixels depend
    /// on the rectangle (a seeded stream that starts at its corner, a statistic of what it draws) must say no, the default: it is
    /// asked once for its rectangle and the bands are cut from what it drew.
    fn by_position(&self) -> bool {
        false
    }

    /// What this kind is called: letters, digits and `_` by convention. Part of a layer's identity.
    fn name(&self) -> &str;

    /// The options of this layer as plain data (an object by convention): everything besides the name that decides what
    /// [`draw`](LayerKind::draw) answers. Part of a layer's identity, and what a saved scene will write.
    fn options(&self) -> Value;

    /// The rectangle `area` = `[x, y, width, height]` of the canvas, in design pixels, drawn at `scale` = `[across, down]`
    /// output pixels per design pixel (the two differ by less than one part in the canvas's long side on its short axis): a
    /// raster of `ceil(width * across)` columns by `ceil(height * down)` rows, straight alpha, values from 0 to 1, whose
    /// pixels divide the rectangle evenly. A product within 1e-12 (relative) of a whole number counts as that number.
    ///
    /// # Errors
    /// What the kind cannot draw; [`Raster::rgba`] refuses a value outside 0 to 1. A raster of another size than the one
    /// asked for is refused by the scene, naming both sizes.
    fn draw(&self, area: [f64; 4], scale: [f64; 2]) -> Result<Raster, Error>;
}
