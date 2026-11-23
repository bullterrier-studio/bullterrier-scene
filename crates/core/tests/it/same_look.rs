//! The tolerance helper that every route compare goes through: a picture off by 3/255 on one sample fails, off by 1/255
//! on a few passes.

use crate::common::looks_the_same;

/// A picture of `samples` grey levels, every sample `level` of 255.
fn grey(samples: usize, level: f32) -> Vec<f32> {
    vec![level / 255.0; samples]
}

#[test]
fn a_sample_three_levels_off_fails_even_when_all_others_agree() {
    let mut off = grey(10_000, 100.0);
    off[7] = 103.0 / 255.0;
    let verdict = looks_the_same(&off, &grey(10_000, 100.0));
    assert!(verdict.is_err_and(|why| why.contains("first at 7")));
}

#[test]
fn a_few_samples_one_level_off_pass() {
    let mut off = grey(10_000, 100.0);
    for sample in off.iter_mut().take(10) {
        *sample = 101.0 / 255.0;
    }
    assert!(looks_the_same(&off, &grey(10_000, 100.0)).is_ok());
}

#[test]
fn two_levels_off_on_a_few_samples_pass_and_on_many_fail() {
    let few = [grey(9_000, 100.0), grey(1_000, 102.0)].concat();
    assert!(
        looks_the_same(&few, &grey(10_000, 100.0)).is_err(),
        "10% of the samples is not a few"
    );
    let one = [grey(9_999, 100.0), grey(1, 102.0)].concat();
    assert!(looks_the_same(&one, &grey(10_000, 100.0)).is_ok());
}

#[test]
fn a_different_length_and_a_value_that_is_not_a_number_fail() {
    assert!(looks_the_same(&grey(3, 1.0), &grey(4, 1.0)).is_err());
    assert!(looks_the_same(&[f32::NAN], &[0.0]).is_err());
}
