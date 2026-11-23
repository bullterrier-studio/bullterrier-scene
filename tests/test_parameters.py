"""`Scene.parameters` and `Scene.with_values` through the package's public interface.

Three oracles: the ecosystem's parameter format (its schema and fixtures are the copy in `docs/parameter-format`, and
every parameter of every example scene is judged by the schema); the round trip (every parameter of a scene set to the
value it holds is accepted and changes nothing, and an address no parameter has is refused); and the scene built by its
own calls. The example scenes are the programs of `docs/examples`, built from synthetic inputs.
"""

import dataclasses
import importlib.util
import json
import re
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


def test_setting_every_parameter_to_its_own_value_is_accepted_and_changes_nothing(scene, same_look):
    parameters = scene.parameters()
    picture = scene.render(96)
    again = scene.with_values(values_of(scene))
    assert again.parameters() == parameters
    assert same_look(again.render(96), picture)
    for address, parameter in parameters.items():
        one = scene.with_values({address: parameter.value})
        assert one.parameters() == parameters, address
        assert same_look(one.render(48), scene.render(48)), address


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


def test_the_headline_slides_in_by_one_address_and_the_ground_stays_as_it_is(same_look):
    module = load("frame_sequence")
    scene = module.build()
    first, last = module.frame(scene, 0, 160), module.frame(scene, module.FRAMES - 1, 160)
    ground = scene.with_values({"headline.opacity": 0}).render(160)
    assert same_look(first, ground)  # at frame 0 the headline is not there
    assert not same_look(first, last)
    assert set(module.frame_values(5)) <= set(scene.parameters())


def test_the_mill_frame_sets_only_addresses_the_scene_lists_and_moves_them(same_look):
    module = load("mill")
    scene = module.build(photo(160, 90), matte(160, 90), matte(160, 90) * 0.5, LOGO)
    rest = module.frame_values(0.0, 0.0, 0.0, 0.0, 0.0, [0, 0, 0, 0])
    busy = module.frame_values(0.9, 0.6, 0.5, 1.0, 1.0, [1, 1, 1, 1])
    assert set(rest) == set(busy) <= set(scene.parameters())
    assert not same_look(module.frame(scene, rest, 64), module.frame(scene, busy, 64))


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


# --- typed handles: a Parameter is accepted where an address is, and a layer hands them out -------------------------


def handles() -> Scene:
    return Scene(12, 8).add(
        Layer.fill("#336699")
        .named("glow")
        .look({"steps": [{"effect": "bloom", "params": {"radius": 6.0}}]})
        .look({"steps": [{"effect": "grain", "params": {"amount": 0.07}}]}),
        Layer.drawing(LOGO).named("logo"),
        Layer.text("Hi", font="bricolage_grotesque", size=6).named("title"),
        Layer.fill("#000000").named("shadow").mask(Layer.fill("#ffffff").place(dx=2), feather=1),
    )


def test_a_parameter_is_accepted_as_a_key_and_equals_the_string_form():
    scene = handles()
    mixed: dict[str | Parameter, float] = {scene.layer("glow").opacity: 0.5, "logo.dx": 3.0}
    by_handle = scene.with_values(mixed)
    assert by_handle == scene.with_values({"glow.opacity": 0.5, "logo.dx": 3.0})
    assert by_handle.parameters()["glow.opacity"].value == 0.5


def test_a_key_that_is_neither_text_nor_addressed_is_a_type_error():
    with pytest.raises(TypeError, match="address"):
        handles().with_values({3: 1.0})  # type: ignore[type-var]  # the type check refuses it too


def test_a_layer_hands_out_the_parameter_scene_lists_for_each_fixed_field():
    scene = handles()
    listed = scene.parameters()
    glow = scene.layer("glow")
    assert glow.opacity.address == "glow.opacity"
    assert glow.opacity == listed["glow.opacity"]
    fields = [
        "opacity",
        "blend",
        "light",
        "dx",
        "dy",
        "scale",
        "aspect",
        "rotate",
        "skew_x",
        "skew_y",
        "flip_x",
        "flip_y",
    ]
    assert [getattr(glow, name) for name in fields] == [listed[f"glow.{name}"] for name in fields]
    assert scene.layer("title").text == listed["title.text"]
    assert glow.color == listed["glow.color"]


def test_runtime_named_parts_are_looked_up_and_are_the_same_parameter():
    scene = handles()
    listed = scene.parameters()
    assert scene.layer("logo").drawing["ring_on"] == listed["logo.drawing.ring_on"]
    assert scene.layer("glow").look()["bloom.radius"] == listed["glow.look.bloom.radius"]
    assert scene.layer("glow").look(2)["grain.amount"] == listed["glow.look#2.grain.amount"]
    assert scene.layer("shadow").mask()["dx"] == listed["shadow.mask.dx"]


def test_an_unknown_layer_is_refused_with_the_nearest_names():
    with pytest.raises(InvalidError, match="did you mean glow"):
        handles().layer("gloww")


def test_a_field_the_layer_kind_lacks_is_refused_with_the_nearest_names():
    scene = handles()
    with pytest.raises(InvalidError, match="no such address"):
        scene.layer("glow").text  # noqa: B018  (a fill has no text)
    with pytest.raises(InvalidError, match=r"did you mean drawing\.ring_on"):
        scene.layer("logo").drawing["ring_n"]
    with pytest.raises(InvalidError, match="no such address"):
        scene.layer("glow").look(3)["bloom.radius"]


# --- a look's colour: a parameter of kind colour ---------------------------------------------------------------------


LAMP = {"steps": [{"effect": "point_light", "params": {"color": "#ffe2b8"}}]}


def lamp() -> Scene:
    return Scene(24, 16).add(
        Layer.fill("#202020").named("ground"),
        Layer.picture(photo(8, 12), maps={"depth": matte(8, 12)}).look(LAMP).named("glow"),
    )


def test_a_looks_colour_is_listed_as_a_colour_and_set_by_text_or_a_triple(same_look):
    scene = lamp()
    listed = scene.parameters()["glow.look.point_light.color"]
    assert (listed.kind, listed.value, listed.default, listed.unit, listed.min, listed.max) == (
        "color",
        "#ffe2b8",
        "#ffe2b8",
        None,
        None,
        None,
    )
    by_hex = scene.with_values({"glow.look.point_light.color": "#336699"}).render(48)
    assert same_look(by_hex, scene.with_values({"glow.look.point_light.color": "#369"}).render(48))
    assert same_look(by_hex, scene.with_values({"glow.look.point_light.color": (0.2, 0.4, 0.6)}).render(48))
    assert same_look(by_hex, scene.with_values({"glow.look.point_light.color": np.array([0.2, 0.4, 0.6])}).render(48))
    assert not same_look(by_hex, scene.render(48))
    assert same_look(
        by_hex,
        Scene(24, 16)
        .add(
            Layer.fill("#202020").named("ground"),
            Layer.picture(photo(8, 12), maps={"depth": matte(8, 12)})
            .look({"steps": [{"effect": "point_light", "params": {"color": "#336699"}}]})
            .named("glow"),
        )
        .render(48),
    )


def test_a_typed_handle_of_a_look_colour_is_a_key_and_a_layers_own_colour_still_works():
    scene = lamp()
    handle = scene.layer("glow").look()["point_light.color"]
    assert handle.kind == "color"
    mixed: dict[str | Parameter, str] = {handle: "#336699", "ground.color": "#ff0000"}
    both = scene.with_values(mixed)
    assert both.parameters()["glow.look.point_light.color"].value == "#336699"
    assert both.parameters()["ground.color"].value == "#ff0000"


def test_a_colour_between_two_levels_of_255_is_listed_as_its_triple():
    value = (
        lamp()
        .with_values({"glow.look.point_light.color": (0.1, 0.25, 0.5)})
        .parameters()["glow.look.point_light.color"]
        .value
    )
    assert isinstance(value, tuple)
    assert value == pytest.approx((0.1, 0.25, 0.5), abs=1e-6)


@pytest.mark.parametrize(
    ("value", "words"),
    [
        ("#12345", "not a colour"),
        ("teal", "'#rrggbb', '#rgb' or [r, g, b]"),
        ((0.2, 1.5, 0.0), "not a colour"),
        (0.5, "a colour"),
        (True, "a colour"),
        (None, "a colour"),
    ],
)
def test_a_bad_look_colour_is_refused_naming_the_address_and_nothing_changes(value, words):
    scene = lamp()
    with pytest.raises(InvalidError, match=rf"glow\.look\.point_light\.color: .*{re.escape(words)}"):
        scene.with_values({"glow.opacity": 0.5, "glow.look.point_light.color": value})
    assert scene.parameters()["glow.opacity"].value == 1.0


def test_a_colour_for_a_number_and_a_triple_of_the_wrong_length_are_refused():
    with pytest.raises(InvalidError, match=r"glow\.look\.point_light\.gain: a number"):
        lamp().with_values({"glow.look.point_light.gain": "#336699"})
    with pytest.raises(InvalidError, match="three numbers"):
        lamp().with_values({"glow.look.point_light.color": (0.2, 0.4)})


def test_an_optional_look_colour_is_none_until_it_is_set_and_then_cannot_be_unset(same_look):
    scene = Scene(24, 16).add(
        Layer.picture(photo(8, 12)).look({"steps": [{"effect": "bloom", "params": {"radius": 4.0}}]}).named("halo")
    )
    listed = scene.parameters()["halo.look.bloom.color"]
    assert (listed.kind, listed.value, listed.default, listed.nullable) == ("color", None, None, True)
    assert same_look(scene.with_values({"halo.look.bloom.color": None}).render(48), scene.render(48))
    tinted = scene.with_values({"halo.look.bloom.color": "#ff0000"})
    assert not same_look(tinted.render(48), scene.render(48))
    with pytest.raises(InvalidError, match="cannot be taken away"):
        tinted.with_values({"halo.look.bloom.color": None})
