"""A scene used as a layer: a reusable badge.

The badge is built once, with its own parameters, and used twice in a poster with different words. A scene layer
blends and fades as one group, and it is also how things that must move together (a logo and its glow) are kept as
one rig. Proves: `Layer.scene`, nested addresses (`badge_a.scene.label.text`), group opacity.
"""

import bullterrier_vector as btvec
from bullterrier_image import imageio

import bullterrier_scene as bts

W, H = 1200, 800


def badge() -> bts.Scene:
    """A round badge with a word on it."""
    return bts.Scene(300, 300).add(
        bts.Layer.drawing(
            btvec.Drawing(
                btvec.Path.circle((150, 150), 140).fill("#ff4b3e").stroke("#ffffff", width=8), size=(300, 300)
            )
        ).named("disc"),
        bts.Layer.text("NEW", font="bricolage_grotesque", size=96, color="#ffffff", align="center", axes={"wght": 800})
        .named("label")
        .place(dx=150, dy=185),
    )


def build() -> bts.Scene:
    """A poster with the badge twice."""
    return bts.Scene(W, H).add(
        bts.Layer.fill("#f2ece0").named("paper"),
        bts.Layer.scene(badge()).named("badge_a").fit((80, 80, 360, 360), "contain").place(rotate=-12),
        bts.Layer.scene(badge()).named("badge_b").fit((700, 380, 360, 360), "contain").place(rotate=9).opacity(0.85),
    )


if __name__ == "__main__":
    # Each use has its own copy of the badge's parameters, under the layer that holds it.
    poster = build().with_values({"badge_b.scene.label.text": "SALE", "badge_b.scene.label.color": "#1b1830"})
    imageio.save(poster.render(W), "badges.png")
