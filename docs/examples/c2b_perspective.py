"""C2b: perspective.

A checker road receding to a horizon under a sky, and two billboards tilted in perspective: one seen from the front,
one from behind (a counter-clockwise quadrilateral is the box seen from behind, a mirror). Every picture is made here
from arithmetic. Proves: `perspective` on a quadrilateral (a trapezoid, a general one, a mirrored one); the near side
drawn sharp and the far side filtered, not aliased, which is what a field of checks smaller than a pixel tests.
"""

import numpy as np

import bullterrier_scene as bts

W, H = 960, 600
HORIZON = 230


def rgb(color: str) -> np.ndarray:
    """A `"#rrggbb"` as a float32 triple."""
    return np.array([int(color[at : at + 2], 16) for at in (1, 3, 5)], np.float32) / 255


def sky() -> np.ndarray:
    """A sky from deep blue at the top to peach at the horizon, and a flat dark ground below it."""
    row = np.clip(np.arange(H, dtype=np.float32) / HORIZON, 0.0, 1.0)[:, None, None]
    picture = rgb("#1d2a5c") * (1 - row) + rgb("#f6b48a") * row
    picture = np.where((np.arange(H)[:, None, None] >= HORIZON), rgb("#2a2433"), picture)
    return np.broadcast_to(picture, (H, W, 3)).astype(np.float32).copy()


def checks(width: int = 1024, height: int = 2560, square: int = 8) -> np.ndarray:
    """A checker of `width` by `height` pixels with squares of `square` pixels, two inks and a line every 16 squares:
    fine enough that the far end of the road is many checks to a pixel, where an unfiltered sample beats."""
    y, x = np.mgrid[0:height, 0:width]
    odd = ((x // square + y // square) % 2).astype(bool)[..., None]
    lines = ((x % (square * 16) < 2) | (y % (square * 16) < 2))[..., None]
    picture = np.where(odd, rgb("#e8e2d0"), rgb("#4b4a58"))
    picture = np.where(lines, rgb("#e0603a"), picture)
    return picture.astype(np.float32)


def poster(text_bars: int = 5, seed: int = 3) -> np.ndarray:
    """A poster, 320 by 220: a bold disc, bars of different lengths for lines of type, and a stripe along the bottom."""
    height, width = 220, 320
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    picture = np.broadcast_to(rgb("#fff4d6"), (height, width, 3)).copy()
    disc = np.hypot(x - 70, y - 70) < 46
    picture[disc] = rgb("#d6342c")
    rng = np.random.default_rng(seed)
    for number in range(text_bars):
        top = 28 + number * 22
        length = int(rng.integers(90, 170))
        picture[top : top + 10, 140 : 140 + length] = rgb("#1d2a5c")
    picture[188:212] = rgb("#1d2a5c")
    picture[196:204, 20:300] = rgb("#fff4d6")
    return picture


def receding_floor() -> bts.Scene:
    """The scene: sky, the road on a trapezoid (a quarter of its width at the horizon), a billboard on a general
    quadrilateral, and a second one on a mirrored quadrilateral."""
    front = [(110, 70), (430, 36), (440, 222), (96, 262)]
    # the same corners with left and right swapped: the poster seen from behind
    behind = [(point[0] + 480, point[1]) for point in (front[1], front[0], front[3], front[2])]
    return bts.Scene(W, H).add(
        bts.Layer.picture(sky()).named("sky"),
        bts.Layer.picture(checks()).named("road").perspective([(400, HORIZON), (560, HORIZON), (1100, H), (-140, H)]),
        bts.Layer.picture(poster()).named("billboard").perspective(front),
        bts.Layer.picture(poster(seed=9))
        .named("billboard_behind")
        .perspective([(x * 0.62 + 290, y * 0.62 + 40) for x, y in behind]),
    )


if __name__ == "__main__":
    from bullterrier_image import imageio

    imageio.save(receding_floor().render(W), "receding_floor.png")
