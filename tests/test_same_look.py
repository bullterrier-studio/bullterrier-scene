"""The compare every route test goes through: off by 3/255 on one sample fails, off by 1/255 on a few passes."""

import numpy as np


def grey(level: float = 100) -> np.ndarray:
    return np.full((100, 100, 4), level / 255, np.float32)


def test_one_sample_three_levels_off_fails(same_look):
    off = grey()
    off[3, 4, 1] = 103 / 255
    assert not same_look(off, grey())


def test_a_few_samples_one_level_off_pass(same_look):
    off = grey()
    off[0, :2, :] = 101 / 255
    assert same_look(off, grey())


def test_two_levels_off_on_one_sample_pass_and_on_many_fail(same_look):
    one = grey()
    one[0, 0, 0] = 102 / 255
    assert same_look(one, grey())
    assert not same_look(grey(102), grey())


def test_another_shape_and_a_value_that_is_not_a_number_fail(same_look):
    assert not same_look(grey()[:50], grey())
    nan = grey()
    nan[0, 0, 0] = np.nan
    assert not same_look(nan, grey())
