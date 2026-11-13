"""C2: placement.

A pile of snapshots on a table, and one tall photo cut three ways into a 9:16 frame. Every picture is made here from
arithmetic (no third-party picture), so the example needs nothing but the package. Proves: `fit` with cover, contain and
stretch and its `align`; `place` with rotate, skew, flip, scale and a move; a placement that acts on the fitted box; a
shadow that is the same placement moved; the delivery crop.
"""

from typing import Any

import numpy as np

import bullterrier_scene as bts

W, H = 1280, 800
FRAME_W, FRAME_H = 720, 1280  # the 9:16 delivery frame
PAPER = "#f4efe6"

# (sky top, sky bottom, sun, far hill, near hill): one palette per snapshot
PALETTES = {
    "dusk": ("#3b2a66", "#f2a65a", "#fff1b8", "#4a3b6b", "#1c1630"),
    "noon": ("#4f9bd9", "#bfe3f5", "#fffbe0", "#5c9a6a", "#2f6b45"),
    "dawn": ("#f7b6a3", "#fde9c8", "#ffffff", "#8d7aa8", "#4d3f6a"),
}


def rgb(color: str) -> np.ndarray:
    """A `"#rrggbb"` as a float32 triple."""
    return np.array([int(color[at : at + 2], 16) for at in (1, 3, 5)], np.float32) / 255


def snapshot(width: int, height: int, palette: str) -> np.ndarray:
    """A made-up photograph, `height` by `width` and opaque: a sky, a sun to the right, two hills and a lit window on
    the left (so that a mirrored print can be told from the original)."""
    top, bottom, sun, far, near = (rgb(color) for color in PALETTES[palette])
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    row = (y / height)[..., None]
    picture = top * (1 - row) + bottom * row
    glow = np.clip(1.0 - np.hypot(x - 0.72 * width, y - 0.3 * height) / (0.12 * min(width, height)), 0.0, 1.0)[
        ..., None
    ]
    picture = picture * (1 - glow) + sun * glow
    horizon = 0.62 * height + 0.05 * height * np.sin(x / (0.15 * width) + 0.6)
    picture = np.where((y > horizon)[..., None], far, picture)
    window = (x > 0.12 * width) & (x < 0.2 * width) & (y > 0.66 * height) & (y < 0.74 * height)
    picture = np.where(window[..., None], rgb("#ffd36b"), picture)
    foreground = 0.78 * height + 0.04 * height * np.sin(x / (0.09 * width) + 1.3)
    picture = np.where((y > foreground)[..., None], near, picture)
    return np.clip(picture, 0.0, 1.0).astype(np.float32)


def table() -> np.ndarray:
    """The table: warm boards, darker toward the corners."""
    y, x = np.mgrid[0:H, 0:W].astype(np.float32)
    boards = 0.5 + 0.5 * np.sin(x / 41.0 + 0.8 * np.sin(y / 230.0))
    light = 1.0 - 0.45 * np.hypot((x - W / 2) / W, (y - H / 2) / H)
    wood = (rgb("#6b4a35") * (0.85 + 0.15 * boards[..., None])) * light[..., None]
    return np.clip(wood, 0.0, 1.0).astype(np.float32)


def print_of(
    name: str,
    photo: np.ndarray,
    frame: tuple[float, float, float, float],
    how: str,
    margin: float = 14,
    **place: Any,
) -> list[bts.Layer]:
    """A shadow, a sheet of paper and the photo `fit` into `frame`, all placed the same way about the frame's middle.

    The paper is the frame grown by `margin`; `how` says how the photo fills the frame ("cover" cuts it, "contain"
    leaves paper around it, "stretch" squashes it); `place` is any of `Layer.place`'s fields.
    """
    x, y, width, height = frame
    paper = (x - margin, y - margin, width + 2 * margin, height + 2 * margin)
    shadow = place | {"dx": place.get("dx", 0) + 8, "dy": place.get("dy", 0) + 12}
    return [
        bts.Layer.fill("#000000").named(f"{name}_shadow").fit(paper, "stretch").place(**shadow).opacity(0.35),
        bts.Layer.fill(PAPER).named(f"{name}_paper").fit(paper, "stretch").place(**place),
        bts.Layer.picture(photo).named(name).fit(frame, how).place(**place),
    ]


def pile_of_prints() -> bts.Scene:
    """Six snapshots on a table, each fitted into its own frame and then turned, skewed, mirrored or scaled about it."""
    dusk, noon, dawn = (snapshot(w, h, p) for w, h, p in [(600, 420, "dusk"), (420, 580, "noon"), (500, 500, "dawn")])
    prints = [
        # a wide photo cut into a tall frame, tilted left
        print_of("cut", dusk, (80, 190, 250, 330), "cover", rotate=-13),
        # a tall photo kept whole in a wide frame: paper shows either side; tilted right
        print_of("whole", noon, (390, 70, 330, 230), "contain", rotate=8),
        # a square squashed into a wide frame and skewed
        print_of("squashed", dawn, (790, 120, 300, 190), "stretch", rotate=3, skew=(10, 0)),
        # the same wide photo mirrored, cut square
        print_of("mirror", dusk, (420, 400, 260, 260), "cover", flip=(True, False), rotate=15),
        # a small one, turned further, on top of the others
        print_of("small", noon, (150, 540, 200, 200), "cover", scale=0.8, rotate=-27, dx=40),
        # a tall one, sheared the other way
        print_of("sheared", dawn, (860, 430, 190, 270), "cover", skew=(0, -9), rotate=-6),
    ]
    layers = [layer for pieces in prints for layer in pieces]
    return bts.Scene(W, H).add(bts.Layer.picture(table()).named("table"), *layers)


def tall_photo() -> np.ndarray:
    """A tall night street, 900 by 2000: a moon on top, lit towers in the middle, a lamp on the road at the bottom."""
    width, height = 900, 2000
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    row = (y / height)[..., None]
    picture = rgb("#101a3c") * (1 - row) + rgb("#6a4f8f") * row
    moon = np.clip(1.0 - np.hypot(x - 0.7 * width, y - 0.1 * height) / (0.07 * width), 0.0, 1.0)[..., None]
    picture = picture * (1 - moon) + rgb("#fffbe0") * moon
    tower = (np.abs(x - 0.3 * width) < 0.16 * width) & (y > 0.3 * height) | (np.abs(x - 0.72 * width) < 0.2 * width) & (
        y > 0.45 * height
    )
    picture = np.where(tower[..., None], rgb("#0b0d1c"), picture)
    lit = tower & (x % 60 < 24) & (y % 90 < 40) & (((x // 60 + y // 90) % 3) > 0)
    picture = np.where(lit[..., None], rgb("#ffcf70"), picture)
    road = y > 0.82 * height
    picture = np.where(road[..., None], rgb("#17151f"), picture)
    lamp = np.clip(1.0 - np.hypot(x - 0.5 * width, (y - 0.9 * height) * 0.6) / (0.2 * width), 0.0, 1.0)[..., None]
    picture = picture + rgb("#ffb347") * lamp * 0.8
    return np.clip(picture, 0.0, 1.0).astype(np.float32)


def delivery_crop(photo: np.ndarray | None = None) -> bts.Scene:
    """One tall photo cut into the 9:16 frame three times, with `align` 0, 0.5 and 1 on the long side: the top, the
    middle and the bottom of the photo stay."""
    photo = tall_photo() if photo is None else photo
    gap = 60
    crops = [
        bts.Layer.picture(photo)
        .named(f"align_{name}")
        .fit((n * (FRAME_W + gap), 0, FRAME_W, FRAME_H), "cover", align=(0.5, align))
        for n, (name, align) in enumerate([("top", 0.0), ("middle", 0.5), ("bottom", 1.0)])
    ]
    return bts.Scene(3 * FRAME_W + 2 * gap, FRAME_H).add(bts.Layer.fill("#1a1a22").named("wall"), *crops)


if __name__ == "__main__":
    from bullterrier_image import imageio

    imageio.save(pile_of_prints().render(W), "pile_of_prints.png")
    imageio.save(delivery_crop().render(1140), "delivery_crop.png")
