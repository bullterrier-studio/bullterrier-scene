"""`bullterrier_scene.scale` through the package's public interface: the Rust core's product, and its refusals as the
package's exceptions."""

import math
from fractions import Fraction

import pytest
from hypothesis import given
from hypothesis import strategies as st

import bullterrier_scene
from bullterrier_scene import BullterrierSceneError, InvalidError, scale

FINITE = st.floats(min_value=-1e150, max_value=1e150, allow_nan=False, allow_infinity=False)


@pytest.mark.parametrize(
    ("value", "factor", "expected"),
    [
        pytest.param(3, 2, 6.0, id="whole numbers work as floats"),
        pytest.param(0.5, 0.5, 0.25, id="a fraction"),
        pytest.param(-4.0, 0.25, -1.0, id="a negative value"),
        pytest.param(1e200, 1e-200, 1.0, id="large times small stays in range"),
    ],
)
def test_scale_gives_the_product(value, factor, expected):
    assert scale(value, factor) == expected


def test_a_zero_result_is_never_negative():
    assert math.copysign(1.0, scale(0.0, -2.0)) == 1.0


@pytest.mark.parametrize(
    ("value", "factor", "named"),
    [
        pytest.param(math.nan, 2.0, "value", id="NaN value"),
        pytest.param(2.0, math.inf, "factor", id="infinite factor"),
        pytest.param(1e200, 1e200, "too large", id="overflow"),
    ],
)
def test_a_refusal_is_an_invalid_error_naming_what_was_wrong(value, factor, named):
    with pytest.raises(InvalidError, match=named):
        scale(value, factor)


def test_a_refusal_can_be_caught_as_the_packages_base_error_and_as_a_value_error():
    with pytest.raises(BullterrierSceneError):
        scale(math.nan, 1.0)
    with pytest.raises(ValueError, match="finite"):
        scale(math.nan, 1.0)


@pytest.mark.parametrize("bad", ["3", None, [1.0]], ids=["text", "None", "list"])
def test_a_wrong_type_is_a_type_error_not_a_library_error(bad):
    with pytest.raises(TypeError):
        scale(bad, 2.0)


@given(value=FINITE, factor=FINITE)
def test_scale_is_the_correctly_rounded_exact_product(value, factor):
    exact = Fraction(value) * Fraction(
        factor
    )  # the oracle: exact rational arithmetic, and Fraction's float() rounds once
    assert scale(value, factor) == float(exact)


def test_the_package_has_a_version():
    assert bullterrier_scene.__version__
