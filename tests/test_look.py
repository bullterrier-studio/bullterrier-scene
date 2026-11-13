"""`poe look`: the example scene it renders, and the PNG it writes."""

import importlib.util
import re
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
    pieces = [
        ("c1_first_picture.py", "first_picture", 64, tmp_path / "deep" / "a.png"),
        ("c1_first_picture.py", "first_picture", 32, tmp_path / "b.png"),
        ("c2_prints.py", "pile_of_prints", 48, tmp_path / "c.png"),
    ]
    assert look.main(pieces) == 0
    assert capsys.readouterr().out.strip() == str(pieces[0][3])
    scene = example.build(example.synthetic_photo())
    for _, _, size, path in pieces[:2]:
        expected = np.rint(scene.render(size) * 255).astype(np.uint8)
        assert np.array_equal(decode(path.read_bytes()), expected)
    assert decode(pieces[2][3].read_bytes()).shape == (30, 48, 4)


def test_the_gallery_page_shows_every_picture_the_task_writes_there_and_no_other(look):
    shown = set(re.findall(r"!\[[^\]]*\]\(([^)]+\.png)\)", (look.GALLERY / "README.md").read_text(encoding="utf-8")))
    written = {path.name for _, _, _, path in look.PIECES if path.parent == look.GALLERY}
    assert shown == written
    assert {path.name for path in look.GALLERY.glob("*.png")} == written  # and each is committed
    assert all((look.EXAMPLES / file).is_file() for file, _, _, _ in look.PIECES)


# --- C2: the pile of prints and the delivery crop ---------------------------------------------------------------------


@pytest.fixture(scope="module")
def prints() -> ModuleType:
    return load("c2_prints", ROOT / "docs" / "examples" / "c2_prints.py")


def test_every_snapshot_is_opaque_in_range_and_different_for_each_palette(prints):
    pictures = [prints.snapshot(60, 40, name) for name in prints.PALETTES]
    for picture in pictures:
        assert picture.shape == (40, 60, 3)
        assert picture.dtype == np.float32
        assert 0 <= picture.min() <= picture.max() <= 1
    assert not np.array_equal(pictures[0], pictures[1])


def test_the_pile_is_the_same_picture_twice_and_at_two_sizes_and_the_table_shows_around_it(prints):
    scene = prints.pile_of_prints()
    first = scene.render(400)
    assert first.shape == (250, 400, 4)
    assert np.array_equal(first, scene.render(400))
    assert (first[..., 3] == 1).all()  # the table is opaque everywhere
    # the corner is the table, far from every print; the middle of a print's photo is not the table
    table = prints.table()
    assert np.allclose(first[0, 0, :3], table[0:3, 0:3].mean(axis=(0, 1)), atol=0.05)
    assert scene.render(200).shape == (125, 200, 4)


def test_a_print_covers_its_frame_with_its_photo_and_a_mirrored_one_is_the_original_flipped(prints):
    # the same photo cut square twice, one mirrored, on a canvas of its own: the mirror is the original left to right
    photo = prints.snapshot(300, 200, "dusk")
    frame = (50, 50, 200, 200)

    def drawn(**place) -> np.ndarray:
        layer = bts.Layer.picture(photo).fit(frame, "cover").place(**place)
        return bts.Scene(300, 300).add(layer).render(300)

    plain, mirrored = drawn(), drawn(flip=(True, False))
    inside = (slice(50, 250), slice(50, 250))
    assert np.allclose(mirrored[inside], plain[inside][:, ::-1], atol=1e-6)
    assert not np.allclose(mirrored[inside], plain[inside], atol=1e-3)
    assert plain[..., 3].sum() == 200 * 200  # exactly the frame: cover fills it and nothing past it


def test_the_delivery_crop_keeps_the_top_the_middle_or_the_bottom_of_the_tall_photo_as_align_says(prints):
    photo = prints.tall_photo()
    scene = prints.delivery_crop(photo)
    assert (scene.width, scene.height) == (3 * 720 + 2 * 60, 1280)
    sheet = scene.render(scene.width)
    # the photo is 900 by 2000 and the frame 720 by 1280: scale 0.8, 1600 rows of which 1280 show; 320 are cut
    for column, first_row in enumerate([0, 160, 320]):
        crop = sheet[:, column * 780 : column * 780 + 720, :3]
        # the band of the frame's row 100..140 shows the photo's rows (first_row + 100 .. 140) / 0.8, averaged
        window = photo[round((first_row + 100) / 0.8) : round((first_row + 140) / 0.8)].mean(axis=(0, 1))
        assert np.allclose(crop[100:140].mean(axis=(0, 1)), window, atol=0.03), column
    assert np.allclose(sheet[:, 720:780, :3], np.array([0x1A, 0x1A, 0x22]) / 255, atol=1e-6)  # the gap shows the wall
