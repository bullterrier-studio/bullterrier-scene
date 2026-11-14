"""`Scene.to_json`, `Scene.from_json` and `==`, through the package's public interface.

The oracle is the round trip: every example scene, with its pictures written to files, comes back equal, draws the same
bits and lists the same parameters. The refusals of a document, each with its JSON Pointer, are the table of the core's
own tests (`crates/core/tests/it/saved.rs`); here a few rows show the classes the facade raises.
"""

import hashlib
import importlib.util
import json
import struct
import zlib
from pathlib import Path
from types import ModuleType
from typing import Any

import numpy as np
import pytest

from bullterrier_scene import FormatError, InvalidError, Layer, ReadError, Scene


def _example_scenes() -> ModuleType:
    """tests/example_scenes.py, loaded by its path (tests/ is no package, and the pytest settings are the template's)"""
    spec = importlib.util.spec_from_file_location("example_scenes", Path(__file__).with_name("example_scenes.py"))
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


examples = _example_scenes()
ALL_SCENES, LOGO, load, matte, photo = (
    examples.ALL_SCENES,
    examples.LOGO,
    examples.load,
    examples.matte,
    examples.photo,
)


def png(path: Path, pixels: np.ndarray) -> Path:
    """An 8-bit PNG of `pixels`, float32 0 to 1: (height, width, 3 or 4) is RGB or RGBA, (height, width) is grey."""
    levels = np.rint(np.asarray(pixels, np.float32) * 255).astype(np.uint8)
    height, width = levels.shape[:2]

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    rows = b"".join(b"\x00" + row.tobytes() for row in levels)
    kind = {2: 0, 3: 2, 4: 6}[levels.ndim if levels.ndim == 2 else levels.shape[2]]
    header = struct.pack(">IIBBBBB", width, height, 8, kind, 0, 0, 0)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")
    )
    return path


def on_files(scene: Scene, folder: Path) -> Scene:
    """`scene` with each picture and map it holds in memory put in a file of `folder` (they have no file to name)."""
    held = {
        address: parameter.value
        for address, parameter in scene.parameters().items()
        if parameter.kind == "picture" and parameter.value is not None and not isinstance(parameter.value, Path)
    }
    return scene.with_values(
        {
            address: png(folder / f"{number}.png", np.asarray(value))
            for number, (address, value) in enumerate(held.items())
        }
    )


def drawn(scene: Scene, size: int = 96) -> bytes | str:
    """The picture's bytes, or the refusal (a template's hole is refused alike by a scene and the scene read back)."""
    try:
        return scene.render(size).tobytes()
    except InvalidError as error:
        return str(error)


@pytest.mark.parametrize("name", list(ALL_SCENES))
def test_an_example_scene_comes_back_equal_draws_the_same_and_lists_the_same_parameters(name, tmp_path):
    scene = on_files(ALL_SCENES[name](), tmp_path)
    text = scene.to_json()
    assert text == scene.to_json()
    back = Scene.from_json(text, base=tmp_path)
    assert back == scene
    assert back.to_json() == text
    assert back.parameters() == scene.parameters()
    assert drawn(back) == drawn(scene)


def test_a_template_with_a_hole_comes_back_a_template_and_fills_the_same_way(tmp_path):
    module = load("cover_template")
    template = module.build()
    back = Scene.from_json(template.to_json())
    assert back == template
    assert back.parameters()["photo.picture"].value is None
    values = module.episode("Pilot", png(tmp_path / "a.png", photo(36, 64)), "#2b0a3d")
    assert drawn(back.with_values(values)) == drawn(template.with_values(values))
    assert not isinstance(drawn(back.with_values(values)), str)


def test_a_drawing_keeps_the_params_that_were_set_on_it(tmp_path):
    scene = Scene(100, 100).add(Layer.drawing(LOGO).named("logo"))
    set_ = scene.with_values({"logo.drawing.ring_on": 0.5})
    back = Scene.from_json(set_.to_json())
    assert back == set_
    assert back != scene
    assert back.parameters()["logo.drawing.ring_on"].value == 0.5
    assert drawn(back) == drawn(set_) != drawn(scene)
    assert json.loads(set_.to_json())["layers"][0]["values"] == {"ring_on": 0.5}


def test_the_saved_scene_example_is_equal_after_the_trip_and_draws_the_same_bits(tmp_path):
    scene, loaded = load("c6b_saved_scene").saved_and_loaded(tmp_path, lambda pixels, path: png(path, pixels))
    assert loaded == scene
    assert loaded.render(96).tobytes() == scene.render(96).tobytes()
    assert (tmp_path / "scene.json").is_file()


# --- equality ---------------------------------------------------------------------------------------------------------


def test_equal_scenes_hold_the_same_data_and_a_scene_is_equal_to_no_other_kind_of_thing():
    red = Scene(4, 4).add(Layer.fill("#ff0000").named("a").opacity(0.5))
    assert red == Scene(4, 4).add(Layer.fill("#FF0000").named("a").opacity(0.5))
    for other in (
        Scene(4, 5).add(Layer.fill("#ff0000").named("a").opacity(0.5)),
        Scene(4, 4).add(Layer.fill("#ff0000").named("a").opacity(0.25)),
        Scene(4, 4).add(Layer.fill("#ff0000").named("b").opacity(0.5)),
    ):
        assert red != other
    assert red != red.layers[0]
    assert red != 3
    assert red.layers[0] == Layer.fill("#ff0000").named("a").opacity(0.5)
    assert red.layers[0] != Layer.fill("#ff0000")
    with pytest.raises(TypeError):
        hash(red)


def test_two_pictures_in_memory_are_equal_by_their_pixels():
    pixels = photo(4, 6)
    held = Scene(6, 4).add(Layer.picture(pixels))
    assert held == Scene(6, 4).add(Layer.picture(pixels.copy()))
    assert held != Scene(6, 4).add(Layer.picture(pixels * 0.5))


# --- the document -----------------------------------------------------------------------------------------------------


def test_the_document_is_one_text_with_its_keys_in_the_order_the_contract_lists():
    scene = Scene(24, 16).add(
        Layer.fill("#101018").named("band").fit((0, 8, 24, 8), "stretch").opacity(0.82),
        Layer.text("Hi", font="bricolage_grotesque", color="#ffffff", size=6, axes={"wght": 800})
        .named("title")
        .place(dx=3, dy=9)
        .blend("screen")
        .as_light(),
        Layer.of("band").named("again").place(dx=1),
    )
    assert scene.to_json() == (
        '{"version":1,"canvas":{"width":24,"height":16},"layers":['
        '{"name":"band","fill":"#101018","fit":{"box":[0,8,24,8],"how":"stretch"},"opacity":0.82},'
        '{"name":"title","text":"Hi","font":"bricolage_grotesque","color":"#ffffff",'
        '"options":{"size":6,"axes":{"wght":800}},"place":{"dx":3,"dy":9},"blend":"screen","light":true},'
        '{"name":"again","of":"band","place":{"dx":1}}]}'
    )


def test_a_scene_whose_pictures_are_arrays_has_no_file_to_name():
    scene = Scene(6, 4).add(Layer.picture(photo(4, 6)).named("photo"))
    with pytest.raises(InvalidError, match=r"photo\.picture: an array has no file"):
        scene.to_json()


def test_a_look_is_saved_inline_with_its_format_and_a_look_file_is_written_out_by_the_next_save(tmp_path):
    (tmp_path / "stop.toml").write_text('[[step]]\neffect = "grade"\nexposure = 1\n', encoding="utf-8")
    scene = Scene(4, 4).add(Layer.fill("#808080").look(tmp_path / "stop.toml"))
    look = json.loads(scene.to_json())["layers"][0]["looks"][0]
    assert look["format"] == 1
    assert look["recipe"]["steps"][0]["effect"] == "grade"
    assert Scene.from_json(scene.to_json()) == scene
    by_path = Scene.from_json(
        '{"version":1,"canvas":{"width":4,"height":4},"layers":[{"fill":"#808080","looks":["stop.toml"]}]}',
        base=tmp_path,
    )
    assert by_path == scene
    assert json.loads(by_path.to_json())["layers"][0]["looks"] == [look]


# --- the files --------------------------------------------------------------------------------------------------------


def test_a_relative_path_is_resolved_against_base_and_its_digest_is_checked_when_the_scene_renders(tmp_path):
    file = png(tmp_path / "photo.png", photo(8, 12))
    digest = hashlib.sha256(file.read_bytes()).hexdigest()
    document: dict[str, Any] = {
        "version": 1,
        "canvas": {"width": 12, "height": 8},
        "layers": [{"picture": {"path": "photo.png"}}],
    }
    loaded = Scene.from_json(json.dumps(document), base=tmp_path)  # reads nothing
    assert loaded.render(12).shape == (8, 12, 4)
    assert json.loads(loaded.to_json())["layers"][0]["picture"] == {"path": "photo.png", "sha256": digest}
    document["layers"][0]["picture"]["sha256"] = "0" * 64
    moved = Scene.from_json(json.dumps(document), base=tmp_path)  # a wrong digest is no matter until it renders
    with pytest.raises(ReadError) as error:
        moved.render(12)
    assert "0" * 64 in str(error.value)
    assert digest in str(error.value)
    elsewhere = Scene.from_json(json.dumps(document), base=tmp_path / "nowhere")
    with pytest.raises(ReadError, match=r"nowhere"):
        elsewhere.render(12)
    with pytest.raises(ReadError, match=r"nowhere"):
        elsewhere.to_json()


def test_a_map_file_is_saved_by_its_path_and_read_back_by_it(tmp_path):
    scene = Scene(12, 8).add(
        Layer.picture(png(tmp_path / "p.png", photo(8, 12)), maps={"matte": png(tmp_path / "m.png", matte(8, 12))})
        .named("photo")
        .mask("matte", feather=1, invert=True)
    )
    document = json.loads(scene.to_json())
    assert document["layers"][0]["maps"]["matte"]["path"].endswith("m.png")
    assert document["layers"][0]["masks"] == [{"map": "matte", "feather": 1, "invert": True}]
    assert Scene.from_json(scene.to_json()) == scene


# --- what the facade raises -------------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("document", "pointer", "words"),
    [
        pytest.param(
            '{"version": 2, "canvas": {"width": 1, "height": 1}}', "/version", "reads scene version 1", id="version"
        ),
        pytest.param('{"version": 1}', "", "`canvas` is missing", id="canvas"),
        pytest.param(
            '{"version": 1, "canvas": {"width": 1, "height": 1}, "layers": [{"fill": "#ffffff", "opacity": -1}]}',
            "/layers/0/opacity",
            "0 to 1",
            id="opacity",
        ),
        pytest.param('{"version": 1, "version": 1}', "/version", "written twice", id="duplicate key"),
        pytest.param("[", "", "not JSON", id="not JSON"),
    ],
)
def test_a_document_that_cannot_be_read_is_a_format_error_that_starts_with_the_pointer(document, pointer, words):
    with pytest.raises(FormatError) as error:
        Scene.from_json(document)
    assert isinstance(error.value, ValueError)
    assert str(error.value).startswith(f"{pointer}: " if pointer else "")
    assert words in str(error.value)


def test_the_text_is_text_and_base_is_a_path():
    with pytest.raises(TypeError, match="text"):
        Scene.from_json(b"{}")  # type: ignore[arg-type]
    with pytest.raises(TypeError, match=r"Path\("):
        Scene.from_json("{}", base="folder")  # type: ignore[arg-type]
