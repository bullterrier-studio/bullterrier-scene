"""M1: a two-ink poster.

A photo is reduced to two inks and clipped into a polygon. A headline sits in front of the photo and behind the
subject, who is cut out and stands on top. Flat shapes and grain complete it. Proves: look, mask by a drawing, mask
by a map, an inverted mask (type behind the subject), a blend mode, one picture at three sizes.
"""

from pathlib import Path

import bullterrier_vector as btvec
from bullterrier_image import Recipe, fx, imageio, subject_matte

import bullterrier_scene as bts

W, H = 1800, 2400
PAPER, INK, RED = "#f2ece0", "#1b1830", "#ff4b3e"
FRAME = (0, 0, W, H)

photo = imageio.load("photos/dancer.jpg")  # RGB float32 (h, w, 3)
matte = subject_matte(photo)  # (h, w) float32 0..1, white = the dancer

two_inks = Recipe(
    [fx.grade(contrast=1.4), fx.dither(method="atkinson", palette="custom", custom=[INK, RED])],
    name="two-ink",
)
grain = Recipe([fx.grain(amount=0.07, size=1.6)], name="grain")


def on_canvas(*items: btvec.Path | btvec.Drawing) -> btvec.Drawing:
    """A vector drawing whose design frame is the poster's canvas."""
    return btvec.Drawing(*items, size=(W, H))


window = on_canvas(
    btvec.Path.polyline([(260, 300), (1640, 160), (1700, 1900), (140, 2100)], closed=True).fill("#ffffff")
)
shapes = on_canvas(btvec.Path.circle((1380, 520), 380).fill(RED), btvec.Path.rect(0, 2000, W, 400).fill(INK))

dancer = bts.Layer.picture(photo).fit(FRAME, "cover").look(two_inks)
cutout = dancer.named("subject").mask(matte)  # the matte covers the picture it was made from

poster = bts.Scene(W, H).add(
    bts.Layer.fill(PAPER).named("paper"),
    bts.Layer.drawing(shapes).named("shapes"),
    dancer.named("photo").mask(bts.Layer.drawing(window)),  # the photo shows only inside the polygon
    bts.Layer.text("MORE\nLAYERS.", font="bricolage_grotesque", size=620, color=PAPER, leading=560, axes={"wght": 800})
    .named("headline")
    .place(dx=110, dy=700)
    .mask(cutout, invert=True),  # hidden where the subject is: the type passes behind it
    cutout,
    bts.Layer.fill("#808080").named("grain").look(grain).blend("overlay"),
)

out = Path("out")
out.mkdir(exist_ok=True)
for size in (600, 1200, 2400):  # the same picture at three sizes
    imageio.save(poster.render(size), out / f"poster-{size}.png")
