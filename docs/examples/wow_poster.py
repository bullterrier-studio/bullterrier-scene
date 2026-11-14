"""The final poster: NIGHT SHIFT.

Layered type, a cut-out subject, neon, halftone, glow on chosen layers, mixed blend modes, a badge, grain and a
vignette. One word is outlined in neon and one is filled; both sit in front of the halftone photo and behind the
subject. Written at the level of calls: it is the acceptance piece for the whole API. Proves: type behind the
subject, two glows that light only their own layer (and stay behind the subject), `hard-light`, `difference`,
`screen`, `multiply` and `overlay` in one stack, and the space of a look: the inset's vignette and grain follow
its own box, in design pixels.
"""

from pathlib import Path

import bullterrier_vector as btvec
import numpy as np
from bullterrier_image import Recipe, fx, imageio, subject_matte

import bullterrier_scene as bts

W, H = 1800, 2400
FRAME = (0, 0, W, H)
VIOLET, PINK, CYAN, YELLOW = "#12002b", "#ff2bd6", "#00f0ff", "#fff200"


def on_canvas(*items: btvec.Path | btvec.Drawing) -> btvec.Drawing:
    """A vector drawing whose design frame is the poster's canvas."""
    return btvec.Drawing(*items, size=(W, H))


def glow(amount: float) -> Recipe:
    """A bloom for one layer."""
    return Recipe([fx.bloom(radius=36, threshold=0.3, amount=amount, name="glow")], name="glow")


def build(photo: np.ndarray, matte: np.ndarray) -> bts.Scene:
    """The poster from a photo of the dog and the matte of the dog."""
    font = btvec.Font.bundled("bricolage_grotesque")
    ground = btvec.Path.rect(0, 0, W, H).fill(
        btvec.linear_gradient([(0, VIOLET), (1, "#4b0a5e")], start=(0, 0), end=(0, H))
    )
    ring_paint = btvec.linear_gradient([(0, PINK), (1, CYAN)], start=(300, 300), end=(1500, 1500))
    rings = [btvec.Path.circle((900, 1050), 260 + 150 * k).fill("none").stroke(ring_paint, width=10) for k in range(4)]
    outline = (
        btvec.text("WILD", font=font, size=900, axes={"wght": 800}, anchor=(120, 900))
        .fill("none")
        .stroke(CYAN, width=8)
    )
    halftone = Recipe(
        [fx.grade(contrast=1.3), fx.halftone(shape="dot", cell=16, ink=PINK, paper=VIOLET)], name="halftone"
    )
    on_subject = Recipe(
        [fx.rim_light(field="subject", color=CYAN, width=14), fx.rgb_split(angle=0, amount=4)], name="subject"
    )
    # A look's pixel values are image's design pixels (1/480 of the canvas's long side) and its space is the layer's own
    # fitted box: this inset's vignette is centred on the inset and its grain is as coarse as the photo behind it,
    # whatever the inset's size.
    inset = Recipe([fx.vignette(amount=0.9, radius=0.4), fx.grain(amount=0.08, size=1.6)], name="inset")

    badge = bts.Scene(300, 300).add(
        bts.Layer.drawing(btvec.Drawing(btvec.Path.circle((150, 150), 140).fill(YELLOW), size=(300, 300))).named(
            "disc"
        ),
        bts.Layer.text("LIVE", font="bricolage_grotesque", size=96, color=VIOLET, align="center", axes={"wght": 800})
        .named("label")
        .place(dx=150, dy=185),
    )
    behind_subject = bts.Layer.of("subject")  # a mask: hide what the subject covers

    return bts.Scene(W, H).add(
        bts.Layer.drawing(on_canvas(ground)).named("ground"),
        bts.Layer.picture(photo).named("halftone").fit(FRAME, "cover").look(halftone).blend("hard-light").opacity(0.7),
        bts.Layer.drawing(on_canvas(*rings)).named("rings"),
        bts.Layer.of("rings").named("rings_glow").look(glow(0.9)).as_light().blend("screen"),  # nothing else glows
        bts.Layer.drawing(on_canvas(outline)).named("outline").mask(behind_subject, invert=True),
        bts.Layer.of("outline")
        .named("outline_glow")
        .look(glow(1.2))
        .as_light()
        .blend("screen")
        .mask(behind_subject, invert=True),  # a reference owns its masks: the glow stays behind the subject too
        bts.Layer.text(
            "NIGHT\nSHIFT", font="bricolage_grotesque", size=520, color=YELLOW, leading=470, axes={"wght": 800}
        )
        .named("headline")
        .place(dx=140, dy=1700)
        .blend("difference")
        .mask(behind_subject, invert=True),
        bts.Layer.picture(photo, maps={"subject": matte})
        .named("subject")
        .fit((200, 500, 1400, 1800), "contain", align=(0.5, 1.0))
        .look(on_subject)
        .mask("subject"),
        bts.Layer.picture(photo).named("inset").fit((1280, 1720, 380, 380), "cover").look(inset),
        bts.Layer.scene(badge).named("badge").fit((1280, 140, 360, 360), "contain").place(rotate=-12).opacity(0.9),
        bts.Layer.fill("#808080")
        .named("grain")
        .look(Recipe([fx.grain(amount=0.08, size=1.6)], name="grain"))
        .blend("overlay"),
        # the fill's box is the canvas, so this vignette is the poster's
        bts.Layer.fill("#ffffff")
        .named("vignette")
        .look(Recipe([fx.vignette(amount=0.8, radius=0.45)], name="v"))
        .blend("multiply"),
    )


if __name__ == "__main__":
    picture = imageio.load("photos/bullterrier.jpg")
    out = Path("out")
    out.mkdir(exist_ok=True)
    imageio.save(build(picture, subject_matte(picture)).render(2400), out / "night-shift.png")
