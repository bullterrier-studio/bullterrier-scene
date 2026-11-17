"""Plates through the Python facade: the inks of a layer, the plates of a scene, the files they are written to, and the
DPI of a picture file. The files are read back by Pillow, an independent decoder; the overprint rule and the hand-made
shapes are written here, outside the code under test."""

import json
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

import bullterrier_scene as bts
from bullterrier_scene.cli import run

PAPER, PINK, BLUE = (244, 239, 228), (232, 80, 91), (42, 111, 181)
HALFTONE = {
    "steps": [
        {
            "effect": "halftone",
            "params": {
                "separation": "spot",
                "inks": ["#e8505b", "#2a6fb5"],
                "paper": "#f4efe4",
                "cell": 4.0,
                "shape": "dot",
            },
        }
    ]
}


def photo() -> np.ndarray:
    x = np.linspace(0.0, 1.0, 24, dtype=np.float32)[None, :]
    y = np.linspace(0.0, 1.0, 16, dtype=np.float32)[:, None]
    return np.stack([0.35 + 0.5 * x + 0 * y, 0.4 + 0.4 * y + 0 * x, 0.55 + 0.3 * (1 - x) + 0 * y], axis=-1)


def printed() -> bts.Scene:
    return bts.Scene(24, 16).add(bts.Layer.picture(photo()).look(HALFTONE).inks("pink", "blue"))


def two_columns() -> bts.Scene:
    shape = lambda at: bts.Layer.fill("#123456").fit((at, 0, 1, 2), "stretch")  # noqa: E731
    return bts.Scene(4, 2).add(shape(0).inks("black"), shape(3).opacity(0.5).inks("red"))


def test_plates_by_the_overprint_rule_are_the_picture():
    scene = printed()
    plates = scene.plates(24)
    assert (plates.names, plates.width, plates.height) == (("pink", "blue"), 24, 16)
    composite = scene.render(24)
    expected = np.empty((16, 24, 3), dtype=np.float64)
    for channel in range(3):
        pink, blue = PINK[channel] / PAPER[channel], BLUE[channel] / PAPER[channel]
        expected[:, :, channel] = (
            PAPER[channel] / 255 * (1 - plates.plane("pink") * (1 - pink)) * (1 - plates.plane("blue") * (1 - blue))
        )
    assert np.abs(composite[:, :, :3] - expected).max() < 1e-4
    assert 20 < plates.plane("pink").sum() < 24 * 16 - 20


def test_the_inks_of_a_layer_are_saved_with_the_scene_and_a_bad_list_is_refused():
    scene = bts.Scene(4, 4).add(bts.Layer.fill("#102030").inks("pink", "blue"))
    assert json.loads(scene.to_json())["layers"][0]["inks"] == ["pink", "blue"]
    assert scene.to_json() == bts.Scene.from_json(scene.to_json()).to_json()
    with pytest.raises(bts.InvalidError, match="inks"):
        bts.Layer.fill("#ffffff").inks()
    with pytest.raises(bts.InvalidError, match="inks"):
        bts.Layer.fill("#ffffff").inks("a", "a")
    with pytest.raises(TypeError, match="inks"):
        bts.Layer.fill("#ffffff").inks(3)  # type: ignore[arg-type]
    with pytest.raises(bts.InvalidError, match="inks"):
        bts.Scene(4, 4).add(bts.Layer.fill("#ffffff")).plates(4)
    with pytest.raises(bts.InvalidError, match="name"):
        two_columns().plates(4).plane("green")


def test_a_plate_png_is_grey_and_says_its_dpi_and_the_marks_are_the_same_on_every_plate(tmp_path: Path):
    options = bts.PdfOptions(dpi=254.0, bleed_mm=0.2, bleed_fill="mirror", registration_marks=True)
    written = two_columns().plates(4).write_png(tmp_path / "poster", options)
    assert [path.name for path in written] == ["poster-black.png", "poster-red.png"]
    black, red = (Image.open(path) for path in written)
    assert black.mode == "L"
    # 2 pixels of bleed and 80 of margin on every side
    assert black.size == (4 + 2 * 82, 2 + 2 * 82)
    assert black.info["dpi"] == pytest.approx((254.0, 254.0), abs=0.01)
    a, b = np.asarray(black, dtype=int), np.asarray(red, dtype=int)
    inside = (slice(80, -80), slice(80, -80))
    margin = np.ones(a.shape, dtype=bool)
    margin[inside] = False
    assert (a[margin] == b[margin]).all()
    assert (a[margin] < 255).sum() > 100
    # the mirrored bleed, by hand: black in column 0 only, so columns -2..5 read 255 0 0 255 255 255 255 255
    assert a[inside][0].tolist() == [255, 0, 0, 255, 255, 255, 255, 255]
    assert b[inside][0].tolist() == [255, 255, 255, 255, 255, 127, 127, 255]


def test_a_plate_pdf_has_a_page_for_each_ink_named_by_it(tmp_path: Path):
    path = tmp_path / "plates.pdf"
    two_columns().plates(4).write_pdf(path, bts.PdfOptions(dpi=300.0, bleed_mm=3.0, registration_marks=True))
    data = path.read_bytes()
    assert data.startswith(b"%PDF-1.7")
    assert data.count(b"/Type /Page\n") == 2
    assert data.index(b"/P (black)") < data.index(b"/P (red)")
    assert data.count(b"/DeviceGray") == 2


def test_files_the_plates_cannot_make_are_refused_naming_the_argument_and_leave_nothing(tmp_path: Path):
    plates = two_columns().plates(4)
    with pytest.raises(bts.WriteError, match="nowhere"):
        plates.write_png(tmp_path / "nowhere" / "a")
    with pytest.raises(bts.WriteError, match="nowhere"):
        plates.write_pdf(tmp_path / "nowhere" / "a.pdf")
    for what, options, named in [
        ("dpi 0", bts.PdfOptions(dpi=0), "dpi"),
        ("dpi nan", bts.PdfOptions(dpi=float("nan")), "dpi"),
        ("bleed 51", bts.PdfOptions(bleed_mm=51), "bleed_mm"),
        ("bleed -1", bts.PdfOptions(bleed_mm=-1), "bleed_mm"),
        ("fill", bts.PdfOptions(bleed_fill="fold"), "bleed_fill"),  # type: ignore[arg-type]
    ]:
        with pytest.raises(bts.InvalidError, match=named):
            plates.write_png(tmp_path / "a", options)
        with pytest.raises(bts.InvalidError, match=named):
            plates.write_pdf(tmp_path / "a.pdf", options)
        assert not list(tmp_path.iterdir()), what


def test_the_underbase_is_a_white_plate_first_choked_and_knocked_out(tmp_path: Path):
    garment = "#202020"
    scene = bts.Scene(9, 9).add(
        bts.Layer.fill(garment),
        bts.Layer.fill("#ff0000").fit((2, 2, 5, 5), "stretch").inks("red"),
    )
    plates = scene.plates(9, underbase=bts.Underbase(garment, choke=1.0))
    assert plates.names == ("white", "red")
    white = plates.plane("white")
    # the rim of the 5 by 5 box is 1 pixel from bare paper: half; inside it, whole
    want = np.zeros((9, 9), dtype=np.float32)
    want[2:7, 2:7] = 0.5
    want[3:6, 3:6] = 1.0
    assert np.allclose(white, want)
    with pytest.raises(bts.InvalidError, match="choke"):
        scene.plates(9, underbase=bts.Underbase(garment, choke=-1.0))
    with pytest.raises(bts.InvalidError, match="name"):
        scene.plates(9, underbase=bts.Underbase(garment, name="red"))
    # equal plates are equal
    assert plates == scene.plates(9, underbase=bts.Underbase(garment, choke=1.0))


@pytest.mark.parametrize(("name", "dpi"), [("out.png", 300.0), ("out.jpg", 300.0)])
def test_a_picture_file_of_the_command_line_says_its_dpi_to_an_independent_decoder(
    tmp_path: Path, name: str, dpi: float
):
    scene = bts.Scene(8, 6).add(bts.Layer.fill("#3a7bd5"))
    document = tmp_path / "scene.json"
    document.write_text(scene.to_json(), encoding="utf-8")
    out = tmp_path / name
    assert run(["render", str(document), str(out), "--dpi", str(dpi)]) == 0
    with Image.open(out) as image:
        # a PNG holds whole pixels per metre: 300 dpi is 11811 of them, which reads back as 299.9994
        assert image.info["dpi"] == pytest.approx((dpi, dpi), abs=0.01)
        assert image.size == (8, 6)
    plain = tmp_path / ("plain" + out.suffix)
    assert run(["render", str(document), str(plain)]) == 0
    with Image.open(plain) as image:
        assert "dpi" not in image.info
