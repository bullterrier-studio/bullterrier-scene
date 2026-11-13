"""M2: a composed world.

A cut-out dog stands on a neon grid floor under a striped sun, with a contact shadow, a reflection, and a glow on
the sun only. The sky, sun and floor are vector drawings. The dog is moved and scaled by `fit`; its shadow,
reflection and the sun's glow are reference layers (`Layer.of`), so nothing is drawn twice by hand and every value
is set once. Proves: a subject standing in a generated world, one look on one layer, light that spills past a
shape (`alpha("light")`).
"""

import bullterrier_vector as btvec
import numpy as np
from bullterrier_image import Recipe, fx, imageio, subject_matte

import bullterrier_scene as bts

W, H, HORIZON, FEET = 1600, 900, 520, 700
SQUASH = 0.12  # the shadow's height as a share of the dog's


def world(*items: btvec.Path | btvec.Drawing) -> btvec.Drawing:
    """A vector drawing on the scene's canvas."""
    return btvec.Drawing(*items, size=(W, H))


def build(photo: np.ndarray, matte: np.ndarray) -> bts.Scene:
    """The scene from a photo of the dog (it ends at the paws) and the matte of the dog."""
    sky_paint = btvec.linear_gradient([(0, "#12002b"), (1, "#ff2bd6")], start=(0, 0), end=(0, HORIZON))
    sun_paint = btvec.linear_gradient([(0, "#fff200"), (1, "#ff2bd6")], start=(0, 140), end=(0, 580))
    fade = btvec.linear_gradient([(0, "#ffffff"), (1, "#000000")], start=(0, FEET), end=(0, H))

    stripes = btvec.Drawing(*[btvec.Path.rect(0, 380 + 34 * k, W, 4 + 3 * k) for k in range(6)])  # thicken downwards
    sun = btvec.Drawing(btvec.Path.circle((800, 360), 220)).subtract(stripes).fill(sun_paint)
    rows = [
        btvec.Path.line((0, HORIZON + 380 * (k / 12) ** 2), (W, HORIZON + 380 * (k / 12) ** 2)) for k in range(1, 13)
    ]
    columns = [btvec.Path.line((800, HORIZON), (800 + 260 * (i - 8), H + 40)) for i in range(17)]
    grid = [line.stroke("#ff2bd6", width=2) for line in rows + columns]

    rim = Recipe([fx.rim_light(field="subject", color="#ff2bd6", width=10)], name="rim")
    glow = Recipe([fx.bloom(radius=60, threshold=0.2, amount=1.0)], name="glow")
    # `anchor=(0.5, 1.0)` on the dog stands its paws on y = FEET; the reflection and shadow turn about the same point.
    dog = (
        bts.Layer.picture(photo, maps={"subject": matte})
        .named("dog")
        .fit((560, FEET - 560, 480, 560), "contain", align=(0.5, 1.0))
        .look(rim)
        .mask("subject")  # cut out by its own map, after the look
    )
    fade_out = bts.Layer.drawing(world(btvec.Path.rect(0, FEET, W, H - FEET).fill(fade)))

    return bts.Scene(W, H).add(
        bts.Layer.drawing(world(btvec.Path.rect(0, 0, W, HORIZON).fill(sky_paint))).named("sky"),
        bts.Layer.drawing(world(sun)).named("sun"),
        bts.Layer.of("sun").named("sun_glow").look(glow).alpha("light").blend("screen"),  # the sun glows, nothing else
        bts.Layer.drawing(world(btvec.Path.rect(0, HORIZON, W, H - HORIZON).fill("#0b0020"), *grid)).named("floor"),
        bts.Layer.of("dog")  # the dog mirrored in the floor, fading away from the paws
        .named("reflection")
        .place(flip=(False, True), anchor=(0.5, 1.0))
        .opacity(0.35)
        .mask(fade_out, use="luminance"),
        bts.Layer.fill("#000000")  # the dog's outline flattened onto the floor and softened
        .named("shadow")
        .mask(bts.Layer.of("dog").place(scale=SQUASH, aspect=1 / SQUASH, anchor=(0.5, 1.0)), feather=12)
        .opacity(0.6)
        .blend("multiply"),
        dog,
    )


if __name__ == "__main__":
    picture = imageio.load("photos/bullterrier.jpg")
    imageio.save(build(picture, subject_matte(picture)).render(1600), "world.png")
