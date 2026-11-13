"""`bullterrier_scene.Layer` through the package's public interface: the sources it takes, in the forms a caller holds
them, and what it refuses. A look or a drawing arrives as serialised data; the real image and vector packages are used
where they are importable, and the tests of the extension itself need neither (a plain object with `to_json` or
`to_dict` stands for the sibling's object, which is all the package asks of it)."""

import json
import struct
import zlib
from pathlib import Path

import numpy as np
import pytest

from bullterrier_scene import FormatError, InvalidError, Layer, ReadError, Scene

PLUS_ONE_STOP = {"steps": [{"effect": "grade", "params": {"exposure": 1.0}, "opacity": 1.0, "blend_mode": "normal"}]}
RECTANGLE = (
    '{"root":[{"stack":{"items":[[{"path":"M1 1 L3 1 L3 3 L1 3 Z"}]],"size":[4,4]}},'
    '{"fill":{"color":"#ff0000"}}],"version":1}'
)
RED_SQUARE = np.zeros((4, 4, 4), np.float32)
RED_SQUARE[1:3, 1:3] = [1, 0, 0, 1]


def render(layer: Layer, canvas: int = 4) -> np.ndarray:
    return Scene(canvas, canvas).add(layer).render(canvas)


class Sibling:
    """What the package asks of an image `Recipe` or a vector `Drawing`: a serialised form, nothing else."""

    def __init__(self, dict_form=None, text=None):
        self.dict_form, self.text = dict_form, text

    def to_dict(self):
        return self.dict_form

    def to_json(self):
        return self.text


def png(path: Path, pixels: np.ndarray) -> Path:
    """An 8-bit PNG of `pixels` (uint8, height, width, 3 or 4), written with the standard library."""
    height, width, channels = pixels.shape

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    rows = b"".join(b"\x00" + row.tobytes() for row in pixels)
    header = struct.pack(">IIBBBBB", width, height, 8, 2 if channels == 3 else 6, 0, 0, 0)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")
    )
    return path


# --- fill, names, opacity --------------------------------------------------------------------------------------


@pytest.mark.parametrize("color", ["#ff00aa", "#FF00AAcc", "#000000"])
def test_a_colour_is_hash_and_six_or_eight_hex_digits(color):
    assert render(Layer.fill(color), 1).shape == (1, 1, 4)


@pytest.mark.parametrize("color", ["ff00aa", "#fff", "#ggg000", "red", ""])
def test_any_other_colour_is_refused_naming_fill(color):
    with pytest.raises(InvalidError, match="fill"):
        Layer.fill(color)


def test_a_colour_that_is_no_text_is_a_type_error():
    with pytest.raises(TypeError):
        Layer.fill(0xFF0000)  # type: ignore[arg-type]  # a wrong type, on purpose


@pytest.mark.parametrize("name", ["photo", "Title_2", "a"])
def test_a_name_is_letters_digits_and_underscores_starting_with_a_letter(name):
    assert Layer.fill("#000000").named(name) is not None


@pytest.mark.parametrize("name", ["2photo", "_photo", "my photo", "", "a.b"])
def test_any_other_name_is_refused_naming_name(name):
    with pytest.raises(InvalidError, match="name"):
        Layer.fill("#000000").named(name)


@pytest.mark.parametrize("value", [-0.1, 1.1, float("nan"), float("inf")])
def test_an_opacity_outside_0_to_1_is_refused_naming_opacity(value):
    with pytest.raises(InvalidError, match="opacity"):
        Layer.fill("#000000").opacity(value)


def test_opacity_takes_a_whole_number_as_a_float():
    assert np.array_equal(render(Layer.fill("#ffffff").opacity(1), 1), [[[1, 1, 1, 1]]])
    assert render(Layer.fill("#ffffff").opacity(0), 1)[0, 0, 3] == 0


def test_a_call_returns_a_new_layer_and_the_old_one_keeps_what_it_had():
    base = Layer.fill("#000000")
    named = base.named("x")
    Scene(2, 2).add(base, base)  # an unnamed layer can be used twice: its names are layer1 and layer2
    with pytest.raises(InvalidError, match="x"):
        Scene(2, 2).add(named, named)


# --- picture ---------------------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("array", "expected"),
    [
        pytest.param(np.array([[[255, 128, 0]]], np.uint8), [1, 128 / 255, 0, 1], id="uint8 rgb is opaque"),
        pytest.param(np.array([[[255, 128, 0, 51]]], np.uint8), [1, 128 / 255, 0, 51 / 255], id="uint8 rgba"),
        pytest.param(np.array([[[65535, 32768, 0]]], np.uint16), [1, 32768 / 65535, 0, 1], id="uint16"),
        pytest.param(np.array([[[1.0, 0.25, 0.0, 0.5]]], np.float32), [1, 0.25, 0, 0.5], id="float32"),
        pytest.param(np.array([[[1.0, 0.25, 0.0]]], np.float64), [1, 0.25, 0, 1], id="float64"),
    ],
)
def test_a_picture_array_of_any_supported_type_is_straight_rgba_from_0_to_1(array, expected):
    assert np.allclose(render(Layer.picture(array), 1)[0, 0], expected, atol=1e-6)


def test_a_picture_array_that_is_not_contiguous_reads_as_it_looks():
    flat = np.arange(8 * 8 * 3, dtype=np.uint8).reshape(8, 8, 3)
    view = flat[::2, ::2]
    assert not view.flags.c_contiguous
    assert np.array_equal(render(Layer.picture(view), 4), render(Layer.picture(np.ascontiguousarray(view)), 4))


@pytest.mark.parametrize(
    "array",
    [
        pytest.param(np.zeros((4, 4), np.float32), id="two dimensions"),
        pytest.param(np.zeros((4, 4, 2), np.float32), id="two channels"),
        pytest.param(np.zeros((4, 4, 5), np.float32), id="five channels"),
        pytest.param(np.zeros((0, 4, 3), np.float32), id="no rows"),
        pytest.param(np.full((1, 1, 3), 1.5, np.float32), id="a value above 1"),
        pytest.param(np.full((1, 1, 3), np.nan, np.float32), id="a NaN"),
    ],
)
def test_a_picture_array_of_another_shape_or_with_a_value_outside_0_to_1_is_refused(array):
    with pytest.raises(InvalidError, match="pixels"):
        Layer.picture(array)


@pytest.mark.parametrize("source", ["photo.png", [[[0, 0, 0]]], np.zeros((1, 1, 3), np.int64), None, 3])
def test_a_picture_of_another_type_is_a_type_error_and_a_text_names_the_fix(source):
    with pytest.raises(TypeError, match=r"the text.*Path" if isinstance(source, str) else None):
        Layer.picture(source)


@pytest.mark.parametrize("dtype", [">f4", ">u2", ">f8"])
def test_an_array_of_the_other_byte_order_is_refused_naming_the_byte_order_and_the_fix(dtype):
    with pytest.raises(TypeError, match=r"byte order.*astype\(array\.dtype\.newbyteorder\(\"=\"\)\)"):
        Layer.picture(np.zeros((1, 1, 3), dtype))


@pytest.mark.parametrize("dtype", [np.int64, np.bool_, np.float16, np.complex64])
def test_an_array_of_an_unsupported_type_is_refused_listing_the_supported_ones(dtype):
    with pytest.raises(TypeError, match=r"uint8, uint16, float32 or float64.*got"):
        Layer.picture(np.zeros((1, 1, 3), dtype))


def test_a_picture_file_draws_what_the_same_array_draws(tmp_path):
    pixels = np.array([[[255, 0, 0, 255], [0, 255, 0, 255]], [[0, 0, 255, 255], [255, 255, 255, 128]]], np.uint8)
    from_file = Scene(2, 2).add(Layer.picture(png(tmp_path / "photo.png", pixels))).render(4)
    from_array = Scene(2, 2).add(Layer.picture(pixels)).render(4)
    assert np.allclose(from_file, from_array, atol=1e-6)


def test_a_picture_file_with_three_channels_is_opaque_and_keeps_its_exact_colour(tmp_path):
    pixels = np.array([[[255, 0, 0], [0, 128, 0]], [[0, 0, 255], [51, 102, 153]]], np.uint8)
    drawn = Scene(2, 2).add(Layer.picture(png(tmp_path / "photo.png", pixels))).render(2)
    assert np.array_equal(drawn[..., 3], np.ones((2, 2), np.float32))
    assert np.array_equal(drawn[..., :3], pixels.astype(np.float32) / 255)


def test_a_picture_file_that_cannot_be_read_is_a_read_error_naming_the_layer_and_the_file(tmp_path):
    scene = Scene(2, 2).add(Layer.picture(tmp_path / "missing.png").named("photo"))
    with pytest.raises(ReadError, match=r"photo.*missing\.png") as raised:
        scene.render(2)
    assert isinstance(raised.value, OSError)


# --- drawing ---------------------------------------------------------------------------------------------------


def test_a_drawing_is_taken_as_an_object_with_to_json_its_text_its_dict_or_a_file(tmp_path):
    (tmp_path / "square.json").write_text(RECTANGLE, encoding="utf-8")
    forms = [Sibling(text=RECTANGLE), RECTANGLE, json.loads(RECTANGLE), tmp_path / "square.json"]
    for form in forms:
        assert np.array_equal(render(Layer.drawing(form)), RED_SQUARE), type(form).__name__


def test_a_drawing_file_that_cannot_be_read_is_a_read_error_at_the_call(tmp_path):
    with pytest.raises(ReadError, match=r"drawing.*missing\.json"):
        Layer.drawing(tmp_path / "missing.json")


@pytest.mark.parametrize("source", [3, None, object(), [1], b"{}"])
def test_a_drawing_that_is_none_of_these_is_a_type_error_naming_the_fix(source):
    with pytest.raises(TypeError, match="Drawing"):
        Layer.drawing(source)


def test_a_drawing_without_a_frame_is_an_invalid_error_naming_the_frame():
    unframed = '{"version":1,"root":[{"path":"M0 0 L1 0 L1 1 Z"},{"fill":{"color":"#ff0000"}}]}'
    with pytest.raises(InvalidError, match="frame"):
        Layer.drawing(unframed)


@pytest.mark.parametrize(
    ("text", "named"),
    [("{", "/drawing"), (RECTANGLE.replace('"version":1', '"version":2'), "version")],
    ids=["not JSON", "a version it does not read"],
)
def test_a_document_vector_cannot_read_is_a_format_error_by_vectors_words(text, named):
    with pytest.raises(FormatError, match=named) as raised:
        Layer.drawing(text)
    assert isinstance(raised.value, ValueError)


def test_a_dict_that_is_not_plain_data_is_an_invalid_error():
    with pytest.raises(InvalidError, match="drawing"):
        Layer.drawing({"version": object()})


# --- look ------------------------------------------------------------------------------------------------------


def grey(level: float) -> Layer:
    return Layer.picture(np.full((2, 2, 3), level, np.float32))


def test_a_look_is_taken_as_a_dict_an_object_with_to_dict_or_a_recipe_file(tmp_path):
    (tmp_path / "stop.toml").write_text('[[step]]\neffect = "grade"\nexposure = 1\n', encoding="utf-8")
    for form in [PLUS_ONE_STOP, Sibling(dict_form=PLUS_ONE_STOP), tmp_path / "stop.toml"]:
        picture = render(grey(0.25).look(form), 2)
        assert np.allclose(picture, [0.5, 0.5, 0.5, 1.0], atol=1e-6), type(form).__name__


def test_looks_run_in_the_order_given():
    flat = {"steps": [{"effect": "grade", "params": {"contrast": 0.0}}]}
    assert np.allclose(render(grey(0.25).look(PLUS_ONE_STOP).look(flat), 2)[..., :3], 0.5, atol=1e-6)
    assert np.allclose(render(grey(0.25).look(flat).look(PLUS_ONE_STOP), 2)[..., :3], 1.0, atol=1e-6)


def test_a_look_leaves_the_layer_it_was_given_to_as_it_was():
    base = grey(0.25)
    base.look(PLUS_ONE_STOP)
    assert np.allclose(render(base, 2), [0.25, 0.25, 0.25, 1.0], atol=1e-6)


@pytest.mark.parametrize("recipe", ["recipe.toml", 3, None, object(), ["grade"]])
def test_a_look_of_another_type_is_a_type_error_and_a_text_names_the_fix(recipe):
    with pytest.raises(TypeError, match=r"the text.*Path" if isinstance(recipe, str) else None):
        grey(0.5).look(recipe)


@pytest.mark.parametrize(
    ("recipe", "named"),
    [
        pytest.param({"steps": [{"effect": "glitterbomb"}]}, "glitterbomb", id="an unknown effect"),
        pytest.param(
            {"steps": [{"effect": "grade", "params": {"exposure": 99}}]}, "exposure", id="a value out of range"
        ),
        pytest.param({"steps": [{"effect": "grade"}], "size": 100}, "size", id="a size of its own"),
        pytest.param({"steps": [{"effect": "grade"}], "frame": {"aspect": 1.0}}, "frame", id="a frame of its own"),
        pytest.param({"steps": [{"effect": "grade", "params": {"exposure": float("nan")}}]}, "look", id="a NaN"),
        pytest.param({"steps": [{"effect": "grade", "params": {"exposure": object()}}]}, "look", id="an object"),
    ],
)
def test_a_look_image_refuses_is_an_invalid_error_at_the_call_by_images_words(recipe, named):
    with pytest.raises(InvalidError, match=named):
        grey(0.5).look(recipe)


def test_a_recipe_format_the_linked_image_crate_does_not_read_is_a_format_error_naming_both():
    with pytest.raises(FormatError, match=r"format 4.*1, 2, 3"):
        grey(0.5).look({"format": 4, "steps": [{"effect": "grade"}]})
    assert grey(0.5).look({"format": 3, "steps": [{"effect": "grade"}]}) is not None


def test_a_recipe_file_that_cannot_be_read_is_a_read_error_at_the_call(tmp_path):
    with pytest.raises(ReadError, match=r"missing\.toml"):
        grey(0.5).look(tmp_path / "missing.toml")


def test_a_look_on_a_fill_or_a_drawing_is_refused_until_the_batch_that_brings_it():
    for layer in (Layer.fill("#ff0000"), Layer.drawing(RECTANGLE)):
        with pytest.raises(InvalidError, match="picture"):
            layer.look(PLUS_ONE_STOP)


def test_a_look_that_needs_a_map_fails_at_render_naming_the_layer_and_the_map():
    masked = {"steps": [{"effect": "grade", "params": {"exposure": 1.0}, "mask": "subject"}]}
    scene = Scene(2, 2).add(grey(0.5).look(masked).named("photo"))
    with pytest.raises(InvalidError, match=r"photo.*subject"):
        scene.render(2)


# --- the real sibling packages, where they are installed -------------------------------------------------------


def test_a_real_image_recipe_is_taken_by_its_to_dict():
    fx = pytest.importorskip("bullterrier_image").fx
    recipe = pytest.importorskip("bullterrier_image").Recipe([fx.grade(exposure=1.0)])
    assert np.allclose(render(grey(0.25).look(recipe), 2), [0.5, 0.5, 0.5, 1.0], atol=1e-6)


def test_a_real_vector_drawing_is_taken_by_its_to_json():
    vector = pytest.importorskip("bullterrier_vector")
    drawing = vector.Drawing(vector.Path.rect(1, 1, 2, 2), size=(4, 4)).fill("#ff0000")
    assert np.array_equal(render(Layer.drawing(drawing)), RED_SQUARE)


def test_a_look_that_overshoots_images_nominal_range_by_rounding_gives_a_picture_a_layer_takes_back():
    scanlines = {
        "steps": [
            {
                "effect": "scanlines",
                "params": {"spacing": 2000, "thickness": 1, "darkness": 1, "offset": 2000, "softness": 1},
                "opacity": 1.0,
                "blend_mode": "normal",
            }
        ]
    }
    rows, columns = np.indices((24, 32))
    checker = np.repeat(((rows + columns) % 2 == 0).astype(np.float32)[..., None], 3, axis=2)
    drawn = Scene(32, 24).add(Layer.picture(checker).look(scanlines)).render(32)
    assert drawn.min() >= 0
    assert drawn.max() <= 1
    assert Layer.picture(drawn) is not None
