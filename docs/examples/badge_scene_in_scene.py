"""A scene used as a layer: a reusable badge.

The badge is built once, with its own layers, and used twice in a poster at different places and sizes with different
words. A scene layer is placed, fitted, rotated, faded and blended as one picture, drawn at the size it has (sharp, not
an enlarged picture), and it is also how things that must move together (a logo and its glow) are kept as one rig.
Proves: `Layer.scene`, group opacity, one scene at two sizes. The words come from building the badge twice
(`badge(word)`): setting the words of one scene by address (`badge_b.scene.label.text`) arrives with parameters (C6).
"""

import json

import bullterrier_scene as bts

W, H = 1200, 800
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


def badge(word: str, color: str = "#ff4b3e") -> bts.Scene:
    """A round badge of 300 by 300 design pixels with a word on it."""
    return bts.Scene(300, 300).add(
        bts.Layer.drawing(disc(144, "#ffffff", 300)).named("ring"),
        bts.Layer.drawing(disc(130, color, 300)).named("disc"),
        bts.Layer.text(word, font="bricolage_grotesque", size=92, color="#ffffff", align="center", axes={"wght": 800})
        .named("label")
        .place(dx=150, dy=182),
    )


def build() -> bts.Scene:
    """A poster with the badge twice: large and tilted, and small with other words and a fade."""
    return bts.Scene(W, H).add(
        bts.Layer.fill("#f2ece0").named("paper"),
        bts.Layer.scene(badge("NEW")).named("badge_a").fit((80, 80, 460, 460), "contain").place(rotate=-12),
        bts.Layer.scene(badge("SALE", "#2f6fdb"))
        .named("badge_b")
        .fit((760, 420, 300, 300), "contain")
        .place(rotate=9)
        .opacity(0.85),
    )


if __name__ == "__main__":
    from bullterrier_image import imageio

    imageio.save(build().render(W), "badges.png")
