"""The final poster: NIGHT SHIFT.

Layered type, a cut-out subject, neon, halftone, glow on chosen layers, mixed blend modes, a badge, grain and a
vignette. Written at the level of calls: it is the acceptance piece for the whole API. Proves: type behind and in
front of the subject, two glows that light only their own layer, `screen`, `hard-light`, `difference`, `multiply`
and `overlay` in one stack.
"""

from pathlib import Path

import bullterrier_vector as btvec
from bullterrier_image import Recipe, fx, imageio, subject_matte

import bullterrier_scene as bts

W, H = 1800, 2400
FRAME = (0, 0, W, H)
VIOLET, PINK, CYAN, YELLOW = "#12002b", "#ff2bd6", "#00f0ff", "#fff200"

photo = imageio.load("photos/bullterrier.jpg")
matte = subject_matte(photo)
font = btvec.Font.bundled("bricolage_grotesque")


def on_canvas(*items: btvec.Path | btvec.Drawing) -> btvec.Drawing:
    """A vector drawing whose design frame is the poster's canvas."""
    return btvec.Drawing(*items, size=(W, H))


def glow(amount: float) -> Recipe:
    """A bloom for one layer, screened over what is below."""
    return Recipe([fx.bloom(radius=36, threshold=0.3, amount=amount, name="glow")], name="glow")


# Neon rings, a headline outlined in neon, and a second headline filled.
ground = btvec.Path.rect(0, 0, W, H).fill(
    btvec.linear_gradient([(0, VIOLET), (1, "#4b0a5e")], start=(0, 0), end=(0, H))
)
ring_paint = btvec.linear_gradient([(0, PINK), (1, CYAN)], start=(300, 300), end=(1500, 1500))
rings = on_canvas(
    *[btvec.Path.circle((900, 1050), 260 + 150 * k).fill("none").stroke(ring_paint, width=10) for k in range(4)]
)
outline = on_canvas(
    btvec.text("WILD", font=font, size=900, axes={"wght": 800}, anchor=(120, 900)).fill("none").stroke(CYAN, width=8)
)

halftone = Recipe(
    [fx.grade(contrast=1.3), fx.halftone(shape="dot", cell=16, ink=PINK, paper=VIOLET)],
    name="halftone",
)
subject = (
    bts.Layer.picture(photo, maps={"subject": matte})
    .fit((200, 500, 1400, 1800), "contain", anchor=(0.5, 1.0))
    .look(
        Recipe([fx.rim_light(field="subject", color=CYAN, width=14), fx.rgb_split(angle=0, amount=4)], name="subject")
    )
    .mask(matte)
)
rings_layer = bts.Layer.drawing(rings).named("rings")
outline_layer = bts.Layer.drawing(outline).named("outline").mask(subject, invert=True)  # behind the subject
badge = bts.Scene(300, 300).add(
    bts.Layer.drawing(btvec.Drawing(btvec.Path.circle((150, 150), 140).fill(YELLOW), size=(300, 300))).named("disc"),
    bts.Layer.text("LIVE", font="bricolage_grotesque", size=96, color=VIOLET, align="center", axes={"wght": 800})
    .named("label")
    .place(dx=150, dy=185),
)

poster = bts.Scene(W, H).add(
    bts.Layer.drawing(on_canvas(ground)).named("ground"),
    bts.Layer.picture(photo).named("halftone").fit(FRAME, "cover").look(halftone).blend("hard-light").opacity(0.7),
    rings_layer,
    rings_layer.named("rings_glow").look(glow(0.9)).blend("screen"),  # the rings glow; nothing else does
    outline_layer,
    outline_layer.named("outline_glow").look(glow(1.2)).blend("screen"),
    bts.Layer.text("NIGHT\nSHIFT", font="bricolage_grotesque", size=520, color=YELLOW, leading=470, axes={"wght": 800})
    .named("headline")
    .place(dx=140, dy=1700)
    .blend("difference")
    .mask(subject, invert=True),
    subject.named("subject"),
    bts.Layer.scene(badge).named("badge").fit((1280, 140, 360, 360), "contain").place(rotate=-12).opacity(0.9),
    bts.Layer.fill("#808080")
    .named("grain")
    .look(Recipe([fx.grain(amount=0.08, size=1.6)], name="grain"))
    .blend("overlay"),
    bts.Layer.fill("#ffffff")
    .named("vignette")
    .look(Recipe([fx.vignette(amount=0.8, radius=0.45)], name="vignette"))
    .blend("multiply"),
)

out = Path("out")
out.mkdir(exist_ok=True)
imageio.save(poster.render(1800), out / "night-shift.png")
