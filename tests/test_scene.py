"""`bullterrier_scene.Scene` through the package's public interface: the canvas, the stack, and the picture it renders.
Expected values are worked out by hand or from the closed form of "over"."""

import numpy as np
import pytest
from hypothesis import given
from hypothesis import strategies as st

import bullterrier_scene as bts
from bullterrier_scene import InvalidError, Layer, Scene


def fills(*colors: str) -> list[Layer]:
    return [Layer.fill(color) for color in colors]


def test_an_empty_scene_renders_a_transparent_float32_array_of_the_size_asked():
    picture = Scene(8, 6).render(16)
    assert picture.dtype == np.float32
    assert picture.shape == (12, 16, 4)
    assert picture.flags.c_contiguous
    assert not picture.any()


@pytest.mark.parametrize(
    ("canvas", "size", "shape"),
    [
        pytest.param((1280, 720), 640, (360, 640, 4), id="16:9"),
        pytest.param((720, 1280), 1280, (1280, 720, 4), id="portrait"),
        pytest.param((100, 100), 10, (10, 10, 4), id="square"),
        pytest.param((1000, 333), 500, (167, 500, 4), id="the short side rounds up"),
        pytest.param((3, 2), 7, (5, 7, 4), id="4.67 rounds up to 5"),
    ],
)
def test_the_long_side_is_the_size_and_the_short_side_keeps_the_aspect_rounded_up(canvas, size, shape):
    assert Scene(*canvas).render(size).shape == shape


def test_an_opaque_layer_is_what_comes_out_and_the_later_layer_wins():
    picture = Scene(2, 2).add(*fills("#ff0000", "#0000ff")).render(2)
    assert np.array_equal(picture, np.broadcast_to(np.array([0, 0, 1, 1], np.float32), (2, 2, 4)))


def test_opacity_mixes_in_straight_alpha_exactly():
    picture = Scene(2, 2).add(Layer.fill("#0000ff"), Layer.fill("#ff0000").opacity(0.5)).render(2)
    assert np.array_equal(picture, np.broadcast_to(np.array([0.5, 0, 0.5, 1], np.float32), (2, 2, 4)))


@given(
    first=st.integers(min_value=1, max_value=255),
    second=st.integers(min_value=1, max_value=255),
)
def test_two_translucent_layers_stack_by_the_closed_form_of_over(first, second):
    scene = Scene(1, 1).add(Layer.fill(f"#ff0000{first:02x}"), Layer.fill(f"#0000ff{second:02x}"))
    a, b = first / 255, second / 255
    alpha = 1 - (1 - a) * (1 - b)
    expected = [a * (1 - b) / alpha, 0, b / alpha, alpha]
    assert np.allclose(scene.render(1)[0, 0], expected, atol=1e-6)


def test_the_array_is_new_each_time_and_changing_it_changes_nothing():
    scene = Scene(2, 2).add(Layer.fill("#336699"))
    first = scene.render(2)
    first[:] = 0
    assert scene.render(2).any()
    assert scene.render(2) is not scene.render(2)


def test_add_returns_a_new_scene_and_the_old_one_is_unchanged():
    before = Scene(2, 2).add(Layer.fill("#ff0000"))
    after = before.add(Layer.fill("#0000ff"))
    assert (len(before.layers), len(after.layers)) == (1, 2)
    assert np.array_equal(before.render(2)[0, 0], [1, 0, 0, 1])
    assert np.array_equal(after.render(2)[0, 0], [0, 0, 1, 1])


def test_a_scene_reports_its_canvas_and_its_layers_bottom_first():
    scene = Scene(1280, 720).add(Layer.fill("#ff0000").named("a"), Layer.fill("#00ff00").named("b"))
    assert (scene.width, scene.height) == (1280, 720)
    assert isinstance(scene.layers, tuple)
    assert all(isinstance(layer, Layer) for layer in scene.layers)
    # the layers are the ones added, in order: a scene of them alone draws the same picture
    assert np.array_equal(Scene(1280, 720).add(*scene.layers).render(16), scene.render(16))


def test_the_same_call_gives_the_same_picture_and_two_scenes_do_not_touch():
    one = Scene(7, 5).add(Layer.fill("#102030"), Layer.fill("#ff8000").opacity(0.3))
    other = Scene(7, 5).add(Layer.fill("#102030"), Layer.fill("#0080ff").opacity(0.3))
    first = one.render(21)
    assert np.array_equal(one.render(21), first)
    assert not np.array_equal(other.render(21), first)
    assert np.array_equal(one.render(21), first)


@pytest.mark.parametrize(
    ("width", "height", "good"),
    [
        (1, 1, True),
        (100_000, 100_000, True),
        (0, 5, False),
        (5, 0, False),
        (-1, 5, False),
        (100_001, 5, False),
        (2**40, 5, False),
        (2**70, 5, False),
    ],
)
def test_a_canvas_is_whole_pixels_from_1_to_100000(width, height, good):
    if good:
        assert Scene(width, height).width == width
    else:
        with pytest.raises(InvalidError, match=r"canvas side is a whole number from 1 to 100000, got"):
            Scene(width, height)


@pytest.mark.parametrize(
    ("size", "good"),
    [(1, True), (64, True), (0, False), (-3, False), (65_001, False), (2**40, False), (2**70, False)],
)
def test_a_size_is_a_whole_number_from_1_to_65000(size, good):
    scene = Scene(4, 4)
    if good:
        assert scene.render(size).shape[0] == size
    else:
        with pytest.raises(InvalidError, match=rf"size is a whole number from 1 to 65000, got {size}"):
            scene.render(size)


def test_a_render_of_more_than_2_pow_27_pixels_is_refused_before_it_is_drawn():
    with pytest.raises(InvalidError, match=r"size.*134217728"):
        Scene(16_385, 8_192).render(16_385)


@pytest.mark.parametrize("bad", ["3", 3.5, None], ids=["text", "float", "None"])
def test_a_wrong_type_is_a_type_error_not_a_library_error(bad):
    with pytest.raises(TypeError):
        Scene(bad, 3)
    with pytest.raises(TypeError):
        Scene(3, 3).render(bad)


def test_add_takes_layers_only():
    with pytest.raises(TypeError, match="Scene"):
        Scene(2, 2).add(Scene(2, 2))  # type: ignore[arg-type]  # a wrong type, on purpose
    with pytest.raises(TypeError, match="str"):
        Scene(2, 2).add("#ff0000")  # type: ignore[arg-type]  # a wrong type, on purpose


def test_a_name_used_twice_is_refused_naming_both_layers():
    with pytest.raises(InvalidError, match=r"ground.*layer 1.*layer 3"):
        Scene(2, 2).add(
            Layer.fill("#000000").named("ground"), Layer.fill("#111111"), Layer.fill("#222222").named("ground")
        )
    with pytest.raises(InvalidError, match="ground"):
        Scene(2, 2).add(Layer.fill("#000000").named("ground")).add(Layer.fill("#111111").named("ground"))


def test_a_scene_holds_at_most_max_layers_layers():
    layers = [Layer.fill("#00000001") for _ in range(bts.MAX_LAYERS)]
    assert len(Scene(2, 2).add(*layers).layers) == bts.MAX_LAYERS
    with pytest.raises(InvalidError, match=str(bts.MAX_LAYERS + 1)):
        Scene(2, 2).add(*layers, Layer.fill("#00000001"))


def test_a_layer_drawn_larger_than_2_pow_27_pixels_is_refused_naming_the_layer_and_its_size_when_a_look_runs_on_it():
    # 60 design pixels at 200 output pixels each: 12,000 by 12,000, on a canvas that itself renders at 1,200 by 1,200
    photo = Layer.picture(np.full((60, 60, 3), 0.5, np.float32)).named("photo")
    # a picture with no look hands its own pixels to the warp: nothing is drawn at that size
    assert Scene(6, 6).add(photo).render(1200).shape == (1200, 1200, 4)
    scene = Scene(6, 6).add(photo.look({"steps": [{"effect": "grade", "params": {"exposure": 0.5}}]}))
    with pytest.raises(InvalidError, match=r"photo.*12000 x 12000.*134217728"):
        scene.render(1200)


# --- references -------------------------------------------------------------------------------------------------------


def swatch(color: str) -> Layer:
    return Layer.fill(color).fit((1, 1, 2, 2), "stretch")


def test_a_reference_draws_the_layer_it_names_again_with_a_join_of_its_own():
    reference = Layer.of("a").place(dx=2).opacity(0.5)
    scene = Scene(4, 4).add(swatch("#ff0000").named("a"), reference)
    copy = Scene(4, 4).add(swatch("#ff0000").named("a"), swatch("#ff0000").place(dx=2).opacity(0.5))
    assert np.array_equal(scene.render(4), copy.render(4))


def test_a_reference_is_stacked_where_it_is_written_whichever_order_the_call_resolves_it_in():
    reference = Layer.of("a").place(dx=1).blend("difference")
    below = Scene(4, 4).add(reference, swatch("#ff0000").named("a")).render(4)
    above = Scene(4, 4).add(swatch("#ff0000").named("a"), reference).render(4)
    # at column 2 both cover: below, the layer it names is drawn over it; above, the difference of red with red is black
    assert below[2, 2].tolist() == [1, 0, 0, 1]
    assert above[2, 2].tolist() == [0, 0, 0, 1]


def test_a_reference_is_an_invalid_error_where_it_names_nothing_or_leads_back_to_itself():
    with pytest.raises(InvalidError, match=r"glow\.of.*nobody"):
        Scene(4, 4).add(Layer.of("nobody").named("glow"))
    with pytest.raises(InvalidError, match="a -> b -> a"):
        Scene(4, 4).add(Layer.of("b").named("a"), Layer.of("a").named("b"))
    with pytest.raises(InvalidError, match="a -> b -> a"):
        Scene(4, 4).add(
            Layer.fill("#000000").named("a").mask(Layer.of("b")), Layer.fill("#000000").named("b").mask(Layer.of("a"))
        )


@pytest.mark.parametrize(
    "call",
    [
        pytest.param(lambda layer: layer.fit((0, 0, 2, 2)), id="fit"),
        pytest.param(lambda layer: layer.perspective([(0, 0), (4, 0), (4, 4), (0, 4)]), id="perspective"),
    ],
)
def test_a_reference_has_no_fit_and_no_perspective(call):
    with pytest.raises(InvalidError, match=r"has no (fit|perspective) of its own"):
        call(Layer.of("a"))


def test_a_reference_names_a_layer_with_text():
    with pytest.raises(TypeError, match="name"):
        Layer.of(3)  # type: ignore[arg-type]  # the test is of the wrong type


def test_a_reference_in_a_scene_keeps_what_was_written_in_layers():
    scene = Scene(4, 4).add(swatch("#ff0000").named("a"), Layer.of("a"))
    assert len(scene.layers) == 2


def test_a_scene_as_a_layer_is_drawn_as_the_scene_and_fades_as_one_picture():
    inner = Scene(4, 2).add(
        Layer.fill("#ff0000").fit((0, 0, 2, 2), "stretch"), Layer.fill("#0000ff").fit((2, 0, 2, 2), "stretch")
    )
    assert np.array_equal(Scene(4, 2).add(Layer.scene(inner)).render(4), inner.render(4))
    # a group at 0.5 over white is each child at 0.5 over white, once (the isolation oracle is in the Rust tests)
    faded = Scene(4, 2).add(Layer.fill("#ffffff"), Layer.scene(inner).opacity(0.5)).render(4)
    assert faded[0, 0].tolist() == [1.0, 0.5, 0.5, 1.0]
    assert faded[0, 3].tolist() == [0.5, 0.5, 1.0, 1.0]


def test_a_scene_as_a_layer_counts_its_layers_and_its_depth_at_each_use():
    inner = Scene(2, 2).add(*[swatch("#00000001") for _ in range(600)])
    with pytest.raises(InvalidError, match="1202"):
        Scene(2, 2).add(Layer.scene(inner), Layer.scene(inner))
    nested = Scene(2, 2).add(swatch("#000000"))
    for _ in range(bts.MAX_DEPTH):
        nested = Scene(2, 2).add(Layer.scene(nested))
    with pytest.raises(InvalidError, match="17"):
        Scene(2, 2).add(Layer.scene(nested))


def test_a_scene_as_a_layer_takes_a_scene():
    with pytest.raises(TypeError, match="Scene"):
        Layer.scene("a scene")  # type: ignore[arg-type]  # a wrong type, on purpose
    with pytest.raises(TypeError, match="Scene"):
        Layer.scene(swatch("#ff0000"))  # type: ignore[arg-type]  # a wrong type, on purpose
    with pytest.raises(TypeError, match=r"Layer\.scene"):
        Scene(2, 2).add(Scene(2, 2))  # type: ignore[arg-type]  # a wrong type, on purpose
