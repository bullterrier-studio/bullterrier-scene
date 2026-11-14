"""C5a: looks on every layer.

A look is not only for pictures. Three layers that were never pictures, each shown plain above and with a look below: a
headline (a text layer) with a gradient map and grain, a flat fill with a canvas weave over it, and a badge (a scene
used as a layer) with a two-colour gradient map applied to the finished badge as one picture. Each look runs on what its
layer drew, at the size the layer has, and leaves the layer's alpha as it is (the corners round the badge stay clear).

Every picture is made here from fills, discs and type, so the example needs nothing but the package.
"""

import json

import bullterrier_scene as bts

W, H = 1200, 820
FONT = "bricolage_grotesque"
PAPER, INK, KRAFT = "#ece6da", "#1b1830", "#c9a77c"
KAPPA = 0.5522847498  # the control distance of a cubic that follows a quarter of a circle, in units of the radius
TILE = (320, 260)  # width and height of a tile
COLUMNS = (60, 440, 820)  # left edge of each tile
ROWS = (60, 420)  # top edge of each tile row

# `Recipe([fx.gradient_map(stops=[...]), fx.grain(amount=0.25, size=1.5)]).to_dict()`
HEADLINE_LOOK = {
    "steps": [
        {
            "effect": "gradient_map",
            "params": {"stops": ["#3a1c71", "#d76d77", "#ffaf7b"]},
            "opacity": 1.0,
            "blend_mode": "normal",
        },
        {"effect": "grain", "params": {"amount": 0.25, "size": 1.5}, "opacity": 1.0, "blend_mode": "normal"},
    ]
}
# `Recipe([fx.paper_texture(kind="canvas", amount=0.35, spacing=14)]).to_dict()`
WEAVE_LOOK = {
    "steps": [
        {
            "effect": "paper_texture",
            "params": {"kind": "canvas", "amount": 0.35, "spacing": 14.0},
            "opacity": 1.0,
            "blend_mode": "normal",
        }
    ]
}
# `Recipe([fx.gradient_map(stops=[...])]).to_dict()`: the badge's ring and word (white) take the last stop
BADGE_STOPS = ["#1b1830", "#ffe0b0"]
BADGE_LOOK = {
    "steps": [
        {"effect": "gradient_map", "params": {"stops": BADGE_STOPS}, "opacity": 1.0, "blend_mode": "normal"},
    ]
}


def disc(radius: float, color: str, frame: int) -> str:
    """A drawing document of a disc in the middle of a square frame, in the form vector's `Drawing.to_json()` writes."""
    c, r = frame / 2, radius
    k = KAPPA * r
    path = (
        f"M{c + r} {c} C{c + r} {c + k} {c + k} {c + r} {c} {c + r} C{c - k} {c + r} {c - r} {c + k} {c - r} {c} "
        f"C{c - r} {c - k} {c - k} {c - r} {c} {c - r} C{c + k} {c - r} {c + r} {c - k} {c + r} {c} Z"
    )
    return json.dumps(
        {
            "root": [{"stack": {"items": [[{"path": path}]], "size": [frame, frame]}}, {"fill": {"color": color}}],
            "version": 1,
        }
    )


def badge() -> bts.Scene:
    """A round badge of 300 by 300 design pixels: a white ring, a red disc and a white word."""
    return bts.Scene(300, 300).add(
        bts.Layer.drawing(disc(144, "#ffffff", 300)).named("ring"),
        bts.Layer.drawing(disc(130, "#ff4b3e", 300)).named("disc"),
        bts.Layer.text("LOOK", font=FONT, size=92, color="#ffffff", align="center", axes={"wght": 800})
        .named("label")
        .place(dx=150, dy=182),
    )


def box(column: int, row: int) -> tuple[int, int, int, int]:
    """The box of the tile in `column` and `row`."""
    return (COLUMNS[column], ROWS[row], *TILE)


def headline(row: int, looked: bool) -> list[bts.Layer]:
    """The dark tile with the headline on it; the look is on the headline and not on the tile."""
    x, y, width, height = box(0, row)
    type_ = bts.Layer.text("Look", font=FONT, size=130, color="#bfbfbf", align="center", axes={"wght": 800})
    type_ = type_.named(f"headline{row}").place(dx=x + width / 2, dy=y + 175)
    return [bts.Layer.fill(INK).fit((x, y, width, height), "stretch"), type_.look(HEADLINE_LOOK) if looked else type_]


def weave(row: int, looked: bool) -> list[bts.Layer]:
    """A flat kraft fill fitted into the tile, with the weave look or without it."""
    fill = bts.Layer.fill(KRAFT).named(f"kraft{row}").fit(box(1, row), "stretch")
    return [fill.look(WEAVE_LOOK) if looked else fill]


def badges(row: int, looked: bool) -> list[bts.Layer]:
    """The badge on a paper tile; the look is on the badge as one picture."""
    x, y, width, height = box(2, row)
    group = bts.Layer.scene(badge()).named(f"badge{row}").fit((x + 30, y + 15, width - 60, height - 30), "contain")
    return [
        bts.Layer.fill("#ffffff").fit((x, y, width, height), "stretch"),
        group.look(BADGE_LOOK) if looked else group,
    ]


def caption(words: str, column: int, row: int) -> bts.Layer:
    """Small type under a tile."""
    x, y, width, height = box(column, row)
    return bts.Layer.text(words, font=FONT, size=30, color=INK, align="center", axes={"wght": 600}).place(
        dx=x + width / 2, dy=y + height + 42
    )


def build() -> bts.Scene:
    """The sheet: plain on the top row, the same layers with a look on the bottom row."""
    layers = [bts.Layer.fill(PAPER).named("paper")]
    for row, looked in ((0, False), (1, True)):
        layers += [*headline(row, looked), *weave(row, looked), *badges(row, looked)]
    layers += [
        caption("a headline, plain", 0, 0),
        caption("a fill, plain", 1, 0),
        caption("a badge, plain", 2, 0),
        caption("gradient map and grain", 0, 1),
        caption("a canvas weave", 1, 1),
        caption("a duotone, as one", 2, 1),
    ]
    return bts.Scene(W, H).add(*layers)


if __name__ == "__main__":
    from bullterrier_image import imageio

    imageio.save(build().render(W), "looks_on_everything.png")
