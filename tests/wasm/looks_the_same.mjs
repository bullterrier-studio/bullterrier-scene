// The one tolerance of "looks the same" in this repository, in levels of 8-bit output after rounding: every sample within
// ALL_LEVELS, and all but PAST_MOST_PER_THOUSAND samples in a thousand within MOST_LEVELS (alpha counts as a sample). The
// numbers are those of crates/core/tests/it/common/looks.rs (the Rust source of them); tests/conftest.py states them for Python.
// A mismatch is reported by a count and the first index of the largest gap, never by a diff of the whole buffer.
export const MOST_LEVELS = 1;
export const ALL_LEVELS = 2;
export const PAST_MOST_PER_THOUSAND = 1;

/** Why `actual` does not look like `expected` (two arrays of 8-bit samples), or null if it does. */
export function differenceInLooks(actual, expected) {
  if (actual.length !== expected.length) return `lengths differ: ${actual.length} and ${expected.length}`;
  let pastMost = 0;
  let worst = 0;
  let at = 0;
  for (let index = 0; index < actual.length; index++) {
    const gap = Math.abs(actual[index] - expected[index]);
    if (gap > MOST_LEVELS) pastMost++;
    if (gap > worst) [worst, at] = [gap, index];
  }
  if (worst > ALL_LEVELS || pastMost * 1000 > actual.length * PAST_MOST_PER_THOUSAND) {
    const share = ((100 * pastMost) / actual.length).toFixed(3);
    return `${pastMost} of ${actual.length} samples (${share} %) are more than ${MOST_LEVELS} level off; the largest is ${worst} levels, first at ${at}`;
  }
  return null;
}
