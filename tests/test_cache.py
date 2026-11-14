"""The cache's one promise: it skips work and changes no result. The oracle is the cold render: a scene made afresh (so
it shares no cache with anything) drawn once. Every address of every example scene is changed on a scene whose cache has
been used, and the picture must equal the cold scene's, bit for bit; a key that lacks an input fails for that address.
The inputs that are no address (the size, a swapped picture, two threads) and two animations' values follow.
"""

import importlib.util
import threading
from collections.abc import Callable
from pathlib import Path
from types import ModuleType

import numpy as np
import pytest

from bullterrier_scene import InvalidError, Parameter, Scene


def _example_scenes() -> ModuleType:
    spec = importlib.util.spec_from_file_location("example_scenes", Path(__file__).with_name("example_scenes.py"))
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


examples = _example_scenes()
ALL_SCENES: dict[str, Callable[[], Scene]] = examples.ALL_SCENES
photo, matte, load = examples.photo, examples.matte, examples.load

SIZE = 32


def cold(scene: Scene) -> Scene:
    """The same layers in a scene made afresh: it shares no cache with `scene` or anything made from it."""
    return Scene(scene.width, scene.height).add(*scene.layers)


def filled(scene: Scene) -> Scene:
    """`scene` with each hole (a picture or a map not given) filled, so that it can be drawn."""
    holes = {
        address: matte(24, 32) if ".map." in address else photo(24, 32)
        for address, parameter in scene.parameters().items()
        if parameter.kind == "picture" and parameter.value is None
    }
    return scene.with_values(holes)


def drawn(scene: Scene, size: int = SIZE) -> bytes | tuple[str, str]:
    """The picture as bytes, or the refusal: a cold and a used scene must refuse alike."""
    try:
        return scene.render(size).tobytes()
    except InvalidError as error:
        return ("InvalidError", str(error))


def another(parameter: Parameter) -> object:
    """A valid value of `parameter` that is not the one it holds."""
    value, low, high = parameter.value, parameter.min, parameter.max
    match parameter.kind:
        case "bool":
            return not value
        case "choice":
            return next(name for name in parameter.choices or () if name != value)
        case "color":
            return "#13a7c5" if value != "#13a7c5" else "#c5a713"
        case "text":
            return f"{value}x"
        case "picture":
            held = np.asarray(value)
            return (1 - held) if held.ndim == 2 else np.clip(held * 0.5 + 0.3, 0, 1).astype(np.float32)
        case "int" | "float":
            assert isinstance(value, (int, float))
            assert low is not None
            assert high is not None
            if parameter.unit == "ratio":
                return min(max(value * 1.37, low), high) if value * 1.37 != value else value + 1
            step = 0.17 * (high - low) if high - low <= 360 else 7.5
            candidate = value + step if value + step <= high else value - step
            return round(candidate) if parameter.kind == "int" else candidate
    raise AssertionError(parameter.kind)


@pytest.mark.parametrize("name", list(ALL_SCENES))
def test_every_parameter_changed_on_a_used_scene_draws_what_a_cold_scene_draws(name):
    used = filled(ALL_SCENES[name]())
    base = drawn(used)
    assert base == drawn(cold(used))
    mismatched = []
    tried = seen = 0
    for address, parameter in used.parameters().items():
        if parameter.value is None:
            continue
        values = {address: another(parameter)}
        # the used scene has drawn the scene and every change before this one, and shares its cache with what it makes
        warm = drawn(used.with_values(values))
        seen += warm != base
        if warm != drawn(cold(used).with_values(values)):
            mismatched.append(address)
        tried += 1
    assert mismatched == []
    # the oracle sees only what shows: a scene whose changes mostly do not show would prove nothing
    assert seen * 8 >= tried, f"{seen} of {tried} changes changed the picture"


@pytest.mark.parametrize("name", ["mill", "composed_world_style", "poster_style"])
def test_a_scene_drawn_at_one_size_then_another_then_the_first_again_draws_what_cold_scenes_draw(name):
    used = ALL_SCENES[name]()
    for size in (24, 56, 24, 56, 31):
        assert drawn(used, size) == drawn(ALL_SCENES[name](), size), size


def test_a_swapped_picture_and_a_swapped_map_draw_what_a_cold_scene_draws():
    used = ALL_SCENES["poster_style"]()
    drawn(used)
    for n in range(3):
        values = {
            "photo.picture": photo(120, 90) * (0.9 - 0.2 * n),
            "photo.map.subject": matte(120, 90) * (1 - 0.3 * n),
        }
        assert drawn(used.with_values(values)) == drawn(ALL_SCENES["poster_style"]().with_values(values))
    # the same pixels in another array are the same picture
    again = {"photo.picture": photo(120, 90) * 0.9}
    assert drawn(used.with_values(again)) == drawn(ALL_SCENES["poster_style"]().with_values(again))


def test_scenes_that_share_a_cache_drawn_by_two_threads_draw_what_cold_scenes_draw():
    base = ALL_SCENES["mill"]()
    frames = [base.with_values({"logo.scale": 1 + n / 20, "logo_glow.opacity": n / 8}) for n in range(8)]
    expected = [
        drawn(ALL_SCENES["mill"]().with_values({"logo.scale": 1 + n / 20, "logo_glow.opacity": n / 8}))
        for n in range(8)
    ]
    got: list[list[tuple[int, bytes | tuple[str, str]]]] = [[], []]

    def work(which: int) -> None:
        for n in [*range(8), *reversed(range(8))] if which else range(8):
            got[which].append((n, drawn(frames[n])))

    threads = [threading.Thread(target=work, args=(which,)) for which in (0, 1)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    assert all(picture == expected[n] for drawn_by_one in got for n, picture in drawn_by_one)


def test_the_mill_frames_of_an_animation_through_one_cache_draw_what_cold_frames_draw():
    module = load("mill")
    used = ALL_SCENES["mill"]()
    for n in range(12):
        t = n / 11
        values = module.frame_values(
            t, abs(1 - 2 * t), t**2, min(1.0, 2 * t), max(0.0, min(1.0, 3 * t - 1.0)), [t, t * 0.6, t * 0.3, 0]
        )
        assert drawn(used.with_values(values)) == drawn(ALL_SCENES["mill"]().with_values(values)), n


def test_the_dog_sways_and_the_burst_pulses_through_one_cache_as_cold_scenes_would_draw():
    """The loop of the wow poster: a looked picture that bobs and turns, a drawing that pulses, a group that swells."""
    used = ALL_SCENES["composed_world_style"]()
    for n in range(10):
        values = {"dog.dy": 30 + 3.0 * np.sin(n), "dog.rotate": 1.4 * np.sin(n / 3), "sun.scale": 1 + 0.035 * (n % 4)}
        assert drawn(used.with_values(values)) == drawn(ALL_SCENES["composed_world_style"]().with_values(values)), n
