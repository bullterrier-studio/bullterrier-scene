//! The distance of each pixel of a shape from the pixels outside it: what the underbase's choke is measured with.

/// `count` as a double: a side is far under 2^52.
#[expect(
    clippy::cast_precision_loss,
    reason = "a side of a picture is under 2^27, exact in a double"
)]
pub(crate) fn float(count: usize) -> f64 {
    count as f64
}

/// How far each pixel lies from the nearest bare (not `covered`) pixel centre, in pixels, 0 for a bare pixel; very large where
/// nothing is bare. Exact: the squared distance transform of Felzenszwalb and Huttenlocher (2012), along the columns and then the
/// rows, each line in time linear in its length.
pub(crate) fn distance_to_bare((columns, rows): (usize, usize), covered: &[bool]) -> Vec<f32> {
    /// Stands for no bare pixel in a line: far past any distance in a picture, small enough that its square is a number.
    const FAR: f64 = 1e20;
    let mut squared: Vec<f64> = covered.iter().map(|covered| if *covered { FAR } else { 0.0 }).collect();
    let longest = columns.max(rows);
    let (mut line, mut made) = (vec![0.0; longest], vec![0.0; longest]);
    let (mut parabolas, mut bounds) = (vec![0_usize; longest], vec![0.0; longest + 1]);
    for x in 0..columns {
        for y in 0..rows {
            line[y] = squared[y * columns + x];
        }
        along(&line[..rows], &mut made[..rows], &mut parabolas, &mut bounds);
        for y in 0..rows {
            squared[y * columns + x] = made[y];
        }
    }
    for y in 0..rows {
        along(
            &squared[y * columns..(y + 1) * columns],
            &mut made[..columns],
            &mut parabolas,
            &mut bounds,
        );
        squared[y * columns..(y + 1) * columns].copy_from_slice(&made[..columns]);
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a distance in a picture is far under what a float holds"
    )]
    squared.iter().map(|square| square.sqrt() as f32).collect()
}

/// The lower envelope of the parabolas `(q - p)^2 + from[p]`: at each place `q`, the least of them, into `to`. `parabolas` and
/// `bounds` are room for the envelope (at least `from.len()` and one more).
fn along(from: &[f64], to: &mut [f64], parabolas: &mut [usize], bounds: &mut [f64]) {
    let mut last = 0;
    parabolas[0] = 0;
    bounds[0] = f64::NEG_INFINITY;
    bounds[1] = f64::INFINITY;
    for q in 1..from.len() {
        let at = |place: usize| float(place);
        let mut meet;
        loop {
            let held = parabolas[last];
            meet = ((from[q] + at(q) * at(q)) - (from[held] + at(held) * at(held))) / (2.0 * at(q) - 2.0 * at(held));
            if meet <= bounds[last] {
                last -= 1;
            } else {
                break;
            }
        }
        last += 1;
        parabolas[last] = q;
        bounds[last] = meet;
        bounds[last + 1] = f64::INFINITY;
    }
    let mut at = 0;
    for (q, slot) in to.iter_mut().enumerate() {
        while bounds[at + 1] < float(q) {
            at += 1;
        }
        let held = parabolas[at];
        *slot = (float(q) - float(held)).powi(2) + from[held];
    }
}
