//! A quadrilateral the fitted box is put on, and the homography that does it.

use crate::Error;
use crate::placement::TRAVEL;

/// Four corners on the canvas, in design pixels, the places the fitted box's top left, top right, bottom right and bottom left
/// land on. A homography maps a rectangle to a strictly convex quadrilateral and to nothing else, so that is the rule: the
/// cross products of consecutive edges are all above 0 or all below 0. A quadrilateral that turns the other way (counter-
/// clockwise on the canvas) is accepted: it is the box seen from behind, a mirror.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Quad {
    corners: [[f64; 2]; 4],
}

impl Quad {
    /// The quadrilateral of `corners`, checked.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming `corners` for a number that is not finite or is beyond the range of a move, and for corners
    /// that are not a strictly convex quadrilateral: a dart (one corner inside the others), a bowtie (two edges that
    /// cross), three corners on a line, a corner twice.
    pub(crate) fn new(corners: [[f64; 2]; 4]) -> Result<Self, Error> {
        for [x, y] in corners {
            TRAVEL.check("corners", x)?;
            TRAVEL.check("corners", y)?;
        }
        let turn = |at: usize| {
            let (from, to, past) = (corners[at], corners[(at + 1) % 4], corners[(at + 2) % 4]);
            (to[0] - from[0]) * (past[1] - to[1]) - (to[1] - from[1]) * (past[0] - to[0])
        };
        let turns = [turn(0), turn(1), turn(2), turn(3)];
        if !(turns.iter().all(|&cross| cross > 0.0) || turns.iter().all(|&cross| cross < 0.0)) {
            return Err(Error::invalid(
                "corners",
                format!(
                    "corners (top left, top right, bottom right, bottom left) make a strictly convex quadrilateral: no corner inside the others (a dart), no edges that cross (a bowtie), no three on a line, no corner twice; got {corners:?}"
                ),
            ));
        }
        Ok(Self { corners })
    }

    /// The homography from the unit square, `(u, v)` with `u` along the top edge and `v` down the left, to the quadrilateral,
    /// as the matrix of `(u, v, 1) -> (x w, y w, w)`: its last entry is 1, and the entries of the last row are 0 exactly when
    /// the quadrilateral is a parallelogram (an affine map, which takes the exact path of the warp). Closed form: the
    /// quadrilateral's corners sum `(x0 - x1 + x2 - x3, y0 - y1 + y2 - y3)` is 0 for a parallelogram.
    pub(crate) fn matrix(&self) -> [[f64; 3]; 3] {
        let [[x0, y0], [x1, y1], [x2, y2], [x3, y3]] = self.corners;
        let (sum_x, sum_y) = (x0 - x1 + x2 - x3, y0 - y1 + y2 - y3);
        if sum_x == 0.0 && sum_y == 0.0 {
            return [[x1 - x0, x3 - x0, x0], [y1 - y0, y3 - y0, y0], [0.0, 0.0, 1.0]];
        }
        let (dx1, dx2, dy1, dy2) = (x1 - x2, x3 - x2, y1 - y2, y3 - y2);
        let determinant = dx1 * dy2 - dx2 * dy1;
        let g = (sum_x * dy2 - dx2 * sum_y) / determinant;
        let h = (dx1 * sum_y - sum_x * dy1) / determinant;
        [
            [x1 - x0 + g * x1, x3 - x0 + h * x3, x0],
            [y1 - y0 + g * y1, y3 - y0 + h * y3, y0],
            [g, h, 1.0],
        ]
    }
}
