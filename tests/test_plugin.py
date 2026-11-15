"""Layer kinds by name: the rules for getting into the registry, the JSON form of a layer made through it, the
cache, what a plugin that breaks the contract is refused with, and threads (the contract every kind keeps is in
test_layer_kind_contract.py). Kind names here start with `plugin-` so that no test collides with another in one
process; each test registers into an isolated registry."""

import math
import re
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import numpy as np
import pytest

import bullterrier_scene as bts
from bullterrier_scene import plugin


def cells(length: float, scale: float) -> int:
    """`ceil(length * scale)`, a product within 1e-12 of a whole number counting as that number."""
    return math.ceil(length * scale * (1 - 1e-12))


def gradient(x, y, width, height, scale_x, scale_y, *, tone=0.5):
    """Red is the tone; green and blue ramp with the design position of the pixel (a function of the question alone)."""
    columns, rows = cells(width, scale_x), cells(height, scale_y)
    across = ((x + np.arange(columns) / scale_x) * 0.01) % 1.0
    down = ((y + np.arange(rows) / scale_y) * 0.01) % 1.0
    pixels = np.empty((rows, columns, 4), dtype=np.float32)
    pixels[..., 0] = tone
    pixels[..., 1] = across[None, :]
    pixels[..., 2] = down[:, None]
    pixels[..., 3] = 1.0
    return pixels


@pytest.fixture(autouse=True)
def own_registry():
    """Each test registers into a registry of its own: nothing a test before it added."""
    with plugin.isolated():
        yield


# --- getting in ---

BAD_NAMES = {
    "empty": "",
    "a capital": "Plugin",
    "a space": "a b",
    "a leading dash": "-a",
    "longer than 64": "a" * 65,
    "a megabyte": "a" * (1 << 20),
    "picture": "picture",
    "drawing": "drawing",
    "text": "text",
    "fill": "fill",
    "scene": "scene",
    "of": "of",
    "make": "make",
}


@pytest.mark.parametrize("name", BAD_NAMES.values(), ids=BAD_NAMES.keys())
def test_a_bad_name_is_refused_in_a_line_and_leaves_the_registry_as_it_was(name):
    with pytest.raises(bts.InvalidError, match="kind") as refused:
        bts.register_layer(name, gradient)
    assert len(str(refused.value)) < 400
    assert bts.layer_kinds() == []


def test_a_good_name_registers_and_is_listed_sorted_and_the_function_is_returned_unchanged():
    assert bts.register_layer("plugin-b.2_x", gradient) is gradient
    bts.layer_kind("plugin-a")(gradient)
    assert bts.layer_kinds() == ["plugin-a", "plugin-b.2_x"]


def test_a_function_that_is_not_callable_is_a_type_error():
    with pytest.raises(TypeError, match="plugin-x"):
        bts.register_layer("plugin-x", 3)  # type: ignore[type-var]  # the known-bad input under test


MODULE_LEVEL = """
def draw(x, y, width, height, scale_x, scale_y):
    import numpy as np
    return np.full((int(width * scale_x), int(height * scale_y), 4), VALUE, dtype=np.float32)
"""


def test_a_module_level_function_defined_again_replaces_itself():
    # what a reloaded module does: the same module and function name, a new function object
    for value in (0.25, 0.75):
        scope: dict[str, Any] = {"__name__": "reloaded_layer_plugin"}
        exec(MODULE_LEVEL.replace("VALUE", str(value)), scope)  # noqa: S102  (test source)
        bts.register_layer("plugin-reloaded", scope["draw"])
    picture = bts.Scene(2, 2).add(bts.Layer.make("plugin-reloaded")).render(2)
    assert picture[0, 0].tolist() == [0.75] * 4


def twin():
    def other(x, y, width, height, scale_x, scale_y):
        return gradient(x, y, width, height, scale_x, scale_y)

    return other


def test_another_function_cannot_take_a_name_a_plugin_holds_and_the_first_keeps_it():
    bts.register_layer("plugin-held", gradient)
    with pytest.raises(bts.InvalidError, match="plugin-held"):
        bts.register_layer("plugin-held", twin())
    assert bts.Scene(4, 4).add(bts.Layer.make("plugin-held", tone=0.5)).render(4)[0, 0, 0] == 0.5


def test_a_second_closure_from_one_factory_cannot_take_the_name_of_the_first_and_the_same_one_may_again():
    first = twin()
    bts.register_layer("plugin-closure", first)
    bts.register_layer("plugin-closure", first)
    with pytest.raises(bts.InvalidError, match="plugin-closure"):
        bts.register_layer("plugin-closure", twin())


def test_isolated_drops_what_the_block_registered_and_keeps_what_was_there():
    bts.register_layer("plugin-outer", gradient)
    with plugin.isolated():
        assert bts.layer_kinds() == []
        bts.register_layer("plugin-inner", gradient)
    assert bts.layer_kinds() == ["plugin-outer"]


# --- Layer.make and the JSON form ---


def plugged() -> bts.Scene:
    return bts.Scene(40, 20).add(
        bts.Layer.fill("#102030"),
        bts.Layer.make("plugin-gradient", tone=0.25).named("tone").opacity(0.5),
        bts.Layer.make("plugin-gradient").place(dx=3.5),
    )


def test_a_scene_with_a_python_kind_is_written_and_read_back_equal_and_draws_the_same():
    bts.register_layer("plugin-gradient", gradient)
    one = plugged()
    text = one.to_json()
    assert '"make":{"kind":"plugin-gradient","options":{"tone":0.25}}' in text
    back = bts.Scene.from_json(text)
    assert back == one
    assert back.to_json() == text
    for size in (40, 81):
        assert back.render(size).tobytes() == one.render(size).tobytes()


def test_a_document_naming_a_kind_that_is_not_registered_is_a_format_error_at_the_kind_listing_the_known_ones():
    bts.register_layer("plugin-gradient", gradient)
    text = plugged().to_json().replace('"kind":"plugin-gradient","options":{"tone":0.25}', '"kind":"plugin-gradiant"')
    with pytest.raises(bts.FormatError, match=r"^/layers/1/make/kind.*plugin-gradient") as refused:
        bts.Scene.from_json(text)
    assert "did you mean plugin-gradient" in str(refused.value)


def test_options_a_kind_cannot_take_are_an_invalid_error_naming_the_kind_at_the_first_draw():
    bts.register_layer("plugin-gradient", gradient)
    layer = bts.Layer.make("plugin-gradient", loudness=3)  # the function has no such keyword: its call fails
    with pytest.raises(bts.InvalidError, match=r"plugin-gradient.*loudness"):
        bts.Scene(4, 4).add(layer).render(4)


@pytest.mark.parametrize(
    ("kind", "options", "mention"),
    [
        pytest.param("plugin-nope", {}, "plugin-gradient", id="unknown kind lists the known ones"),
        pytest.param("plugin-gradient", {"tone": object()}, "options", id="options that are not data"),
        pytest.param("plugin-gradient", {"tone": float("nan")}, "options", id="options that are not JSON"),
    ],
)
def test_make_refuses_what_it_cannot_make_naming_the_problem(kind, options, mention):
    bts.register_layer("plugin-gradient", gradient)
    with pytest.raises(bts.InvalidError, match=mention):
        bts.Layer.make(kind, **options)


def test_options_of_a_megabyte_are_carried_to_the_function_and_written():
    seen = []

    def read(x, y, width, height, scale_x, scale_y, *, text):
        seen.append(len(text))
        return gradient(x, y, width, height, scale_x, scale_y)

    bts.register_layer("plugin-big", read)
    scene = bts.Scene(4, 4).add(bts.Layer.make("plugin-big", text="x" * (1 << 20)))
    scene.render(4)
    assert seen == [1 << 20]
    assert bts.Scene.from_json(scene.to_json()) == scene


# --- the cache ---


def test_what_a_kind_drew_is_kept_and_a_changed_option_is_never_served_stale():
    asked = []

    def counting(x, y, width, height, scale_x, scale_y, *, tone):
        asked.append(tone)
        return gradient(x, y, width, height, scale_x, scale_y, tone=tone)

    bts.register_layer("plugin-counting", counting)
    base = bts.Scene(40, 20)
    first = base.add(bts.Layer.make("plugin-counting", tone=0.2))
    second = base.add(bts.Layer.make("plugin-counting", tone=0.6))
    warm = first.render(80)
    assert first.render(80).tobytes() == warm.tobytes()
    assert asked == [0.2]
    other = second.render(80)
    assert not np.array_equal(other, warm)
    # the same picture a scene with an empty cache of its own draws
    cold = bts.Scene(40, 20).add(bts.Layer.make("plugin-counting", tone=0.6)).render(80)
    assert other.tobytes() == cold.tobytes()
    assert first.render(80).tobytes() == warm.tobytes()


# --- a plugin that breaks the contract is refused, naming the kind ---


def wrong_size(*_):
    return np.zeros((1, 1, 4), np.float32)


def not_a_number(x, y, width, height, scale_x, scale_y):
    pixels = gradient(x, y, width, height, scale_x, scale_y)
    pixels[1, 2, 1] = np.nan
    return pixels


def above_one(x, y, width, height, scale_x, scale_y):
    pixels = gradient(x, y, width, height, scale_x, scale_y)
    pixels[0, 0, 3] = 1.5
    return pixels


def raises(*_):
    raise KeyError("boom")


def huge(*_):
    return np.broadcast_to(np.float32(0), (1 << 14, 1 << 14, 4))  # 2^28 pixels, no memory behind it


def not_an_array(*_):
    return "pixels"


def two_dimensional(*_):
    return np.zeros((4, 4), np.float32)


BAD_KINDS = {
    "the wrong size": (wrong_size, r"1 x 1.*8 x 8"),
    "a NaN": (not_a_number, r"row 1, column 2.*nan"),
    "a value above 1": (above_one, r"1\.5"),
    "an exception": (raises, r"KeyError.*boom"),
    "more pixels than a picture has": (huge, r"2\^27"),
    "something that is no array": (not_an_array, "plugin-bad"),
    "a grey array": (two_dimensional, "plugin-bad"),
}


@pytest.mark.parametrize(("draw", "mention"), BAD_KINDS.values(), ids=BAD_KINDS.keys())
def test_a_plugin_that_breaks_the_contract_is_refused_naming_the_kind_and_the_value(draw, mention):
    bts.register_layer("plugin-bad", draw)
    scene = bts.Scene(8, 8).add(bts.Layer.make("plugin-bad"))
    with pytest.raises(bts.InvalidError, match="plugin-bad") as refused:
        scene.render(8)
    assert re.search(mention, str(refused.value), flags=re.IGNORECASE), str(refused.value)


# --- threads ---


def test_several_renders_of_a_python_kind_at_once_give_what_one_render_gives():
    bts.register_layer("plugin-gradient", gradient)
    scenes = [bts.Scene(40, 20).add(bts.Layer.make("plugin-gradient", tone=tone / 8)) for tone in range(9)]
    expected = [scene.render(160).tobytes() for scene in scenes]
    with ThreadPoolExecutor(max_workers=8) as pool:
        found = list(pool.map(lambda scene: scene.render(160).tobytes(), scenes * 4))
    assert found == expected * 4


def by_place(x, y, width, height, scale_x, scale_y):
    """Pixels at the rectangle's even division: a function of the pixel's place, so any rows are a crop of the whole."""
    columns, rows = cells(width, scale_x), cells(height, scale_y)
    across = ((x + np.arange(columns) * width / columns) * 0.01) % 1.0
    down = ((y + np.arange(rows) * height / rows) * 0.01) % 1.0
    pixels = np.empty((rows, columns, 4), dtype=np.float32)
    pixels[..., 0] = 0.5
    pixels[..., 1] = across[None, :]
    pixels[..., 2] = down[:, None]
    pixels[..., 3] = 1.0
    return pixels


@pytest.mark.parametrize("size", [40, 37])
def test_a_kind_that_draws_by_position_is_asked_for_the_rows_a_band_reads_and_the_bands_make_the_picture(size):
    asked = []

    def recording(x, y, width, height, scale_x, scale_y):
        asked.append(height)
        return by_place(x, y, width, height, scale_x, scale_y)

    bts.register_layer("plugin-by-position", recording, by_position=True)
    scene = bts.Scene(40, 30).add(bts.Layer.make("plugin-by-position"))
    whole = scene.render(size)
    asked.clear()
    bands = []
    scene.render_bands(size, 3, lambda first, band: bands.append(band.copy()))
    assert np.concatenate(bands).tobytes() == whole.tobytes()
    assert len(asked) >= 10
    assert max(asked) < 30


def test_a_kind_that_does_not_say_so_is_asked_once_for_its_rectangle_whatever_the_bands():
    asked = []

    def recording(x, y, width, height, scale_x, scale_y):
        asked.append(height)
        return by_place(x, y, width, height, scale_x, scale_y)

    bts.register_layer("plugin-whole", recording)
    bts.Scene(40, 30).add(bts.Layer.make("plugin-whole")).render_bands(40, 3, lambda first, band: None)
    assert asked == [30.0]
