"""A picture or map array of any layout, dtype or channel count reaches the render as the same pixels, and a bad value
is refused with the same words. Characterisation of the array-to-raster conversion, through the public interface."""

import numpy as np
import pytest

from bullterrier_scene import InvalidError, Layer, Scene

HEIGHT, WIDTH = 6, 8


def drawn(array: np.ndarray) -> np.ndarray:
    return Scene(WIDTH, HEIGHT).add(Layer.picture(array)).render(WIDTH)


def levels(shape: tuple[int, ...]) -> np.ndarray:
    return np.random.default_rng(1).integers(0, 256, shape).astype(np.float64) / 255.0


@pytest.mark.parametrize("channels", [3, 4])
def test_every_dtype_and_layout_draws_the_same_pixels(channels):
    base = levels((HEIGHT, WIDTH, channels))
    opaque = base if channels == 4 else np.concatenate([base, np.ones((HEIGHT, WIDTH, 1))], axis=2)
    expected = drawn(opaque.astype(np.float32))
    assert np.allclose(expected[..., :3] * expected[..., 3:], opaque[..., :3] * opaque[..., 3:], atol=1e-5)
    wide = np.zeros((HEIGHT, WIDTH * 2, channels))
    wide[:, ::2] = base  # a view with a stride, not contiguous
    strided = wide[:, ::2]
    assert not strided.flags.c_contiguous
    flipped = base[::-1].copy()[::-1]  # a view of negative stride
    for variant in (
        base.astype(np.float32),
        base,
        np.round(base * 255).astype(np.uint8),
        np.round(base * 65535).astype(np.uint16),
        strided.astype(np.float32),
        strided,
        flipped.astype(np.float32),
    ):
        assert np.allclose(drawn(variant), expected, atol=1e-2 if variant.dtype == np.uint8 else 1e-5)
    assert np.array_equal(drawn(base.astype(np.float32)), drawn(strided.astype(np.float32)))


def test_a_strided_map_is_the_same_map_as_a_contiguous_one():
    grey = levels((HEIGHT, WIDTH)).astype(np.float32)
    wide = np.zeros((HEIGHT, WIDTH * 2), dtype=np.float32)
    wide[:, ::2] = grey

    def masked(values: np.ndarray) -> np.ndarray:
        picture = Layer.picture(np.ones((HEIGHT, WIDTH, 3), dtype=np.float32), maps={"m": values}).mask("m")
        return Scene(WIDTH, HEIGHT).add(Layer.fill("#000000"), picture).render(WIDTH)

    assert np.array_equal(masked(wide[:, ::2]), masked(grey))
    assert np.array_equal(masked(grey.astype(np.float64)), masked(grey))


@pytest.mark.parametrize("dtype", [np.float32, np.float64])
def test_a_value_out_of_range_is_refused_with_its_place(dtype):
    pixels = np.zeros((HEIGHT, WIDTH, 4), dtype=dtype)
    pixels[2, 3, 1] = 1.5
    with pytest.raises(InvalidError, match=r"row 2, column 3 has 1\.5 in channel 1"):
        Layer.picture(pixels)


def test_a_nan_is_refused_with_its_place():
    pixels = np.zeros((HEIGHT, WIDTH, 3), dtype=np.float32)
    pixels[1, 4, 2] = np.nan
    with pytest.raises(InvalidError, match=r"row 1, column 4 has NaN in channel 2"):
        Layer.picture(pixels)


def test_a_shape_that_is_not_a_picture_is_refused():
    with pytest.raises(InvalidError, match=r"\(height, width, 3 or 4\), got shape \[6, 8, 2\]"):
        Layer.picture(np.zeros((HEIGHT, WIDTH, 2), dtype=np.float32))
