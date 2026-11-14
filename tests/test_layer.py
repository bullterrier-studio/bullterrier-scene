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

from bullterrier_scene import MAX_DEPTH, FormatError, InvalidError, Layer, ReadError, Scene

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


# --- fit and place ---------------------------------------------------------------------------------------------


def numbered(height: int, width: int) -> np.ndarray:
    """An opaque picture whose every pixel differs, in multiples of 1/64 (exact in a float)."""
    index = np.arange(height * width, dtype=np.float32).reshape(height, width)
    return np.stack([index / 64, (index * 3 % 64) / 64, (index * 7 % 64) / 64, np.ones_like(index)], axis=-1)


def canvas_of(layer: Layer, size: int) -> np.ndarray:
    return Scene(size, size).add(layer).render(size)


@pytest.mark.parametrize(
    ("turns", "numpy_turns"),
    [
        pytest.param(90, -1, id="90 is one clockwise quarter"),
        pytest.param(180, 2, id="180"),
        pytest.param(270, 1, id="270"),
    ],
)
def test_a_quarter_turn_is_numpys_rot90_of_the_picture(turns: int, numpy_turns: int):
    picture = numbered(6, 6)
    turned = canvas_of(Layer.picture(picture).place(rotate=turns), 6)
    assert np.array_equal(turned, np.rot90(picture, numpy_turns))


@pytest.mark.parametrize(
    ("flip", "expected"),
    [
        pytest.param((True, False), np.fliplr, id="across x"),
        pytest.param((False, True), np.flipud, id="across y"),
        pytest.param((True, True), lambda picture: np.flipud(np.fliplr(picture)), id="both"),
        pytest.param((False, False), lambda picture: picture, id="neither"),
    ],
)
def test_a_flip_is_numpys_flip(flip, expected):
    picture = numbered(6, 6)
    assert np.array_equal(canvas_of(Layer.picture(picture).place(flip=flip), 6), expected(picture))


def test_a_move_by_whole_pixels_shifts_the_picture_and_leaves_the_rest_clear():
    picture = numbered(4, 4)
    moved = canvas_of(Layer.picture(picture).place(dx=3, dy=2), 8)
    expected = np.zeros((8, 8, 4), np.float32)
    expected[2:6, 3:7] = picture
    assert np.array_equal(moved, expected)


@pytest.mark.parametrize("align", [0.0, 0.5, 1.0])
def test_cover_cuts_the_taller_photo_where_align_says(align: float):
    photo = numbered(10, 4)
    cut = canvas_of(Layer.picture(photo).fit((0, 0, 4, 4), "cover", align=(0.5, align)), 4)
    first = round(6 * align)
    assert np.array_equal(cut, photo[first : first + 4])


def test_contain_places_the_narrower_photo_in_the_slack_by_align():
    photo = numbered(4, 2)
    placed = Scene(8, 4).add(Layer.picture(photo).fit((0, 0, 8, 4), "contain", align=(1.0, 0.0))).render(8)
    expected = np.zeros((4, 8, 4), np.float32)
    expected[:, 6:] = photo
    assert np.array_equal(placed, expected)


def test_fit_and_place_return_new_layers():
    layer = Layer.picture(numbered(4, 4))
    before = canvas_of(layer, 4)
    layer.place(dx=2)
    layer.fit((0, 0, 2, 2), "contain")
    assert np.array_equal(canvas_of(layer, 4), before)


@pytest.mark.parametrize(
    ("call", "field"),
    [
        pytest.param(lambda layer: layer.place(scale=0), "scale", id="scale 0"),
        pytest.param(lambda layer: layer.place(scale=1000.5), "scale", id="scale above 1000"),
        pytest.param(lambda layer: layer.place(aspect=-1), "aspect", id="negative aspect"),
        pytest.param(lambda layer: layer.place(skew=(90, 0)), "skew_x", id="skew x of 90"),
        pytest.param(lambda layer: layer.place(skew=(0, -89.5)), "skew_y", id="skew y below -89"),
        pytest.param(lambda layer: layer.place(dx=float("nan")), "dx", id="NaN dx"),
        pytest.param(lambda layer: layer.place(dy=2e6), "dy", id="dy past 1e6"),
        pytest.param(lambda layer: layer.place(rotate=float("inf")), "rotate", id="infinite rotate"),
        pytest.param(lambda layer: layer.place(anchor=(0.5, 1.5)), "anchor", id="anchor past 1"),
        pytest.param(lambda layer: layer.fit((0, 0, 0, 5)), "box", id="a box of width 0"),
        pytest.param(lambda layer: layer.fit((0, 0, 5, float("nan"))), "box", id="a NaN box"),
        pytest.param(lambda layer: layer.fit((0, 0, 5, 5), "fill"), "how", id="another word"),
        pytest.param(lambda layer: layer.fit((0, 0, 5, 5), align=(1.5, 0)), "align", id="align above 1"),
    ],
)
def test_a_bad_value_is_an_invalid_error_naming_the_field(call, field: str):
    with pytest.raises(InvalidError, match=field):
        call(Layer.fill("#ffffff"))


HUGE = 10**400  # an integer beyond the range of a float


@pytest.mark.parametrize(
    ("call", "argument"),
    [
        pytest.param(lambda layer: layer.place(dx="2"), "dx", id="dx a text"),
        pytest.param(lambda layer: layer.place(dy=[1]), "dy", id="dy a list"),
        pytest.param(lambda layer: layer.place(dx=True), "dx", id="dx a bool"),
        pytest.param(lambda layer: layer.place(scale=False), "scale", id="scale a bool"),
        pytest.param(lambda layer: layer.place(aspect=np.bool_(True)), "aspect", id="aspect a numpy bool"),
        pytest.param(lambda layer: layer.place(rotate="90"), "rotate", id="rotate a text"),
        pytest.param(lambda layer: layer.place(skew=3), "skew", id="skew no pair"),
        pytest.param(lambda layer: layer.place(skew=(1, 2, 3)), "skew", id="skew of three"),
        pytest.param(lambda layer: layer.place(skew=(1, True)), "skew", id="skew with a bool"),
        pytest.param(lambda layer: layer.place(skew="12"), "skew", id="skew a text"),
        pytest.param(lambda layer: layer.place(flip=True), "flip", id="flip no pair"),
        pytest.param(lambda layer: layer.place(flip=(1, 0)), "flip", id="flip of numbers"),
        pytest.param(lambda layer: layer.place(flip=(True,)), "flip", id="flip of one"),
        pytest.param(lambda layer: layer.place(anchor=0.5), "anchor", id="anchor no pair"),
        pytest.param(lambda layer: layer.place(anchor=(0.5, "x")), "anchor", id="anchor with a text"),
        pytest.param(lambda layer: layer.fit((0, 0, 10)), "box", id="box of three"),
        pytest.param(lambda layer: layer.fit((0, 0, 1, 1, 1)), "box", id="box of five"),
        pytest.param(lambda layer: layer.fit(5), "box", id="box a number"),
        pytest.param(lambda layer: layer.fit("0011"), "box", id="box a text"),
        pytest.param(lambda layer: layer.fit((True, False, 1, 1)), "box", id="box of bools"),
        pytest.param(lambda layer: layer.fit((0, 0, 1, 1), None), "how", id="how None"),
        pytest.param(lambda layer: layer.fit((0, 0, 1, 1), 3), "how", id="how a number"),
        pytest.param(lambda layer: layer.fit((0, 0, 1, 1), align=0.5), "align", id="align no pair"),
        pytest.param(lambda layer: layer.fit((0, 0, 1, 1), align=(0.5,)), "align", id="align of one"),
        pytest.param(lambda layer: layer.fit((0, 0, 1, 1), align=(0.5, True)), "align", id="align with a bool"),
    ],
)
def test_a_wrong_shape_or_type_is_a_type_error_naming_the_argument(call, argument: str):
    with pytest.raises(TypeError, match=argument):
        call(Layer.fill("#ffffff"))


@pytest.mark.parametrize(
    ("call", "argument"),
    [
        pytest.param(lambda layer: layer.place(dx=HUGE), "dx", id="dx beyond a float"),
        pytest.param(lambda layer: layer.place(dy=-HUGE), "dy", id="dy beyond a float"),
        pytest.param(lambda layer: layer.place(rotate=HUGE), "rotate", id="rotate beyond a float"),
        pytest.param(lambda layer: layer.place(skew=(0, HUGE)), "skew", id="skew beyond a float"),
        pytest.param(lambda layer: layer.place(anchor=(HUGE, 0)), "anchor", id="anchor beyond a float"),
        pytest.param(lambda layer: layer.fit((0, 0, HUGE, 10)), "box", id="box beyond a float"),
        pytest.param(lambda layer: layer.fit((0, 0, 1, 1), align=(0, HUGE)), "align", id="align beyond a float"),
        pytest.param(lambda layer: layer.place(scale=float("nan")), "scale", id="scale NaN"),
        pytest.param(lambda layer: layer.place(skew=(float("inf"), 0)), "skew", id="skew infinite"),
        pytest.param(lambda layer: layer.fit((0, float("-inf"), 1, 1)), "box", id="box infinite"),
        pytest.param(lambda layer: layer.fit((0, 0, 1, 1), align=(float("nan"), 0)), "align", id="align NaN"),
    ],
)
def test_a_number_beyond_float_range_or_not_finite_is_an_invalid_error_naming_the_argument(call, argument: str):
    with pytest.raises(InvalidError, match=argument):
        call(Layer.fill("#ffffff"))


def test_the_value_an_invalid_error_names_is_shown_short():
    with pytest.raises(InvalidError) as caught:
        Layer.fill("#ffffff").place(dx=HUGE)
    assert len(str(caught.value)) < 200


@pytest.mark.parametrize(
    "call",
    [
        pytest.param(lambda layer: layer.place(dx=np.float32(2), scale=np.int64(2)), id="numpy numbers"),
        pytest.param(lambda layer: layer.place(skew=[1, 2], anchor=np.array([0.5, 0.5])), id="a list and an array"),
        pytest.param(lambda layer: layer.place(flip=(np.bool_(True), False)), id="numpy bools in flip"),
        pytest.param(lambda layer: layer.fit([0, 0, 4, 4], "contain", align=[0, 1]), id="a list box"),
    ],
)
def test_numpy_numbers_lists_and_arrays_are_taken_where_a_number_or_a_pair_is(call):
    call(Layer.fill("#ffffff"))


def test_place_takes_keywords_only():
    with pytest.raises(TypeError, match="positional"):
        Layer.fill("#ffffff").place(2)  # type: ignore[call-arg]  # the point: a positional argument


# --- perspective -----------------------------------------------------------------------------------------------


def test_a_perspective_on_the_box_itself_is_the_picture_and_a_flat_one_is_a_move():
    picture = numbered(4, 4)
    on_itself = canvas_of(Layer.picture(picture).perspective([(0, 0), (4, 0), (4, 4), (0, 4)]), 4)
    assert np.array_equal(on_itself, picture)
    moved = canvas_of(Layer.picture(picture).perspective([(3, 2), (7, 2), (7, 6), (3, 6)]), 8)
    expected = np.zeros((8, 8, 4), np.float32)
    expected[2:6, 3:7] = picture
    assert np.array_equal(moved, expected)


def test_a_mirrored_quadrilateral_is_numpys_flip_of_the_clockwise_one():
    picture = numbered(4, 4)
    mirrored = canvas_of(Layer.picture(picture).perspective([(4, 0), (0, 0), (0, 4), (4, 4)]), 4)
    assert np.array_equal(mirrored, np.fliplr(picture))


def test_a_perspective_with_the_far_side_narrow_leaves_the_corners_of_the_canvas_clear():
    drawn = canvas_of(Layer.picture(numbered(8, 8)).perspective([(10, 2), (22, 2), (30, 30), (2, 30)]), 32)
    assert drawn[0, 0, 3] == 0
    assert drawn[0, 31, 3] == 0
    assert drawn[20, 16, 3] == 1


def test_the_last_of_place_and_perspective_wins():
    picture = numbered(4, 4)
    flat = [(3, 2), (7, 2), (7, 6), (3, 6)]
    by_place = canvas_of(Layer.picture(picture).place(dx=3, dy=2), 8)
    assert np.array_equal(canvas_of(Layer.picture(picture).place(dx=9).perspective(flat), 8), by_place)
    assert np.array_equal(canvas_of(Layer.picture(picture).perspective(flat).place(dx=3, dy=2), 8), by_place)


@pytest.mark.parametrize(
    ("corners", "match"),
    [
        pytest.param([(0, 0), (10, 10), (10, 0), (0, 10)], "corners", id="a bowtie"),
        pytest.param([(0, 0), (10, 0), (2, 2), (0, 10)], "corners", id="a dart"),
        pytest.param([(0, 0), (5, 0), (10, 0), (0, 10)], "corners", id="three on a line"),
        pytest.param([(0, 0), (1e9, 0), (10, 10), (0, 10)], "corners", id="a corner at 1e9"),
        pytest.param([(0, 0), (float("nan"), 0), (10, 10), (0, 10)], "corners", id="a NaN"),
        pytest.param([(0, 0), (HUGE, 0), (10, 10), (0, 10)], "corners", id="an integer beyond a float"),
    ],
)
def test_corners_that_are_no_convex_quadrilateral_are_an_invalid_error_naming_them(corners, match: str):
    with pytest.raises(InvalidError, match=match):
        Layer.fill("#ffffff").perspective(corners)


@pytest.mark.parametrize(
    "corners",
    [
        pytest.param([(0, 0), (1, 0), (1, 1)], id="three corners"),
        pytest.param([(0, 0), (1, 0), (1, 1), (0, 1), (5, 5)], id="five corners"),
        pytest.param([(0, 0), (1, 0), (1, 1), (0,)], id="a corner of one number"),
        pytest.param([(0, 0), (1, 0), (1, 1), 0], id="a corner that is a number"),
        pytest.param([(0, 0), (1, 0), (1, 1), (0, True)], id="a bool"),
        pytest.param("0011", id="a text"),
        pytest.param(None, id="None"),
    ],
)
def test_corners_of_the_wrong_shape_or_type_are_a_type_error_naming_them(corners):
    with pytest.raises(TypeError, match="corners"):
        Layer.fill("#ffffff").perspective(corners)


# --- maps and masks ------------------------------------------------------------------------------------------------


def grey_png(path: Path, levels: np.ndarray) -> Path:
    """An 8-bit grey PNG of `levels` (uint8, height, width), written with the standard library."""
    height, width = levels.shape

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    rows = b"".join(b"\x00" + row.tobytes() for row in levels)
    header = struct.pack(">IIBBBBB", width, height, 8, 0, 0, 0, 0)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")
    )
    return path


LEVELS = np.array([[0.0, 0.25, 0.5, 1.0], [1.0, 0.5, 0.25, 0.0]], np.float32)


def masked_alpha(layer: Layer) -> np.ndarray:
    return Scene(4, 2).add(layer).render(4)[..., 3]


def photo(**maps) -> Layer:
    """A picture of the size of `LEVELS` with the maps given."""
    return Layer.picture(np.full((2, 4, 3), 0.5, np.float32), maps=maps)


@pytest.mark.parametrize(
    "given",
    [
        pytest.param(LEVELS, id="float32"),
        pytest.param(LEVELS.astype(np.float64), id="float64"),
        pytest.param(np.rint(LEVELS * 255).astype(np.uint8), id="uint8"),
        pytest.param(np.rint(LEVELS * 65535).astype(np.uint16), id="uint16"),
    ],
)
def test_a_map_array_of_any_supported_type_is_the_alpha_a_mask_by_its_name_gives(given):
    # a whole level of 255 or 65535 is within half a level of the quarters this picture holds
    assert np.allclose(masked_alpha(photo(m=given).mask("m")), LEVELS, atol=0.5 / 255)


def test_a_map_file_is_read_when_the_scene_renders_as_a_grey_picture(tmp_path):
    path = grey_png(tmp_path / "m.png", np.rint(LEVELS * 255).astype(np.uint8))
    layer = photo(m=path).mask("m")  # not read yet: a file that is gone later is a render error
    assert np.allclose(masked_alpha(layer), LEVELS, atol=0.5 / 255)
    path.unlink()
    with pytest.raises(ReadError, match=r"m\.png"):
        masked_alpha(layer)


def test_a_map_of_another_size_or_a_hole_is_refused_at_render_naming_the_layer_and_the_map():
    with pytest.raises(InvalidError, match=r"photo\.map\.m.*3 x 2.*4 x 2"):
        masked_alpha(photo(m=np.zeros((2, 3), np.float32)).named("photo"))
    with pytest.raises(InvalidError, match=r"photo\.map\.m.*hole"):
        masked_alpha(photo(m=None).named("photo"))


@pytest.mark.parametrize(
    ("maps", "error", "match"),
    [
        pytest.param({"m": np.zeros((2, 4, 1), np.float32)}, InvalidError, "values.*height, width", id="three axes"),
        pytest.param({"m": np.full((2, 4), 2.0, np.float32)}, InvalidError, "values", id="a level above 1"),
        pytest.param({"m": np.zeros((2, 4), np.int8)}, TypeError, "uint8, uint16, float32 or float64", id="an int8"),
        pytest.param({"m": "m.png"}, TypeError, r"Path\('m.png'\)", id="a text"),
        pytest.param({"2m": LEVELS}, InvalidError, "map", id="a name that starts with a digit"),
        pytest.param({"": LEVELS}, InvalidError, "map", id="an empty name"),
    ],
)
def test_a_map_that_is_not_one_is_refused_at_the_call(maps, error, match):
    with pytest.raises(error, match=match):
        photo(**maps)


def test_a_mask_by_a_layer_reads_alpha_or_luminance_and_inverts():
    ramp = np.array([[[1, 0, 0, a] for a in (0.0, 0.25, 0.5, 1.0)]] * 2, np.float32)
    by = Layer.picture(ramp)
    fill = Layer.fill("#ffffff")
    assert np.allclose(masked_alpha(fill.mask(by)), LEVELS[[0, 0]], atol=1e-6)
    assert np.allclose(masked_alpha(fill.mask(by, invert=True)), 1 - LEVELS[[0, 0]], atol=1e-6)
    # pure red is 0.2126 of white, times its alpha
    assert np.allclose(masked_alpha(fill.mask(by, use="luminance")), 0.2126 * LEVELS[[0, 0]], atol=1e-6)


def test_a_feather_blurs_the_edge_of_a_mask_and_a_feather_of_zero_does_not():
    half = np.zeros((20, 20, 4), np.float32)
    half[:, :10] = 1
    base = Layer.fill("#ffffff")

    def edge_row(**options) -> np.ndarray:
        return Scene(20, 20).add(base.mask(Layer.picture(half), **options)).render(20)[10, :, 3]

    hard, soft = edge_row(), edge_row(feather=2)
    assert np.array_equal(hard, edge_row(feather=0))
    assert np.array_equal(hard, (np.arange(20) < 10).astype(np.float32))
    assert 0 < soft[9] < 1
    assert 0 < soft[10] < 1
    assert abs(soft[9] + soft[10] - 1) < 1e-5  # symmetric about the edge, up to the far edges of the shape


def test_a_mask_call_returns_a_new_layer_and_the_old_one_keeps_what_it_had():
    base = photo(m=LEVELS)
    masked = base.mask("m")
    assert np.array_equal(masked_alpha(base), np.ones((2, 4), np.float32))
    assert np.allclose(masked_alpha(masked), LEVELS, atol=1e-6)


@pytest.mark.parametrize(
    ("call", "match"),
    [
        pytest.param(lambda: photo(m=LEVELS).mask("n"), r'mask.*"n".*its maps: m', id="a name that is no map"),
        pytest.param(lambda: Layer.fill("#fff000").mask("m"), "mask", id="a name on a layer with no picture"),
        pytest.param(lambda: photo(m=LEVELS).mask("m", use="luminance"), "use", id="a map as luminance"),
        pytest.param(lambda: photo(m=LEVELS).mask("m", use="light"), "use", id="another word for use"),
        pytest.param(lambda: photo(m=LEVELS).mask("m", feather=-1), "feather", id="a negative feather"),
        pytest.param(lambda: photo(m=LEVELS).mask("m", feather=1e9), "feather.*1000000", id="a feather past the range"),
        pytest.param(
            lambda: photo(m=LEVELS).mask("m", feather=float("nan")), "feather", id="a feather that is no number"
        ),
        pytest.param(
            lambda: Layer.fill("#ffffff").mask(Layer.fill("#ffffff").named("shape")),
            "mask.*name",
            id="a named mask layer",
        ),
    ],
)
def test_a_mask_the_core_refuses_is_an_invalid_error_naming_the_field(call, match):
    with pytest.raises(InvalidError, match=match):
        call()


@pytest.mark.parametrize(
    ("kwargs", "match"),
    [
        pytest.param({"by": 3}, "by", id="a number for by"),
        pytest.param({"by": None}, "by", id="None for by"),
        pytest.param({"by": "m", "use": 1}, "use", id="a number for use"),
        pytest.param({"by": "m", "feather": "8"}, "feather", id="a text for the feather"),
        pytest.param({"by": "m", "feather": True}, "feather", id="a bool for the feather"),
        pytest.param({"by": "m", "invert": 1}, "invert", id="a number for invert"),
    ],
)
def test_a_wrong_type_in_a_mask_call_is_a_type_error_naming_the_argument(kwargs, match):
    with pytest.raises(TypeError, match=match):
        photo(m=LEVELS).mask(**kwargs)


def test_masks_nest_to_max_depth_and_one_deeper_is_an_invalid_error():
    assert MAX_DEPTH == 16
    layer = Layer.fill("#ffffff")
    for _ in range(MAX_DEPTH):
        layer = Layer.fill("#ffffff").mask(layer)
    assert np.array_equal(masked_alpha(layer), np.ones((2, 4), np.float32))
    with pytest.raises(InvalidError, match=r"mask.*16"):
        Layer.fill("#ffffff").mask(layer)


def test_mask_is_keyword_only_after_by():
    with pytest.raises(TypeError):
        Layer.fill("#ffffff").mask(Layer.fill("#ffffff"), "luminance")  # type: ignore[call-arg]  # on purpose


# --- text ----------------------------------------------------------------------------------------------------------


def typeset(content="Hi", **options) -> Layer:
    return Layer.text(content, font="bricolage_grotesque", **options)


def test_a_text_layer_is_painted_in_its_colour_where_the_letters_are_and_transparent_elsewhere():
    drawn = Scene(200, 100).add(typeset(size=60, color="#336699").place(dx=20, dy=70)).render(200)
    painted = drawn[..., 3] > 0
    assert 200 < painted.sum() < 0.3 * painted.size
    assert np.allclose(drawn[drawn[..., 3] == 1][:, :3], np.array([0x33, 0x66, 0x99]) / 255, atol=1e-6)
    assert not drawn[~painted].any()


def test_the_text_options_are_vectors_and_the_text_is_set_where_place_puts_its_anchor():
    plain = Scene(300, 100).add(typeset("Hi", size=60).place(dx=10, dy=70)).render(300)[..., 3]
    moved = Scene(300, 100).add(typeset("Hi", size=60).place(dx=110, dy=70)).render(300)[..., 3]
    # a move by whole pixels: the same picture, to the rounding of the fraction the box starts at (1e-14 of a pixel)
    assert np.allclose(plain[:, :190], moved[:, 100:290], atol=2 / 255)
    wide = Scene(300, 100).add(typeset("Hi", size=60, tracking=400).place(dx=10, dy=70)).render(300)[..., 3]
    assert (wide > 0).any(axis=0).nonzero()[0].max() > (plain > 0).any(axis=0).nonzero()[0].max()


@pytest.mark.parametrize(
    ("options", "match"),
    [
        pytest.param({"size": -1}, "size", id="a negative size"),
        pytest.param({"bogus": 1}, "bogus", id="an unknown option"),
        pytest.param({"align": "middle"}, "align", id="a word that is no alignment"),
        pytest.param({"anchor": [1, 2]}, "anchor", id="an anchor"),
        pytest.param({"inside": "M0 0 L9 0 L9 9 Z"}, "inside", id="a shape"),
        pytest.param({"size": {"param": "s"}}, "size", id="an expression"),
        pytest.param({"color": "red"}, "color", id="a colour that is none"),
    ],
)
def test_a_text_option_vector_or_scene_refuses_is_an_invalid_error_naming_it(options, match):
    with pytest.raises(InvalidError, match=match):
        typeset(**options)


def test_a_font_that_is_not_there_is_refused_by_name_or_by_file(tmp_path):
    with pytest.raises(InvalidError, match=r"no_such_font.*bricolage_grotesque"):
        Layer.text("Hi", font="no_such_font")
    with pytest.raises(ReadError, match=r"none\.ttf"):
        Layer.text("Hi", font=tmp_path / "none.ttf")
    (tmp_path / "bad.ttf").write_bytes(b"not a font")
    with pytest.raises(InvalidError, match=r"font.*bad\.ttf"):
        Layer.text("Hi", font=tmp_path / "bad.ttf")


@pytest.mark.parametrize(
    ("call", "match"),
    [
        pytest.param(lambda: Layer.text("Hi"), "font", id="no font"),  # type: ignore[call-arg]  # on purpose
        pytest.param(lambda: Layer.text("Hi", font=3), "font", id="a number for the font"),  # type: ignore[arg-type]  # on purpose
        pytest.param(
            lambda: Layer.text(3, font="bricolage_grotesque"),  # type: ignore[arg-type]  # on purpose
            "content",
            id="a number for the content",
        ),
        pytest.param(lambda: typeset(color=0), "color", id="a number for the colour"),
    ],
)
def test_a_text_of_the_wrong_type_is_a_type_error_naming_the_argument(call, match):
    with pytest.raises(TypeError, match=match):
        call()


def test_options_that_are_not_plain_data_are_an_invalid_error_and_a_text_masks():
    with pytest.raises(InvalidError, match="text"):
        typeset(size={1, 2})
    letters = typeset("Hi", size=60).place(dx=20, dy=70)
    clipped = Scene(200, 100).add(Layer.fill("#ff0000").mask(letters)).render(200)
    assert np.array_equal(clipped[..., 3], Scene(200, 100).add(letters).render(200)[..., 3])


# --- the Python surface the review of C3 found loose -----------------------------------------------------------------


@pytest.mark.parametrize("maps", [[("m", LEVELS)], (("m", LEVELS),), "m", 3, [LEVELS]])
def test_maps_that_are_no_mapping_are_a_type_error_naming_maps_and_what_is_wanted(maps):
    with pytest.raises(TypeError, match=r"maps.*mapping"):
        Layer.picture(np.zeros((2, 4, 3), np.float32), maps=maps)


def test_a_map_name_that_is_no_text_is_a_type_error_naming_maps():
    with pytest.raises(TypeError, match=r"maps.*name"):
        Layer.picture(np.zeros((2, 4, 3), np.float32), maps={3: LEVELS})  # type: ignore[dict-item]  # on purpose


def test_the_text_option_refusal_names_the_python_call_not_rusts():
    with pytest.raises(InvalidError, match=r"Layer\.drawing") as raised:
        typeset(inside="M0 0 L9 0 L9 9 Z")
    assert "::" not in str(raised.value)


def test_a_lone_surrogate_in_the_content_is_an_invalid_error_naming_content():
    with pytest.raises(InvalidError, match="content"):
        typeset("\ud800")


@pytest.mark.parametrize("value", [-1e-300, 1e300, -1e-9])
def test_a_number_the_core_refuses_is_shown_short(value):
    with pytest.raises(InvalidError, match="feather") as raised:
        photo(m=LEVELS).mask("m", feather=value)
    assert len(str(raised.value)) < 160


def test_a_boolean_map_is_refused_naming_the_dtype_and_the_fix():
    with pytest.raises(TypeError, match=r"bool.*astype"):
        photo(m=LEVELS > 0.3)


@pytest.mark.parametrize("value", [True, np.True_])
def test_a_bool_is_no_opacity(value):
    with pytest.raises(TypeError, match="opacity"):
        Layer.fill("#ffffff").opacity(value)


# --- blend and light -----------------------------------------------------------------------------------------------

BLEND_NAMES = [
    "normal",
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "color-dodge",
    "color-burn",
    "hard-light",
    "soft-light",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
]


def cell(rgba):
    return Layer.picture(np.array([[rgba]], np.float32))


def joined(backdrop, source):
    return Scene(1, 1).add(cell(backdrop), source).render(1)[0, 0]


@pytest.mark.parametrize("name", BLEND_NAMES)
def test_every_css_name_is_a_blend_and_a_blend_is_a_new_layer(name):
    below, plain = [0.5, 0.25, 0.5, 1], cell([0.25, 0.5, 0.75, 1])
    blended = plain.blend(name)
    assert blended is not plain
    if name == "normal":
        assert np.array_equal(joined(below, blended), joined(below, plain))
    else:
        assert not np.array_equal(joined(below, blended), joined(below, plain))
    # the layer the call started from is unchanged
    assert np.array_equal(joined(below, plain), [0.25, 0.5, 0.75, 1])


def test_multiply_by_hand_and_the_layers_opacity_scales_what_is_blended():
    # multiply of (1, 0.5, 0) at opacity 0.5 over the opaque (0.5, 0.5, 1): B = (0.5, 0.25, 0), and
    # 0.5 Cb + 0.5 B = (0.5, 0.375, 0.5)
    got = joined([0.5, 0.5, 1, 1], cell([1, 0.5, 0, 1]).blend("multiply").opacity(0.5))
    assert np.allclose(got, [0.5, 0.375, 0.5, 1], atol=1e-6)


@pytest.mark.parametrize("word", ["Multiply", "soft_light", "SCREEN", "softlight", "colour-dodge", "bogus", ""])
def test_a_word_that_is_no_mode_is_refused_naming_blend_the_names_and_the_nearest(word):
    with pytest.raises(InvalidError, match="blend") as raised:
        Layer.fill("#ffffff").blend(word)
    message = str(raised.value)
    assert all(name in message for name in BLEND_NAMES)
    assert "did you mean" in message


@pytest.mark.parametrize(("word", "fix"), [("Multiply", "multiply"), ("soft_light", "soft-light")])
def test_a_slip_of_case_or_underscore_names_the_fix(word, fix):
    with pytest.raises(InvalidError, match=f'did you mean "{fix}"'):
        Layer.fill("#ffffff").blend(word)


@pytest.mark.parametrize("value", [None, 3, b"multiply", ["multiply"], True])
def test_a_blend_that_is_no_text_is_a_type_error_naming_blend(value):
    with pytest.raises(TypeError, match="blend"):
        Layer.fill("#ffffff").blend(value)


def test_a_light_layer_screened_over_black_gives_its_colour_black_adds_nothing_and_white_is_opaque():
    black = cell([0, 0, 0, 1])
    got = joined([0, 0, 0, 1], cell([0.25, 0.5, 0.75, 1]).as_light().blend("screen"))
    assert np.allclose(got, [0.25, 0.5, 0.75, 1], atol=1e-6)
    assert np.allclose(joined([0.2, 0.4, 0.6, 1], black.as_light().blend("screen")), [0.2, 0.4, 0.6, 1], atol=1e-6)
    assert np.allclose(Scene(1, 1).add(cell([1, 1, 1, 1]).as_light()).render(1)[0, 0], [1, 1, 1, 1])
    # (0.5, 0.5, 0.5) at 0.5 is 0.25 of light: alpha 0.25, straight colour white
    assert np.allclose(Scene(1, 1).add(cell([0.5, 0.5, 0.5, 0.5]).as_light()).render(1)[0, 0], [1, 1, 1, 0.25])


def test_as_light_off_is_no_call_and_on_is_the_default():
    layer = cell([0.25, 0.5, 0.75, 0.5])
    plain = Scene(1, 1).add(layer).render(1)
    assert np.array_equal(Scene(1, 1).add(layer.as_light(False)).render(1), plain)
    assert np.array_equal(Scene(1, 1).add(layer.as_light()).render(1), Scene(1, 1).add(layer.as_light(True)).render(1))
    assert not np.array_equal(Scene(1, 1).add(layer.as_light()).render(1), plain)


@pytest.mark.parametrize("value", [1, 0, "yes", None, 1.0])
def test_light_that_is_no_bool_is_a_type_error_naming_on(value):
    with pytest.raises(TypeError, match="on"):
        Layer.fill("#ffffff").as_light(value)


def test_a_mask_layer_with_a_blend_or_a_light_is_refused_at_the_mask_call_and_normal_is_not():
    for made, word in [(Layer.fill("#ffffff").blend("screen"), "blend"), (Layer.fill("#ffffff").as_light(), "light")]:
        with pytest.raises(InvalidError, match="mask") as raised:
            Layer.fill("#ffffff").mask(made)
        assert word in str(raised.value)
    Layer.fill("#ffffff").mask(Layer.fill("#ffffff").blend("normal").as_light(False))
