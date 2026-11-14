"""Banded rendering (`Scene.render_bands`): the bands put together are the picture `render` makes, bit for bit.

The oracle runs over every example scene (the cover template, the frame sequence, mill, the poster, composed world,
chrome, groups, references, masks of every kind and feather, maps, text, perspective, blends, light with room, looks)
and a scene with a plugin layer, at band heights from 1 to the whole picture and at sizes whose long side is odd and
even."""

import importlib.util
from collections.abc import Callable
from pathlib import Path
from types import ModuleType

import numpy as np
import numpy.typing as npt
import pytest

import bullterrier_scene as bts
from bullterrier_scene import InvalidError, Layer, Scene, plugin


def _example_scenes() -> ModuleType:
    spec = importlib.util.spec_from_file_location("example_scenes", Path(__file__).with_name("example_scenes.py"))
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


examples = _example_scenes()
ALL_SCENES: dict[str, Callable[[], Scene]] = examples.ALL_SCENES

SIZES = (48, 49)


def heights(rows: int) -> list[int]:
    """One, a few, a prime near an eighth, half, all, and some that divide nothing."""
    prime = next(n for n in range(max(rows // 8, 2), rows + 2) if all(n % d for d in range(2, n)))
    return sorted({h for h in (1, 7, prime, rows // 2, rows, rows * 2 // 3 + 1, rows - 1) if 1 <= h <= rows})


def bands(scene: Scene, size: int, band_rows: int) -> tuple[npt.NDArray[np.float32], list[tuple[int, int]]]:
    """The picture made of the bands, and the rows each band was."""
    found: list[npt.NDArray[np.float32]] = []
    rows: list[tuple[int, int]] = []

    def sink(first: int, band: npt.NDArray[np.float32]) -> None:
        rows.append((first, first + band.shape[0]))
        found.append(band.copy())

    scene.render_bands(size, band_rows, sink)
    return np.concatenate(found), rows


def same_bits(left: npt.NDArray[np.float32], right: npt.NDArray[np.float32]) -> bool:
    return left.shape == right.shape and bool(np.array_equal(left.view(np.uint32), right.view(np.uint32)))


def filled(scene: Scene) -> Scene:
    """`scene` with each hole (a picture or a map not given) filled, so that it can be drawn."""
    holes = {
        address: examples.matte(24, 32) if ".map." in address else examples.photo(24, 32)
        for address, parameter in scene.parameters().items()
        if parameter.kind == "picture" and parameter.value is None
    }
    return scene.with_values(holes)


def check(scene: Scene, size: int) -> None:
    whole = scene.render(size)
    for band_rows in heights(whole.shape[0]):
        picture, rows = bands(scene, size, band_rows)
        assert [a for a, _ in rows] == list(range(0, whole.shape[0], band_rows)), (
            "in order, a band starts where one ended"
        )
        assert all(a < b <= a + band_rows for a, b in rows)
        assert rows[-1][1] == whole.shape[0], "the bands reach the last row"
        assert same_bits(picture, whole), f"bands of {band_rows} rows at size {size}"


@pytest.mark.parametrize("size", SIZES)
@pytest.mark.parametrize("name", sorted(ALL_SCENES))
def test_bands_make_the_picture_of_every_example_scene(name: str, size: int) -> None:
    check(filled(ALL_SCENES[name]()), size)


@pytest.mark.parametrize("size", SIZES)
def test_bands_make_the_picture_of_a_scene_with_a_plugin_layer(size: int) -> None:
    with plugin.isolated():

        @bts.layer_kind("bands-gradient")
        def gradient(x, y, width, height, scale_x, scale_y, *, tone=0.5):
            columns, rows = int(np.ceil(width * scale_x * (1 - 1e-12))), int(np.ceil(height * scale_y * (1 - 1e-12)))
            pixels = np.empty((rows, columns, 4), dtype=np.float32)
            pixels[..., 0] = tone
            pixels[..., 1] = (((x + np.arange(columns) / scale_x) * 0.01) % 1.0)[None, :]
            pixels[..., 2] = (((y + np.arange(rows) / scale_y) * 0.01) % 1.0)[:, None]
            pixels[..., 3] = 1.0
            return pixels

        scene = Scene(60, 40).add(
            Layer.fill("#102030"),
            Layer.make("bands-gradient", tone=0.3).place(dx=7.5, dy=3.25, rotate=12),
        )
        check(scene, size)


def test_a_sink_that_raises_ends_the_render_and_its_exception_is_raised() -> None:
    scene = ALL_SCENES["blend_sheet"]()
    seen = []

    def sink(first: int, band: npt.NDArray[np.float32]) -> None:
        seen.append(first)
        if len(seen) == 2:
            raise KeyError("stop here")

    with pytest.raises(KeyError, match="stop here"):
        scene.render_bands(48, 8, sink)
    assert len(seen) == 2


@pytest.mark.parametrize(
    ("size", "band_rows", "named"), [(48, 0, "band_rows"), (48, 10_000, "band_rows"), (0, 8, "size")]
)
def test_a_band_that_is_none_and_a_size_that_is_none_are_refused_by_name(size: int, band_rows: int, named: str) -> None:
    scene = Scene(8, 4).add(Layer.fill("#808080"))
    with pytest.raises(InvalidError, match=named):
        scene.render_bands(size, band_rows, lambda first, band: None)
