//! The feather: a blur of a plane of mask values by three passes of one box, along x then along y.
//!
//! One pass is a box of real half-width `h = n + f` (`n` whole, `f` from 0 to 1): the pixels within `n` of the centre weigh
//! 1, the two at `n + 1` weigh `f`, all over `2h + 1`. The weights are never negative and sum to 1, so the kernel is convex
//! (a plane of values from 0 to 1 stays so, with no clamp). Three passes have a variance of three times one pass's, and
//! `h` is chosen so that it is sigma squared: the kernel is the Gaussian's in variance, within 0.0099 of its distribution
//! function, and reaches `3 ceil(h)` pixels and no further. The edge of a line is replicated beyond its ends.
//!
//! A window sum is a suffix sum of one block of the line plus a prefix sum of the next (van Herk and Gil-Werman): sums of
//! non-negative numbers only, in `f64`, so a value is never below 0 by the residue of a subtraction and a window of ones
//! sums to exactly its length. Reasons and measurements: `docs/design-decisions.md`.

use crate::Error;
use crate::geometry::MAX_PIXELS;

/// A pass of a box: whole half-width `n` and the weight `f` of the two pixels beyond.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Box1 {
    n: usize,
    f: f64,
}

impl Box1 {
    /// The box whose three passes have the variance `sigma` squared, for a sigma above 0.
    ///
    /// The variance of one pass is `(n (n + 1) (2n + 1) / 3 + 2 f (n + 1)^2) / (2n + 1 + 2f)`; set to `t = sigma^2 / 3` and
    /// solved for `f` at the largest `n` whose box of whole width has a variance `n (n + 1) / 3` of at most `t`.
    fn of(sigma: f64) -> Self {
        let t = sigma * sigma / 3.0;
        // n (n + 1) <= 3 t: the root of the quadratic, rounded down
        let root = ((1.0 + 12.0 * t).sqrt() - 1.0) / 2.0;
        // a sigma past what a count holds saturates: the plane check refuses it before anything is blurred
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a root of a positive number, saturating"
        )]
        let n = root.floor() as usize;
        #[expect(clippy::cast_precision_loss, reason = "a half-width of at most 2^27 pixels is exact")]
        let whole = n as f64;
        let f = (t * (2.0 * whole + 1.0) - whole * (whole + 1.0) * (2.0 * whole + 1.0) / 3.0)
            / (2.0 * (whole + 1.0) * (whole + 1.0) - 2.0 * t);
        Self { n, f }
    }

    /// The pixels the three passes reach to each side: the whole half-width, and one more where the end pixels weigh something.
    fn reach(self) -> usize {
        3usize.saturating_mul(self.n.saturating_add(usize::from(self.f > 0.0)))
    }
}

/// The pixels a blur of `sigma` (above 0) output pixels reaches to each side; 0 for a sigma of 0, which blurs nothing.
pub(crate) fn reach(sigma: f64) -> usize {
    if sigma > 0.0 { Box1::of(sigma).reach() } else { 0 }
}

/// A plane of `columns` by `rows` values (pixels, margin included) with room for the reach of the blur on each side must not
/// pass the pixels a picture may have: refused naming `field`.
///
/// # Errors
/// [`Error::Invalid`] naming `field` for a blur whose working plane, `reach` pixels more on each of the four sides, has
/// more than 2^27 pixels.
pub(crate) fn fits(field: &str, (columns, rows): (usize, usize), reach: (usize, usize)) -> Result<(), Error> {
    let plane = columns
        .saturating_add(reach.0.saturating_mul(2))
        .checked_mul(rows.saturating_add(reach.1.saturating_mul(2)));
    match plane {
        Some(pixels) if pixels <= MAX_PIXELS => Ok(()),
        _ => Err(Error::invalid(
            field,
            format!(
                "a feather that reaches {} x {} pixels past a plane of {columns} x {rows} needs more than {MAX_PIXELS} pixels (2^27): a smaller feather or a smaller size",
                reach.0, reach.1
            ),
        )),
    }
}

/// `values` (`columns` by `rows`, row after row) blurred by `sigma` = (along x, along y) output pixels; a sigma of 0 along an
/// axis leaves that axis as it is. The edge is replicated.
pub(crate) fn blur(values: &mut [f32], (columns, rows): (usize, usize), (sigma_x, sigma_y): (f64, f64)) {
    let mut lines = Lines::default();
    if sigma_x > 0.0 {
        let kernel = Box1::of(sigma_x);
        for row in values.chunks_exact_mut(columns) {
            lines.pass(row.iter().copied(), columns, kernel);
            for (value, blurred) in row.iter_mut().zip(&lines.now) {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a mean of values from 0 to 1 is held as a 32-bit float"
                )]
                {
                    *value = *blurred as f32;
                }
            }
        }
    }
    if sigma_y > 0.0 {
        let kernel = Box1::of(sigma_y);
        for column in 0..columns {
            lines.pass(values.iter().skip(column).step_by(columns).copied(), rows, kernel);
            for (value, blurred) in values.iter_mut().skip(column).step_by(columns).zip(&lines.now) {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a mean of values from 0 to 1 is held as a 32-bit float"
                )]
                {
                    *value = *blurred as f32;
                }
            }
        }
    }
}

/// The working buffers of one line, kept from line to line.
#[derive(Default)]
struct Lines {
    /// The line, then after each pass what the pass made.
    now: Vec<f64>,
    next: Vec<f64>,
    /// The line with its end values repeated `n + 1` times at each end.
    padded: Vec<f64>,
    /// The sum of the block of the padded line up to each pixel, and from each pixel to the block's end.
    prefix: Vec<f64>,
    suffix: Vec<f64>,
}

impl Lines {
    /// `line` (`length` values) through three passes of `kernel`, left in `self.now`. The line is extended by its end values as
    /// far as the three passes reach before the first pass and cut back after the last, so the edge is repeated once, as a
    /// line of the input's, and not again after each pass (a pass makes the end values of the next a blend, and a kernel as
    /// wide as the line would flatten it).
    fn pass(&mut self, line: impl Iterator<Item = f32>, length: usize, kernel: Box1) {
        let reach = kernel.reach();
        self.now.clear();
        self.now.extend(line.map(f64::from));
        debug_assert_eq!(self.now.len(), length);
        let (first, last) = (self.now[0], self.now[length - 1]);
        self.now.splice(0..0, std::iter::repeat_n(first, reach));
        self.now.extend(std::iter::repeat_n(last, reach));
        for _ in 0..3 {
            self.once(kernel);
            std::mem::swap(&mut self.now, &mut self.next);
        }
        self.now.truncate(reach + length);
        self.now.drain(..reach);
    }

    /// One pass of `kernel` over `self.now`, into `self.next`.
    fn once(&mut self, Box1 { n, f }: Box1) {
        let length = self.now.len();
        let edge = n + 1;
        let (first, last) = (self.now[0], self.now[length - 1]);
        self.padded.clear();
        self.padded.resize(edge, first);
        self.padded.extend_from_slice(&self.now);
        self.padded.resize(length + 2 * edge, last);
        // sums over blocks of the window's width
        let width = 2 * n + 1;
        self.prefix.clear();
        self.prefix.reserve(self.padded.len());
        for (at, value) in self.padded.iter().enumerate() {
            let before = if at % width == 0 { 0.0 } else { self.prefix[at - 1] };
            self.prefix.push(before + value);
        }
        self.suffix.clear();
        self.suffix.resize(self.padded.len(), 0.0);
        for at in (0..self.padded.len()).rev() {
            let after = if at % width == width - 1 || at + 1 == self.padded.len() {
                0.0
            } else {
                self.suffix[at + 1]
            };
            self.suffix[at] = after + self.padded[at];
        }
        #[expect(clippy::cast_precision_loss, reason = "a half-width of at most 2^27 pixels is exact")]
        let total = (width as f64) + 2.0 * f;
        self.next.clear();
        for at in 0..length {
            // the window is padded[at + 1 ..= at + 2n + 1]
            let start = at + 1;
            let window = if start % width == 0 {
                self.suffix[start]
            } else {
                self.suffix[start] + self.prefix[start + width - 1]
            };
            self.next
                .push((window + f * (self.padded[at] + self.padded[at + 2 * n + 2])) / total);
        }
    }
}

// Inline, because no public route reaches the kernel's own numbers: its variance and its reach (a mask's feather reaches them
// only through the blurred values the render tests compare with the Gaussian's distribution function).
#[cfg(test)]
mod tests {
    use super::{Box1, blur, fits, reach};

    /// The variance of the kernel of three passes, from the weights themselves.
    fn variance_of_three_passes(kernel: Box1) -> f64 {
        let n = i32::try_from(kernel.n).unwrap_or(0);
        let one: Vec<(i32, f64)> = (-n - 1..=n + 1)
            .map(|at| (at, if at.abs() <= n { 1.0 } else { kernel.f }))
            .collect();
        let total: f64 = one.iter().map(|(_, weight)| weight).sum();
        let mean_square: f64 = one
            .iter()
            .map(|(at, weight)| weight * f64::from(*at).powi(2))
            .sum::<f64>()
            / total;
        3.0 * mean_square
    }

    #[test]
    fn the_kernel_has_the_variance_asked_for_at_any_sigma() {
        for sigma in [0.01, 0.3, 1.0, 2.0, 2.5, 3.0, 7.7, 30.0, 300.0, 1900.0] {
            let kernel = Box1::of(sigma);
            assert!((0.0..1.0).contains(&kernel.f), "{sigma}: f is {}", kernel.f);
            let got = variance_of_three_passes(kernel);
            assert!(
                (got - sigma * sigma).abs() <= 1e-9 * sigma * sigma,
                "sigma {sigma}: variance {got} is not {}",
                sigma * sigma
            );
        }
    }

    #[test]
    fn the_reach_is_what_the_three_passes_touch() {
        assert_eq!(reach(0.0), 0);
        // sigma 3: t = 3, n = 2 (2 * 3 = 6 <= 9), f > 0, so each pass reaches 3 and the three reach 9
        assert_eq!(reach(3.0), 9);
    }

    #[test]
    fn a_plane_may_have_2_pow_27_pixels_with_the_reach_and_not_one_more() {
        // 16,384 by 8,192 is 2^27: a plane of 16,380 by 8,188 with a reach of 2 each side is exactly that
        assert!(fits("feather", (16_380, 8_188), (2, 2)).is_ok());
        assert!(fits("feather", (16_380, 8_189), (2, 2)).is_err());
        assert!(fits("feather", (usize::MAX, 1), (1, 0)).is_err());
    }

    #[test]
    fn a_line_of_ones_stays_exactly_one_and_a_line_of_zeros_exactly_zero() {
        for sigma in [0.4, 2.0, 9.3, 40.0] {
            let mut ones = vec![1.0_f32; 7 * 5];
            blur(&mut ones, (7, 5), (sigma, sigma));
            assert!(
                ones.iter().all(|value| value.to_bits() == 1.0_f32.to_bits()),
                "ones at {sigma}"
            );
            let mut zeros = vec![0.0_f32; 7 * 5];
            blur(&mut zeros, (7, 5), (sigma, sigma));
            assert!(zeros.iter().all(|value| value.to_bits() == 0), "zeros at {sigma}");
        }
    }
}
