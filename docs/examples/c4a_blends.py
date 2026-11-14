"""C4a: every blend mode on the same two layers.

A contact sheet: sixteen tiles, each a colourful backdrop with the same striped source laid over it by one blend mode,
labelled with its CSS name by a text layer. The source has a soft alpha (opaque in the middle, clear at the edge), so
each tile also shows how a mode treats a translucent source. Every picture is made here from arithmetic, so the example
needs nothing but the package. Proves: `Layer.blend` for the sixteen modes, blending where the layer lands (sixteen
source layers on one backdrop picture), `Layer.text` for the labels.

Compare a tile with what a browser's `mix-blend-mode` gives for the same two pictures: the modes are the same W3C set,
blended in sRGB values.
"""

import numpy as np

import bullterrier_scene as bts

TILE_W, TILE_H, GUTTER, LABEL = 200, 150, 20, 30
COLUMNS = ROWS = 4
W = GUTTER + COLUMNS * (TILE_W + GUTTER)
H = GUTTER + ROWS * (TILE_H + LABEL + GUTTER)
FONT = "bricolage_grotesque"
MODES = [
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


def backdrop_tile() -> np.ndarray:
    """A made-up photograph, opaque: hue sweeping across, brightness falling from a bright top, two soft blobs."""
    y, x = np.mgrid[0:TILE_H, 0:TILE_W].astype(np.float32)
    u, v = x / TILE_W, y / TILE_H
    hue = 2.0 * np.pi * (u + 0.15 * v)
    rgb = np.stack([0.5 + 0.5 * np.sin(hue), 0.5 + 0.5 * np.sin(hue + 2.1), 0.5 + 0.5 * np.sin(hue + 4.2)], axis=-1)
    rgb = rgb * (1.0 - 0.55 * v[..., None]) + 0.12
    for cx, cy, tone in [(0.25, 0.7, (0.95, 0.85, 0.3)), (0.78, 0.35, (0.1, 0.2, 0.55))]:
        blob = np.exp(-(((u - cx) / 0.16) ** 2 + ((v - cy) / 0.24) ** 2))[..., None]
        rgb = rgb * (1.0 - blob) + np.array(tone, np.float32) * blob
    return np.clip(rgb, 0.0, 1.0).astype(np.float32)


def source_tile() -> np.ndarray:
    """The layer laid over it, straight RGBA: diagonal stripes of red, white, cyan and a dark blue, opaque in the middle
    and clear at the edge."""
    y, x = np.mgrid[0:TILE_H, 0:TILE_W].astype(np.float32)
    stripe = ((x + y) // 14).astype(int) % 4
    colours = np.array([[0.95, 0.15, 0.1], [0.97, 0.97, 0.9], [0.1, 0.8, 0.9], [0.1, 0.1, 0.35]], np.float32)
    radius = np.hypot((x - TILE_W / 2) / (TILE_W / 2), (y - TILE_H / 2) / (TILE_H / 2))
    alpha = np.clip(1.6 - 1.5 * radius, 0.0, 1.0)
    return np.dstack([colours[stripe], alpha]).astype(np.float32)


def origin(index: int) -> tuple[int, int]:
    """The top left of tile `index` on the sheet, in design pixels."""
    column, row = index % COLUMNS, index // COLUMNS
    return GUTTER + column * (TILE_W + GUTTER), GUTTER + row * (TILE_H + LABEL + GUTTER)


def blend_sheet() -> bts.Scene:
    """The sheet: one backdrop picture with a photograph in each tile, and over each tile the same source blended by one
    mode, named under it."""
    sheet = np.full((H, W, 3), 0.07, np.float32)
    tile = backdrop_tile()
    for index in range(len(MODES)):
        x, y = origin(index)
        sheet[y : y + TILE_H, x : x + TILE_W] = tile
    layers = [bts.Layer.picture(sheet).named("backdrop")]
    source = source_tile()
    for index, mode in enumerate(MODES):
        x, y = origin(index)
        layers.append(bts.Layer.picture(source).named(f"source{index + 1}").place(dx=x, dy=y).blend(mode))
        layers.append(
            bts.Layer.text(mode, font=FONT, size=17, color="#f2ece0", axes={"wght": 600}).place(
                dx=x, dy=y + TILE_H + 21
            )
        )
    return bts.Scene(W, H).add(*layers)


if __name__ == "__main__":
    from bullterrier_image import imageio

    imageio.save(blend_sheet().render(W), "blend_sheet.png")
