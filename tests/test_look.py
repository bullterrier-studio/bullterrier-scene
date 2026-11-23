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


def test_the_example_renders_the_same_array_twice_and_at_other_sizes(example, same_look):
    scene = example.build(example.synthetic_photo())
    assert same_look(scene.render(320), scene.render(320))
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


def test_the_pile_is_the_same_picture_twice_and_at_two_sizes_and_the_table_shows_around_it(prints, same_look):
    scene = prints.pile_of_prints()
    first = scene.render(400)
    assert first.shape == (250, 400, 4)
    assert same_look(first, scene.render(400))
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


# --- C2b: the receding road and the billboards ------------------------------------------------------------------------


@pytest.fixture(scope="module")
def perspective() -> ModuleType:
    return load("c2b_perspective", ROOT / "docs" / "examples" / "c2b_perspective.py")


def test_the_receding_floor_is_the_same_picture_twice_and_at_two_sizes(perspective, same_look):
    scene = perspective.receding_floor()
    first = scene.render(480)
    assert first.shape == (300, 480, 4)
    assert same_look(first, scene.render(480))
    assert (first[..., 3] == 1).all()  # the sky and the ground are opaque everywhere
    assert scene.render(240).shape == (150, 240, 4)


def test_the_road_fades_to_grey_toward_the_horizon_where_its_checks_are_smaller_than_a_pixel(perspective):
    drawn = perspective.receding_floor().render(perspective.W)
    middle = slice(450, 510)
    far = drawn[240:250, middle, 0]  # just under the horizon: the checks are a tenth of a pixel
    near = drawn[560:570, middle, 0]  # a check is several pixels
    assert far.std() < 0.05
    assert near.std() > 0.15
    # and the far road is the mean of the two inks, not one of them
    inks = [perspective.rgb("#e8e2d0")[0], perspective.rgb("#4b4a58")[0]]
    assert abs(far.mean() - np.mean(inks)) < 0.1


def test_the_billboard_seen_from_behind_is_the_poster_mirrored(perspective):
    # a flat quadrilateral each way, so that the mirror is exact: the poster, and the poster on the mirrored corners
    poster = perspective.poster()
    height, width = poster.shape[:2]
    front = bts.Scene(width, height).add(
        bts.Layer.picture(poster).perspective([(0, 0), (width, 0), (width, height), (0, height)])
    )
    behind = bts.Scene(width, height).add(
        bts.Layer.picture(poster).perspective([(width, 0), (0, 0), (0, height), (width, height)])
    )
    assert np.array_equal(front.render(width)[..., :3], poster)
    assert np.array_equal(behind.render(width)[..., :3], poster[:, ::-1])


# --- C3: letters filled with a landscape, type behind a figure -----------------------------------------------------


@pytest.fixture(scope="module")
def clip() -> ModuleType:
    return load("c3_clip", ROOT / "docs" / "examples" / "c3_clip.py")


def test_the_clipped_type_is_the_same_picture_twice_and_at_two_sizes(clip, same_look):
    scene = clip.clipped_type()
    first = scene.render(600)
    assert first.shape == (400, 600, 4)
    assert same_look(first, scene.render(600))
    assert (first[..., 3] == 1).all()  # the ground is opaque everywhere
    assert scene.render(300).shape == (200, 300, 4)


def test_the_figure_stands_in_front_of_the_type_and_the_type_shows_either_side_of_it(clip):
    drawn = clip.clipped_type().render(clip.W)
    row = 610  # inside the line BEHIND (it stands on y = 680 and is 250 high)
    # the type is pale (the vignette dims it a little near the edge) where the figure is not: its first letter
    assert (drawn[row, 150:330, :3].min(axis=-1) > 0.6).sum() > 40
    # the figure is red where its matte is 1, on the same row: the type does not show through it
    _, matte = clip.figure()
    left = clip.W // 2 - matte.shape[1] // 2
    inside = np.where(matte[row - (clip.H - 30 - matte.shape[0])] > 0.99)[0] + left
    assert len(inside) > 20
    figure = drawn[row, inside, :3]
    assert (figure[:, 0] > 2 * figure[:, 1]).all()


def test_the_landscape_shows_only_in_the_letters_and_nowhere_else_above_the_figure(clip):
    drawn = clip.clipped_type().render(clip.W)
    ground = clip.rgb(clip.GROUND)
    band = drawn[40:370, 150:1050, :3]
    differs = np.abs(band - ground).max(axis=-1) > 0.1
    assert 0.1 < differs.mean() < 0.5  # letters, not a picture over the whole band
    # the sun of the landscape (it is orange) is in the band, in a letter
    assert (band[..., 0] - band[..., 2] > 0.35).any()


# --- C4a: every blend mode on the same two layers ----------------------------------------------------------------


@pytest.fixture(scope="module")
def blends() -> ModuleType:
    return load("c4a_blends", ROOT / "docs" / "examples" / "c4a_blends.py")


def test_the_sheet_is_the_same_picture_twice_and_each_tile_is_the_same_backdrop_under_its_own_mode(blends, same_look):
    sheet = blends.blend_sheet().render(blends.W)
    assert sheet.shape == (blends.H, blends.W, 4)
    assert same_look(sheet, blends.blend_sheet().render(blends.W))
    # the corner of each tile is clear in the source (alpha 0 at the edge), so it shows the backdrop whatever the mode
    backdrop = blends.backdrop_tile()
    for index in range(len(blends.MODES)):
        x, y = blends.origin(index)
        assert np.allclose(sheet[y, x, :3], backdrop[0, 0], atol=1e-6), blends.MODES[index]


def test_a_tile_is_the_w3c_formula_of_its_mode_at_the_alpha_of_the_source(blends):
    sheet = blends.blend_sheet().render(blends.W)
    backdrop, source = blends.backdrop_tile(), blends.source_tile()
    row, column = 75, 100  # the middle of the tile: opaque
    cb, cs = backdrop[row, column], source[row, column, :3]
    assert source[row, column, 3] == 1
    for mode, expected in [
        ("normal", cs),
        ("multiply", cb * cs),
        ("screen", cb + cs - cb * cs),
        ("difference", abs(cb - cs)),
    ]:
        x, y = blends.origin(blends.MODES.index(mode))
        assert np.allclose(sheet[y + row, x + column, :3], expected, atol=1e-5), mode
    # a translucent pixel of the source: (1 - a) cb + a B(cb, cs), by multiply
    row, column = 75, 20
    a = source[row, column, 3]
    assert 0.05 < a < 0.95
    x, y = blends.origin(blends.MODES.index("multiply"))
    expected = (1 - a) * backdrop[row, column] + a * backdrop[row, column] * source[row, column, :3]
    assert np.allclose(sheet[y + row, x + column, :3], expected, atol=1e-5)


# --- C4b: references: a glowing sun, a figure with its shadow and reflection, a headline behind it -----------------


@pytest.fixture(scope="module")
def evening() -> ModuleType:
    return load("c4b_references", ROOT / "docs" / "examples" / "c4b_references.py")


def luminance(pixels: np.ndarray) -> np.ndarray:
    return pixels[..., :3] @ np.array([0.2126, 0.7152, 0.0722], np.float32)


def test_the_evening_is_the_same_opaque_picture_twice_and_at_two_sizes(evening, same_look):
    scene = evening.build()
    first = scene.render(600)
    assert first.shape == (400, 600, 4)
    assert same_look(first, scene.render(600))
    assert (first[..., 3] == 1).all()  # the floor is opaque everywhere
    assert scene.render(300).shape == (200, 300, 4)


def test_the_suns_glow_is_light_round_the_disc_that_fades_with_the_distance_and_spills_past_its_box(evening):
    drawn = evening.build().render(evening.W)
    # the sun's picture is the box (620, 40, 500, 500), its disc 110 pixels across from its middle (870, 290)
    row = 290
    ring, further, far = (luminance(drawn[row, x]) for x in (870 + 125, 870 + 175, 1150))
    assert ring > further > far + 0.01  # the glow brightens the sky it reaches, less with the distance
    assert (
        luminance(drawn[row, 1150]) > luminance(drawn[row, 20]) + 0.005
    )  # past the sun's box (C5c room), where a light layer shows it


def test_the_shadow_is_cast_along_the_floor_from_the_figure_and_the_floor_is_clear_elsewhere(evening):
    drawn = evening.build().render(evening.W)
    floor = evening.rgb("#4a3772")
    # the shadow is the figure flattened to 0.18 of its height and skewed 55 degrees, so it falls to the figure's right,
    # over the rows just above its feet (the figure itself covers the rest of it)
    beside = drawn[evening.FEET - 90 : evening.FEET - 60, 770:830, :3]
    assert luminance(beside).min() < 0.7 * luminance(floor)
    assert np.allclose(drawn[evening.FEET - 40, 20, :3], floor, atol=1e-6)  # far from the sun's glow


def test_the_reflection_is_the_figure_mirrored_in_its_feet_and_faded(evening):
    drawn = evening.build().render(evening.W)
    skin, matte = evening.figure()
    column, below = 600, 40  # the middle of the figure; 40 rows under its feet
    source = evening.FEET - 40 - (evening.FEET - 520)  # the row of the figure that lands there
    fade = 1.0 - below / 220.0
    alpha = 0.4 * fade * matte[source, column - (evening.W // 2 - 180)]
    expected = evening.rgb("#4a3772") * (1 - alpha) + skin[source, column - (evening.W // 2 - 180)] * alpha
    assert np.allclose(drawn[evening.FEET + below, column, :3], expected, atol=2e-3)


def test_the_headline_passes_behind_the_figure_and_shows_either_side_of_it(evening):
    drawn = evening.build().render(evening.W)
    skin, _ = evening.figure()
    paper = evening.rgb(evening.PAPER)
    row = 560
    inside = drawn[row, 600, :3]  # the body of the figure, where the type is hidden
    assert np.allclose(inside, skin[row - (evening.FEET - 520), 600 - (evening.W // 2 - 180)], atol=1e-6)
    assert (np.abs(drawn[row, :, :3] - paper).max(axis=-1) < 1e-3).sum() > 150  # the letters either side


# --- C4c: groups: a badge twice, a pair of cards faded as one and one by one -----------------------------------------


@pytest.fixture(scope="module")
def groups() -> ModuleType:
    return load("c4c_groups", ROOT / "docs" / "examples" / "c4c_groups.py")


def level(color: str) -> np.ndarray:
    """A `"#rrggbb"` as a float32 triple."""
    return np.array([int(color[at : at + 2], 16) for at in (1, 3, 5)], np.float32) / 255


def test_a_group_fades_as_one_picture_and_the_layers_of_the_other_pair_each_fade(groups):
    drawn = groups.build().render(groups.W)
    band, bottom, top = level(groups.INK), level("#ff9f1c"), level("#2ec4b6")
    # the pairs are 360 by 240 boxes at (120, 500) and (720, 500); the cards overlap at x 96 to 264, y 72 to 168 in them
    for left, overlap, bottom_only in ((120, 0.5 * top + 0.5 * band, 0.5 * bottom + 0.5 * band),):
        assert np.allclose(drawn[620, left + 180, :3], overlap, atol=2e-5)
        assert np.allclose(drawn[520, left + 30, :3], bottom_only, atol=2e-5)
    # the same cards as two layers each at half: the bottom card shows through the top one
    assert np.allclose(drawn[620, 720 + 180, :3], 0.5 * top + 0.25 * bottom + 0.25 * band, atol=2e-5)
    assert np.allclose(drawn[520, 720 + 30, :3], 0.5 * bottom + 0.5 * band, atol=2e-5)


def test_the_badge_is_drawn_at_the_size_each_use_has(groups, same_look):
    scene = groups.build()
    first = scene.render(groups.W)
    assert first.shape == (groups.H, groups.W, 4)
    assert same_look(first, scene.render(groups.W))
    assert (first[..., 3] == 1).all()  # the paper is opaque everywhere
    # above the word, inside each badge's disc: the large one (a box at (70, 30), 380 across) is red, the small one blue
    assert np.allclose(first[110, 260, :3], level("#ff4b3e"), atol=1e-5)
    assert np.allclose(first[150, 875, :3], level("#2f6fdb"), atol=1e-5)


def test_the_badge_example_renders(tmp_path):
    badge = load("badge_scene_in_scene", ROOT / "docs" / "examples" / "badge_scene_in_scene.py")
    drawn = badge.build().render(300)
    assert drawn.shape == (200, 300, 4)
    assert 0.0 <= drawn.min() <= drawn.max() <= 1.0


# --- C5a: looks on every layer: a headline, a fill and a badge, plain above and looked below -------------------------


@pytest.fixture(scope="module")
def everything() -> ModuleType:
    return load("c5a_looks_on_everything", ROOT / "docs" / "examples" / "c5a_looks_on_everything.py")


def test_a_look_changes_only_the_layer_it_is_on_and_keeps_the_alpha_of_a_badge(everything):
    drawn = everything.build().render(everything.W)
    drop = everything.ROWS[1] - everything.ROWS[0]
    ground, cream = level(everything.INK), level(everything.BADGE_STOPS[1])
    # the dark tile behind the headline, and the white tile behind the badge, are not looked at: the same in both rows
    assert np.allclose(drawn[430, 70, :3], ground, atol=1e-5)
    assert np.allclose(drawn[430 - drop, 70, :3], ground, atol=1e-5)
    # the badge's ring is white plain and the last stop of the gradient map looked; the clear corner of the badge shows
    # the white tile in both rows (the look kept the group's alpha)
    ring_x, ring_y = 1085, 550
    assert np.allclose(drawn[ring_y - drop, ring_x, :3], 1.0, atol=1e-5)
    assert np.allclose(drawn[ring_y, ring_x, :3], cream, atol=2e-3)
    assert np.allclose(drawn[440, 870, :3], 1.0, atol=1e-5)
    assert np.allclose(drawn[440 - drop, 870, :3], 1.0, atol=1e-5)


def test_the_weave_look_texturizes_a_flat_fill_and_keeps_its_brightness(everything):
    drawn = everything.build().render(everything.W)
    x, y, width, height = everything.box(1, 0)
    plain = drawn[y + 20 : y + height - 20, x + 20 : x + width - 20, :3]
    x, y, width, height = everything.box(1, 1)
    looked = drawn[y + 20 : y + height - 20, x + 20 : x + width - 20, :3]
    assert plain.std(axis=(0, 1)).max() < 1e-3  # a flat fill is flat, in each channel
    assert looked.std(axis=(0, 1)).min() > 0.02  # and the weave is not
    assert np.abs(looked.mean(axis=(0, 1)) - plain.mean(axis=(0, 1))).max() < 0.05  # it keeps the average brightness


# --- C6a: one scene, six covers --------------------------------------------------------------------------------------


@pytest.fixture(scope="module")
def six_covers() -> ModuleType:
    return load("c6a_covers", ROOT / "docs" / "examples" / "c6a_covers.py")


def test_six_covers_come_from_one_template_that_is_left_a_hole(six_covers):
    template = six_covers.build()
    with pytest.raises(bts.InvalidError, match=r"photo\.picture"):
        template.render(six_covers.W)  # the template holds no photo
    covers = six_covers.covers()
    assert len(covers) == 6
    assert len({cover.tobytes() for cover in covers}) == 6
    assert template.parameters()["photo.picture"].value is None  # and is still a hole after six covers were made


def test_a_cover_shows_its_accent_over_the_ground_where_the_photo_was_moved_away(six_covers):
    # the first cover moves its photo 40 pixels left, so the right edge of the band lies over the ground alone: the band
    # is its accent at 0.9 of the ground's, by the closed form of "over"
    cover = six_covers.covers()[0]
    accent = np.array([0xC2, 0x41, 0x0C], np.float32) / 255
    ground = np.array([0x14, 0x11, 0x0F], np.float32) / 255
    assert np.allclose(cover[150, 245, :3], 0.9 * accent + 0.1 * ground, atol=1e-5)
    assert cover[150, 245, 3] == 1  # a cover is opaque
