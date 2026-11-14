"""The contract every registered layer kind keeps, run over each one found in the registry (the tests' own two, and any
installed plugin), so a new kind needs no registration here: the picture the scene gets for any rectangle and scale it
asks for has the size asked, values from 0 to 1, no NaN, and is the same twice. The scene itself refuses a kind that
breaks the first rule (tests/test_plugin.py)."""

import math

import numpy as np
import pytest

import bullterrier_scene as bts


def cells(length: float, scale: float) -> int:
    return math.ceil(length * scale * (1 - 1e-12))


def gradient(x, y, width, height, scale_x, scale_y, *, tone=0.5):
    """Red is the tone; green and blue ramp with the design position of the pixel."""
    columns, rows = cells(width, scale_x), cells(height, scale_y)
    pixels = np.empty((rows, columns, 4), dtype=np.float32)
    pixels[..., 0] = tone
    pixels[..., 1] = (((x + np.arange(columns) / scale_x) * 0.01) % 1.0)[None, :]
    pixels[..., 2] = (((y + np.arange(rows) / scale_y) * 0.01) % 1.0)[:, None]
    pixels[..., 3] = 1.0
    return pixels


def flat(x, y, width, height, scale_x, scale_y):
    """Half grey, half transparent."""
    return np.full((cells(height, scale_y), cells(width, scale_x), 4), 0.5, np.float32)


# names that cannot collide with another test's: the registry is process-wide and these stay registered
bts.register_layer("contract-gradient", gradient)
bts.register_layer("contract-flat", flat)
KINDS = bts.layer_kinds()

# the rectangles the scene asks for, by what the public contract reaches: the canvas at a size, a fit, a placement scale
SCENES = {
    "the canvas at the output's size": (lambda layer: layer, 40),
    "a fractional scale": (lambda layer: layer, 37),
    "a tiny output": (lambda layer: layer, 1),
    "a part of the canvas, cover": (lambda layer: layer.fit((3.3, 1.1, 10.7, 4.9)), 61),
    "contain": (lambda layer: layer.fit((0, 0, 17.5, 9.5), "contain"), 77),
    "stretch with fractional ends": (lambda layer: layer.fit((0.25, 0.75, 20.5, 8.5), "stretch"), 53),
    "scaled beyond the canvas": (lambda layer: layer.place(scale=3.0, dx=-20.0), 40),
    "a tiny scale": (lambda layer: layer.place(scale=0.01), 90),
}


def test_the_contract_has_kinds_to_check():
    assert {"contract-gradient", "contract-flat"} <= set(KINDS)


@pytest.mark.parametrize("scene_name", SCENES.keys())
@pytest.mark.parametrize("kind", KINDS)
def test_a_registered_kind_keeps_the_contract(kind, scene_name):
    shape, size = SCENES[scene_name]

    def drawn() -> np.ndarray:
        return bts.Scene(30, 15).add(shape(bts.Layer.make(kind))).render(size)

    picture = drawn()
    assert picture.dtype == np.float32
    assert picture.shape[2] == 4
    assert max(picture.shape[:2]) == size
    assert np.all(np.isfinite(picture)), "a NaN"
    assert picture.min() >= 0.0
    assert picture.max() <= 1.0
    assert picture.tobytes() == drawn().tobytes(), "the same question gave two pictures"
