//! The marks of a printed page: crop marks and registration marks, written once. A mark is a polyline in the unit the caller
//! draws in (PDF points for a page, pixels for a plate's raster), computed from the trim box and the bleed, so the marks of a
//! PDF and of a PNG lie at the same millimetres from the trim and a plate has them where every other plate has them.

use std::f64::consts::TAU;

/// Millimetres in an inch (exact, by definition).
pub(crate) const MM_PER_INCH: f64 = 25.4;
/// Points in an inch (the PDF unit, exact by definition).
pub(crate) const POINTS_PER_INCH: f64 = 72.0;
/// Crop marks start this far outside the bleed box, so no mark touches the bleed (common print practice: Adobe's default offset
/// is 3 mm).
const GAP_MM: f64 = 3.0;
/// Each crop mark is this long; a registration mark is as tall, so both fit the same margin.
const LENGTH_MM: f64 = 5.0;
/// What a page grows by on every side to hold the marks, outside the bleed box.
pub(crate) const MARGIN_MM: f64 = GAP_MM + LENGTH_MM;
/// The line width of a mark, in points (Adobe's default).
pub(crate) const WIDTH_POINTS: f64 = 0.25;
/// The radius of a registration mark's circle; its cross reaches to the margin's edges.
const CIRCLE_MM: f64 = 1.5;
/// The corners of a registration mark's circle.
const CIRCLE_CORNERS: usize = 48;

/// Which marks to draw.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Which {
    pub(crate) crop: bool,
    pub(crate) registration: bool,
}

/// The marks of a page whose trim box is `trim` (`[left, bottom, right, top]`, y up) with a bleed of `bleed_mm`, in a unit of
/// which `per_mm` make a millimetre. Crop marks: a horizontal and a vertical line at each corner of the trim, outside the bleed
/// box. Registration marks: a circle crossed by two lines, centred in the margin on the middle of each side of the trim.
pub(crate) fn lines(trim: [f64; 4], bleed_mm: f64, per_mm: f64, which: Which) -> Vec<Vec<[f64; 2]>> {
    let [left, bottom, right, top] = trim;
    let out = |from_mm: f64| (bleed_mm + from_mm) * per_mm;
    let mut marks = Vec::new();
    if which.crop {
        for (edge, outward) in [(left, -1.0), (right, 1.0)] {
            for y in [bottom, top] {
                marks.push(vec![
                    [edge + outward * out(GAP_MM), y],
                    [edge + outward * out(GAP_MM + LENGTH_MM), y],
                ]);
            }
        }
        for (edge, outward) in [(bottom, -1.0), (top, 1.0)] {
            for x in [left, right] {
                marks.push(vec![
                    [x, edge + outward * out(GAP_MM)],
                    [x, edge + outward * out(GAP_MM + LENGTH_MM)],
                ]);
            }
        }
    }
    if which.registration {
        let middle = [(left + right) / 2.0, (bottom + top) / 2.0];
        let centre = GAP_MM + LENGTH_MM / 2.0;
        let half = LENGTH_MM / 2.0 * per_mm;
        let mut centres = Vec::new();
        for outward in [-1.0, 1.0] {
            centres.push([middle[0], if outward < 0.0 { bottom } else { top } + outward * out(centre)]);
            centres.push([if outward < 0.0 { left } else { right } + outward * out(centre), middle[1]]);
        }
        for [x, y] in centres {
            marks.push(vec![[x - half, y], [x + half, y]]);
            marks.push(vec![[x, y - half], [x, y + half]]);
            marks.push(
                (0..=CIRCLE_CORNERS)
                    .map(|corner| {
                        // a corner of the 48-gon, as many as there are in a turn
                        #[expect(clippy::cast_precision_loss, reason = "a corner number is under 49")]
                        let turn = TAU * corner as f64 / CIRCLE_CORNERS as f64;
                        [x + CIRCLE_MM * per_mm * turn.cos(), y + CIRCLE_MM * per_mm * turn.sin()]
                    })
                    .collect(),
            );
        }
    }
    marks
}
