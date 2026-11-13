"""M2: a composed world.

A cut-out dog stands on a neon grid floor under a striped sun, with a contact shadow, a reflection, and a glow on
the sun only. The sky, sun and floor are vector drawings. The dog is moved and scaled by `fit`. Proves: a subject
standing in a generated world, a shadow and a reflection made from the same layer, one look on one layer, light that
spills past a shape (`screen`).
"""

import bullterrier_vector as btvec
from bullterrier_image import Recipe, fx, imageio, subject_matte

import bullterrier_scene as bts

W, H, HORIZON, FEET = 1600, 900, 520, 700


def world(*items: btvec.Path | btvec.Drawing) -> btvec.Drawing:
    """A vector drawing on the scene's canvas."""
    return btvec.Drawing(*items, size=(W, H))


sky_paint = btvec.linear_gradient([(0, "#12002b"), (1, "#ff2bd6")], start=(0, 0), end=(0, HORIZON))
sun_paint = btvec.linear_gradient([(0, "#fff200"), (1, "#ff2bd6")], start=(0, 140), end=(0, 580))
fade = btvec.linear_gradient([(0, "#ffffff"), (1, "#000000")], start=(0, FEET), end=(0, H))

sun = (
    btvec.Drawing(btvec.Path.circle((800, 360), 220))
    .subtract(
        btvec.Drawing(*[btvec.Path.rect(0, 380 + 34 * k, W, 4 + 3 * k) for k in range(6)])  # stripes thicken downwards
    )
    .fill(sun_paint)
)
rows = [btvec.Path.line((0, HORIZON + 380 * (k / 12) ** 2), (W, HORIZON + 380 * (k / 12) ** 2)) for k in range(1, 13)]
columns = [btvec.Path.line((800, HORIZON), (800 + 260 * (i - 8), H + 40)) for i in range(17)]
grid = [line.stroke("#ff2bd6", width=2) for line in rows + columns]

photo = imageio.load("photos/bullterrier.jpg")
matte = subject_matte(photo)
rim = Recipe([fx.rim_light(field="subject", color="#ff2bd6", width=10)], name="rim")

# The photo ends at the paws, so `anchor=(0.5, 1.0)` stands them on y = FEET.
dog = (
    bts.Layer.picture(photo, maps={"subject": matte})
    .named("dog")
    .fit((560, FEET - 560, 480, 560), "contain", anchor=(0.5, 1.0))
    .look(rim)
    .mask(matte)
)
sun_layer = bts.Layer.drawing(world(sun)).named("sun")

scene = bts.Scene(W, H).add(
    bts.Layer.drawing(world(btvec.Path.rect(0, 0, W, HORIZON).fill(sky_paint))).named("sky"),
    sun_layer,
    sun_layer.named("sun_glow")
    .look(Recipe([fx.bloom(radius=60, threshold=0.2, amount=1.0)], name="glow"))
    .blend("screen"),
    bts.Layer.drawing(world(btvec.Path.rect(0, HORIZON, W, H - HORIZON).fill("#0b0020"), *grid)).named("floor"),
    dog.named("reflection")  # the dog mirrored in the floor, fading away from the paws
    .place(scale=(1, -0.8), anchor=(0.5, 1.0))
    .opacity(0.35)
    .mask(bts.Layer.drawing(world(btvec.Path.rect(0, FEET, W, H - FEET).fill(fade))), use="luminance"),
    bts.Layer.fill("#000000")
    .named("shadow")  # the dog flattened onto the floor and blurred
    .mask(dog.place(scale=(1, 0.12), anchor=(0.5, 1.0)), feather=18)
    .opacity(0.6)
    .blend("multiply"),
    dog,
)
imageio.save(scene.render(1600), "world.png")
