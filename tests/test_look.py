"""`poe look`: the example scene it renders, and the PNG it writes."""

import importlib.util
import struct
import zlib
from pathlib import Path
from types import ModuleType

import numpy as np
import pytest

import bullterrier_scene as bts

ROOT = Path(__file__).resolve().parents[1]


def load(name: str, path: Path) -> ModuleType:
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.fixture(scope="module")
def example() -> ModuleType:
    return load("c1_first_picture", ROOT / "docs" / "examples" / "c1_first_picture.py")


@pytest.fixture(scope="module")
def look() -> ModuleType:
    return load("look", ROOT / "tools" / "look.py")


def decode(png: bytes) -> np.ndarray:
    """The RGBA pixels of an 8-bit PNG whose rows are all filter 0, read with the standard library."""
    assert png.startswith(b"\x89PNG\r\n\x1a\n")
    chunks, at = {}, 8
    while at < len(png):
        (length,) = struct.unpack(">I", png[at : at + 4])
        kind, data = png[at + 4 : at + 8], png[at + 8 : at + 8 + length]
        assert struct.unpack(">I", png[at + 8 + length : at + 12 + length]) == (zlib.crc32(kind + data),)
        chunks[kind] = data
        at += 12 + length
    width, height, depth, color = struct.unpack(">IIBB", chunks[b"IHDR"][:10])
    assert (depth, color) == (8, 6)
    rows = np.frombuffer(zlib.decompress(chunks[b"IDAT"]), np.uint8).reshape(height, 1 + width * 4)
    assert not rows[:, 0].any()
    return rows[:, 1:].reshape(height, width, 4)


def test_the_headline_changes_only_the_pixels_it_covers_and_draws_them_in_ink(example):
    photo = example.synthetic_photo()
    with_headline = example.build(photo).render(example.W)
    without = bts.Scene(example.W, example.H).add(bts.Layer.picture(photo).look(example.NIGHT)).render(example.W)
    assert with_headline.shape == (example.H, example.W, 4)
    changed = (with_headline != without).any(axis=-1)
    assert changed.any()
    assert 0 < changed.mean() < 0.2  # lettering, not the picture
    ink = np.array([int(example.INK[at : at + 2], 16) for at in (1, 3, 5)] + [255]) / 255
    assert np.allclose(with_headline[changed], ink, atol=1e-6)
    assert (with_headline[..., 3] == 1).all()


def test_the_example_renders_the_same_array_twice_and_at_other_sizes(example):
    scene = example.build(example.synthetic_photo())
    assert np.array_equal(scene.render(320), scene.render(320))
    assert scene.render(640).shape == (360, 640, 4)


def test_the_photo_is_synthetic_opaque_and_in_range(example):
    photo = example.synthetic_photo()
    assert photo.shape == (720, 1280, 3)
    assert photo.dtype == np.float32
    assert 0 <= photo.min() <= photo.max() <= 1


def test_the_look_task_writes_the_pngs_it_is_given_and_prints_the_first(look, example, tmp_path, capsys):
    pieces = [(64, tmp_path / "deep" / "a.png"), (32, tmp_path / "b.png")]
    assert look.main(pieces) == 0
    assert capsys.readouterr().out.strip() == str(pieces[0][1])
    scene = example.build(example.synthetic_photo())
    for size, path in pieces:
        expected = np.rint(scene.render(size) * 255).astype(np.uint8)
        assert np.array_equal(decode(path.read_bytes()), expected)
