//! The one tolerance of this repository for "looks the same" between two routes to a picture (threads, bands, cache,
//! device, platform, command line against library), in levels of 8-bit output after rounding: at most `MOST_LEVELS` on all
//! but one sample in a thousand, and at most `ALL_LEVELS` on every sample. Included by path in the command line's tests.

/// See the module.
pub(crate) const MOST_LEVELS: u8 = 1;
/// See the module.
pub(crate) const ALL_LEVELS: u8 = 2;
/// See the module: samples allowed past `MOST_LEVELS`, per thousand.
const PAST_MOST_PER_THOUSAND: usize = 1;

/// `Ok` when the 8-bit samples `got` look the same as `expected`; else what differs: the length, or how many samples are
/// off by more than `MOST_LEVELS`, the largest gap and the first index of it (never a diff of the buffers).
pub(crate) fn levels_look_the_same(got: &[u8], expected: &[u8]) -> Result<(), String> {
    if got.len() != expected.len() {
        return Err(format!("lengths differ: {} and {}", got.len(), expected.len()));
    }
    let (mut past_most, mut worst, mut at) = (0_usize, 0_u8, 0_usize);
    for (index, (g, e)) in got.iter().zip(expected).enumerate() {
        let gap = g.abs_diff(*e);
        past_most += usize::from(gap > MOST_LEVELS);
        if gap > worst {
            (worst, at) = (gap, index);
        }
    }
    if worst > ALL_LEVELS || past_most * 1000 > got.len() * PAST_MOST_PER_THOUSAND {
        return Err(format!(
            "{past_most} of {} samples are more than {MOST_LEVELS} level off; the largest is {worst} levels, first at {at}",
            got.len()
        ));
    }
    Ok(())
}
