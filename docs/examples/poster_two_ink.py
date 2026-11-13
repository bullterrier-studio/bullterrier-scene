"""M1: a two-ink poster.

A photo is reduced to two inks and clipped into a polygon. A headline sits in front of the photo and behind the
subject, who is cut out and stands on top. Flat shapes and grain complete it. Proves: look, mask by a drawing,
mask by a named map, an inverted mask by a reference layer (type behind the subject), a blend mode, one picture at
three sizes.
"""

from pathlib import Path

import bullterrier_vector as btvec
import numpy as np
from bullterrier_image import Recipe, fx, imageio, subject_matte

import bullterrier_scene as bts

W, H = 1800, 2400
PAPER, INK, RED = "#f2ece0", "#1b1830", "#ff4b3e"
FRAME = (0, 0, W, H)

two_inks = Recipe(
    [fx.grade(contrast=1.4), fx.dither(method="atkinson", palette="custom", custom=[INK, RED])],
    name="two-ink",
)
grain = Recipe([fx.grain(amount=0.07, size=1.6)], name="grain")


def on_canvas(*items: btvec.Path | btvec.Drawing) -> btvec.Drawing:
    """A vector drawing whose design frame is the poster's canvas."""
    return btvec.Drawing(*items, size=(W, H))


def build(photo: np.ndarray, matte: np.ndarray) -> bts.Scene:
    """The poster from a photo (h, w, 3) and the matte (h, w) of its subject."""
    window = on_canvas(
        btvec.Path.polyline([(260, 300), (1640, 160), (1700, 1900), (140, 2100)], closed=True).fill("#ffffff")
    )
    shapes = on_canvas(btvec.Path.circle((1380, 520), 380).fill(RED), btvec.Path.rect(0, 2000, W, 400).fill(INK))
    dancer = bts.Layer.picture(photo, maps={"subject": matte}).fit(FRAME, "cover").look(two_inks)

    return bts.Scene(W, H).add(
        bts.Layer.fill(PAPER).named("paper"),
        bts.Layer.drawing(shapes).named("shapes"),
        dancer.named("photo").mask(bts.Layer.drawing(window)),  # the photo shows only inside the polygon
        bts.Layer.text(
            "MORE\nLAYERS.", font="bricolage_grotesque", size=620, color=PAPER, leading=560, axes={"wght": 800}
        )
        .named("headline")
        .place(dx=110, dy=700)
        .mask(bts.Layer.of("subject"), invert=True),  # hidden where the subject is: the type passes behind it
        dancer.named("subject").mask("subject"),  # the same picture again, cut out by its own named map
        bts.Layer.fill("#808080").named("grain").look(grain).blend("overlay"),
    )


if __name__ == "__main__":
    picture = imageio.load("photos/dancer.jpg")  # RGB float32 (h, w, 3)
    poster = build(picture, subject_matte(picture))  # the matte: (h, w) float32 0..1, white = the dancer
    out = Path("out")
    out.mkdir(exist_ok=True)
    for size in (600, 1200, 2400):  # the same picture at three sizes
        imageio.save(poster.render(size), out / f"poster-{size}.png")
