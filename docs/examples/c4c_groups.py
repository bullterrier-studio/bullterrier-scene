"""C4c: groups.

Two things a scene used as a layer does. A badge is built once and used twice, at two places and sizes, with other
words: each use is drawn at the size it has, so the small one is as sharp as the large. And a group fades as one
picture: the same two overlapping cards at half opacity, left as one group (the overlap shows the top card at half, over
what is below) and right as two layers each at half (the overlap shows the bottom card through the top one).

Every picture is made here from fills, discs and type, so the example needs nothing but the package.
"""

import json

import bullterrier_scene as bts

W, H = 1200, 800
FONT = "bricolage_grotesque"
PAPER, INK = "#f2ece0", "#1b1830"
KAPPA = 0.5522847498  # the control distance of a cubic that follows a quarter of a circle, in units of the radius


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


def badge(word: str, color: str) -> bts.Scene:
    """A round badge of 300 by 300 design pixels with a word on it."""
    return bts.Scene(300, 300).add(
        bts.Layer.drawing(disc(144, "#ffffff", 300)).named("ring"),
        bts.Layer.drawing(disc(130, color, 300)).named("disc"),
        bts.Layer.text(word, font=FONT, size=92, color="#ffffff", align="center", axes={"wght": 800})
        .named("label")
        .place(dx=150, dy=182),
    )


def cards(box: tuple[int, int, int, int]) -> list[bts.Layer]:
    """Two overlapping opaque cards, the second over the first, laid out as a 300 by 200 picture fitted into `box`."""
    x, y, width, _ = box
    scale = width / 300
    return [
        bts.Layer.fill("#ff9f1c").fit((x, y, 220 * scale, 140 * scale), "stretch"),
        bts.Layer.fill("#2ec4b6").fit((x + 80 * scale, y + 60 * scale, 220 * scale, 140 * scale), "stretch"),
    ]


def caption(words: str, dx: float, dy: float) -> bts.Layer:
    """Small type on the dark band."""
    return bts.Layer.text(words, font=FONT, size=34, color=PAPER, axes={"wght": 600}).place(dx=dx, dy=dy)


def build() -> bts.Scene:
    """The poster: a badge twice above, and below on a dark band the same two cards faded as one and one by one."""
    group = bts.Scene(300, 200).add(*cards((0, 0, 300, 200)))
    return bts.Scene(W, H).add(
        bts.Layer.fill(PAPER).named("paper"),
        bts.Layer.scene(badge("NEW", "#ff4b3e"))
        .named("badge_large")
        .fit((70, 30, 380, 380), "contain")
        .place(rotate=-12),
        bts.Layer.scene(badge("SALE", "#2f6fdb"))
        .named("badge_small")
        .fit((760, 110, 230, 230), "contain")
        .place(rotate=9),
        bts.Layer.fill(INK).named("band").fit((0, 440, W, 360), "stretch"),
        bts.Layer.scene(group).named("faded_as_one").fit((120, 500, 360, 240), "contain").opacity(0.5),
        *[layer.opacity(0.5) for layer in cards((720, 500, 360, 240))],
        caption("faded as one", 120, 775),
        caption("each faded", 720, 775),
    )


if __name__ == "__main__":
    from bullterrier_image import imageio

    imageio.save(build().render(W), "groups.png")
