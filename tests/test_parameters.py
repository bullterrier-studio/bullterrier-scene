"""`Scene.parameters` and `Scene.with_values` through the package's public interface.

Three oracles: the ecosystem's parameter format (its schema and fixtures are the copy in `docs/parameter-format`, and
every parameter of every example scene is judged by the schema); the round trip (every parameter of a scene set to the
value it holds is accepted and changes nothing, and an address no parameter has is refused); and the scene built by its
own calls. The example scenes are the programs of `docs/examples`, built from synthetic inputs.
"""

import dataclasses
import importlib.util
import json
from pathlib import Path
from types import ModuleType

import numpy as np
import pytest
from jsonschema import Draft202012Validator

import bullterrier_scene as bts
from bullterrier_scene import InvalidError, Layer, Parameter, Scene

ROOT = Path(__file__).resolve().parents[1]
FORMAT = ROOT / "docs" / "parameter-format"
SCHEMA = Draft202012Validator(json.loads((FORMAT / "schemas" / "parameter-3.schema.json").read_text(encoding="utf-8")))


def _example_scenes() -> ModuleType:
    """tests/example_scenes.py, loaded by its path (tests/ is no package, and the pytest settings are the template's)"""
    spec = importlib.util.spec_from_file_location("example_scenes", Path(__file__).with_name("example_scenes.py"))
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


examples = _example_scenes()
LOGO, SCENES, load, matte, photo = examples.LOGO, examples.SCENES, examples.load, examples.matte, examples.photo


def fixtures(name: str) -> list[object]:
    entries = json.loads((FORMAT / "fixtures" / "parameters" / name).read_text(encoding="utf-8"))
    return [pytest.param(entry["parameter"], id=entry["label"]) for entry in entries]


@pytest.mark.parametrize("parameter", fixtures("valid.json"))
def test_the_schema_accepts_a_valid_fixture(parameter):
    assert [error.message for error in SCHEMA.iter_errors(parameter)] == []


@pytest.mark.parametrize("parameter", fixtures("invalid.json"))
def test_the_schema_refuses_an_invalid_fixture(parameter):
    assert list(SCHEMA.iter_errors(parameter))


# --- the example scenes, from synthetic inputs ------------------------------------------------------------------------


@pytest.fixture(scope="module", params=list(SCENES), ids=list(SCENES))
def scene(request):
    return SCENES[request.param]()


def values_of(scene: Scene) -> dict[str, object]:
    return {address: parameter.value for address, parameter in scene.parameters().items()}


def same(left: np.ndarray, right: np.ndarray) -> bool:
    return left.shape == right.shape and left.tobytes() == right.tobytes()


def test_every_parameter_of_every_example_scene_conforms_to_the_parameter_format(scene):
    refused = {}
    for address, parameter in scene.parameters().items():
        described = parameter.to_dict()
        del described["address"]  # a dotted address is no `name`: the format's rule
        described["name"] = "parameter"
        errors = [error.message for error in SCHEMA.iter_errors(described)]
        if errors:
            refused[address] = errors
    assert refused == {}


def test_the_parameters_are_ordered_layer_by_layer_and_keyed_by_their_address(scene):
    parameters = scene.parameters()
    assert all(address == parameter.address for address, parameter in parameters.items())
    layer_names = [address.split(".")[0] for address in parameters]
    # a layer's parameters are together, layers in the order of the stack
    seen: list[str] = []
    for name in layer_names:
        if not seen or seen[-1] != name:
            seen.append(name)
    assert len(seen) == len(set(seen)) == len(scene.layers)


def test_setting_every_parameter_to_its_own_value_is_accepted_and_changes_nothing(scene):
    parameters = scene.parameters()
    picture = scene.render(96)
    again = scene.with_values(values_of(scene))
    assert again.parameters() == parameters
    assert same(again.render(96), picture)
    for address, parameter in parameters.items():
        one = scene.with_values({address: parameter.value})
        assert one.parameters() == parameters, address
        assert same(one.render(48), scene.render(48)), address


def test_an_address_the_scene_does_not_list_is_refused(scene):
    for address, parameter in scene.parameters().items():
        for wrong in (address + "x", address + ".", address.upper(), address.replace(".", "_", 1)):
            if wrong in scene.parameters():
                continue
            with pytest.raises(InvalidError):
                scene.with_values({wrong: parameter.value})


def test_a_refused_set_changes_nothing(scene):
    before = values_of(scene)
    first = next(iter(before))
    with pytest.raises(InvalidError):
        scene.with_values({first: before[first], "nowhere.opacity": 1})
    assert values_of(scene) == before


# --- the examples' frames -------------------------------------------------------------------------------------------


def test_the_cover_template_makes_six_different_covers_from_one_scene():
    module = load("cover_template")
    template = module.build()
    covers = [
        module.frame(template, module.episode(title, photo(36, 64) * (n + 1) / 6, colour), 64)
        for n, (title, _, colour) in enumerate(module.EPISODES)
    ]
    assert len({cover.tobytes() for cover in covers}) == 6
    with pytest.raises(InvalidError, match=r"photo\.picture"):
        template.render(64)  # the hole


def test_the_headline_slides_in_by_one_address_and_the_ground_stays_as_it_is():
    module = load("frame_sequence")
    scene = module.build()
    first, last = module.frame(scene, 0, 160), module.frame(scene, module.FRAMES - 1, 160)
    ground = scene.with_values({"headline.opacity": 0}).render(160)
    assert same(first, ground)  # at frame 0 the headline is not there
    assert not same(first, last)
    assert set(module.frame_values(5)) <= set(scene.parameters())


def test_the_mill_frame_sets_only_addresses_the_scene_lists_and_moves_them():
    module = load("mill")
    scene = module.build(photo(160, 90), matte(160, 90), matte(160, 90) * 0.5, LOGO)
    rest = module.frame_values(0.0, 0.0, 0.0, 0.0, 0.0, [0, 0, 0, 0])
    busy = module.frame_values(0.9, 0.6, 0.5, 1.0, 1.0, [1, 1, 1, 1])
    assert set(rest) == set(busy) <= set(scene.parameters())
    assert not same(module.frame(scene, rest, 64), module.frame(scene, busy, 64))


# --- the facade -----------------------------------------------------------------------------------------------------


def small() -> Scene:
    return Scene(12, 8).add(
        Layer.picture(None, maps={"matte": None}).named("photo"),
        Layer.fill("#336699").named("band"),
        Layer.text("Hi", font="bricolage_grotesque", size=6).named("title"),
    )


def test_a_parameter_is_a_frozen_record_of_the_formats_fields():
    parameter = small().parameters()["title.dx"]
    assert dataclasses.is_dataclass(parameter)
    assert isinstance(parameter, Parameter)
    assert (parameter.kind, parameter.unit, parameter.min, parameter.max, parameter.cyclic) == (
        "float",
        "px",
        -1e6,
        1e6,
        False,
    )
    with pytest.raises(dataclasses.FrozenInstanceError):
        parameter.value = 3.0  # type: ignore[misc]


def test_a_choice_a_picture_a_text_and_a_colour_are_described():
    parameters = small().parameters()
    blend = parameters["band.blend"]
    assert (blend.kind, blend.default, blend.choices is not None and len(blend.choices)) == ("choice", "normal", 16)
    photo_ = parameters["photo.picture"]
    assert (photo_.kind, photo_.nullable, photo_.default, photo_.value) == ("picture", True, None, None)
    assert parameters["photo.map.matte"].value is None
    text = parameters["title.text"]
    assert (text.kind, text.min_length, text.value) == ("text", 1, "Hi")
    assert parameters["band.color"].value == "#336699"
    assert parameters["title.rotate"].cyclic


def test_values_may_be_python_numbers_numpy_numbers_bools_paths_arrays_and_none():
    filled = small().with_values(
        {
            "band.opacity": np.float32(0.5),
            "band.dx": np.int64(3),
            "band.flip_x": np.bool_(True),
            "title.dy": 4,
            "photo.picture": np.zeros((4, 6, 3), np.uint8),
            "photo.map.matte": np.ones((4, 6), np.float32),
            "title.blend": np.str_("multiply"),
        }
    )
    parameters = filled.parameters()
    assert parameters["band.opacity"].value == 0.5
    assert parameters["band.dx"].value == 3.0
    assert parameters["band.flip_x"].value is True
    assert parameters["title.blend"].value == "multiply"
    assert np.asarray(parameters["photo.picture"].value).shape == (4, 6, 4)
    assert np.asarray(parameters["photo.map.matte"].value).shape == (4, 6)
    assert filled.render(12).shape == (8, 12, 4)
    # a Path is a file, and None is a hole again
    by_file = filled.with_values({"photo.picture": Path("a.png"), "photo.map.matte": None})
    assert by_file.parameters()["photo.picture"].value == Path("a.png")
    assert by_file.parameters()["photo.map.matte"].value is None


def test_a_held_picture_is_a_handle_that_goes_back_in_without_a_copy_of_its_pixels():
    pixels = photo(4, 6)
    held = small().with_values({"photo.picture": pixels, "photo.map.matte": matte(4, 6)})
    handle = held.parameters()["photo.picture"].value
    assert np.asarray(handle).shape == (4, 6, 4)
    assert np.array_equal(np.asarray(handle)[..., :3], pixels)
    assert np.asarray(handle, dtype=np.float64).dtype == np.float64
    assert held.with_values({"photo.picture": handle}).parameters() == held.parameters()


@pytest.mark.parametrize(
    ("values", "kind"),
    [
        pytest.param({"band.opacity": object()}, TypeError, id="an object is no value"),
        pytest.param({3: 1}, TypeError, id="an address is text"),
        pytest.param({"band.opacity": 10**400}, InvalidError, id="beyond the range of a float"),
        pytest.param({"band.opacity": True}, InvalidError, id="a bool is no number"),
        pytest.param({"band.light": 1}, InvalidError, id="a number is no bool"),
        pytest.param({"band.opacity": "0.5"}, InvalidError, id="a text is no number"),
        pytest.param({"band.opacity": 1.5}, InvalidError, id="out of range"),
        pytest.param({"band.opacity": float("nan")}, InvalidError, id="not a number"),
        pytest.param({"band.color": "red"}, InvalidError, id="a bad colour"),
        pytest.param({"title.text": ""}, InvalidError, id="no characters"),
        pytest.param({"band.blend": "Multiply"}, InvalidError, id="a bad blend"),
        pytest.param({"photo.picture": np.zeros((4, 6), np.uint8)}, InvalidError, id="a map is no picture"),
        pytest.param({"photo.map.matte": np.zeros((4, 6, 3), np.uint8)}, InvalidError, id="a picture is no map"),
        pytest.param({"photo.picture": "a.png"}, InvalidError, id="a text is no picture: pass Path(...)"),
        pytest.param({"band.opacity": np.zeros((2, 2, 3), np.bool_)}, TypeError, id="a bool array"),
    ],
)
def test_a_value_the_scene_cannot_take_is_refused(values, kind):
    with pytest.raises(kind):
        small().with_values(values)


def test_values_must_be_a_mapping():
    with pytest.raises(TypeError):
        small().with_values([("band.opacity", 1)])  # type: ignore[arg-type]


def test_the_scene_is_unchanged_and_the_new_one_shares_what_did_not_change():
    before = small()
    held = before.with_values({"photo.picture": photo(4, 6), "photo.map.matte": matte(4, 6)})
    after = held.with_values({"band.color": "#ff0000"})
    assert before.parameters()["band.color"].value == "#336699"
    assert held.parameters()["band.color"].value == "#336699"
    assert after.parameters()["band.color"].value == "#ff0000"
    assert after.parameters()["photo.picture"] == held.parameters()["photo.picture"]
    assert len(after.layers) == len(held.layers) == 3


def test_an_address_without_a_field_is_refused_saying_what_an_address_is():
    with pytest.raises(InvalidError, match=r"<layer>\.<field>"):
        small().with_values({"band": 0.5})


def test_the_package_exports_parameter():
    assert bts.Parameter is Parameter
    assert "Parameter" in bts.__all__
