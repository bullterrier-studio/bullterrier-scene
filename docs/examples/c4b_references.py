"""C4b: references.

A made-up evening: a sun that glows, a figure that stands on a floor, and a headline that passes behind it. Nothing is
drawn twice by hand: the glow, the shadow, the reflection and the headline's cut are all `Layer.of`, a layer that
draws a named layer of the scene again. Proves: a reference that adds a look and is made light (the sun's glow on a
PICTURE layer), written below the layer it names (a call finds the name wherever it is, and the glow stays behind the
sun); a reference with its own place and a mask (a shadow flattened and cast along the floor from the figure, a
reflection mirrored below its feet); a reference as a mask (the headline hidden where the figure is); and that all of
them follow the layers they name.

Every picture is made here from arithmetic, so the example needs nothing but the package. The sun is an opaque picture
whose sky is black and which is joined by `screen`: a look runs on a picture's colour over black, and a light reference
shows the halo that the look adds past the disc. The look grows no room past the picture's box until C5, so the sun's
picture has room around the disc.
"""

import numpy as np

import bullterrier_scene as bts

W, H = 1200, 800
HORIZON, FEET = 500, 700
FONT = "bricolage_grotesque"
PAPER = "#f2ece0"
# `Recipe([fx.bloom(radius=45, threshold=0.2, amount=2.5)]).to_dict()`: light bleeding from the sun's disc
GLOW = {
    "steps": [
        {
            "effect": "bloom",
            "params": {"radius": 45.0, "threshold": 0.2, "amount": 2.5},
            "opacity": 1.0,
            "blend_mode": "normal",
        }
    ]
}


def rgb(color: str) -> np.ndarray:
    """A `"#rrggbb"` as a float32 triple."""
    return np.array([int(color[at : at + 2], 16) for at in (1, 3, 5)], np.float32) / 255


def sky() -> np.ndarray:
    """A made-up sky, opaque: violet at the top, orange at the horizon (one row of colour for each row)."""
    row = np.linspace(0.0, 1.0, HORIZON, dtype=np.float32)[:, None, None]
    return np.broadcast_to(rgb("#2c1b5e") * (1 - row) + rgb("#ff7a4a") * row, (HORIZON, 4, 3)).copy()


def sun(size: int = 500) -> np.ndarray:
    """The sun on black, opaque: a disc 0.44 of the picture across, yellow at the top and red at the bottom, with room
    round it for its glow."""
    y, x = np.mgrid[0:size, 0:size].astype(np.float32)
    disc = np.clip(0.5 + (0.22 * size - np.hypot(x - size / 2, y - size / 2)), 0.0, 1.0)[..., None]
    tone = rgb("#ffe27a") * (1 - y / size)[..., None] + rgb("#ff4a3a") * (y / size)[..., None]
    return (tone * disc).astype(np.float32)


def figure(width: int = 360, height: int = 520) -> tuple[np.ndarray, np.ndarray]:
    """A made-up cut-out: a round head over a body that widens to the base, shaded from the left, and its matte (1 where
    the figure is, with an edge a pixel wide)."""
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    head = np.hypot(x - width / 2, y - 0.2 * height) - 0.16 * height
    half = 0.07 * width + 0.36 * width * np.clip((y - 0.32 * height) / (0.68 * height), 0.0, 1.0) ** 0.8
    body = np.maximum(np.abs(x - width / 2) - half, np.maximum(0.33 * height - y, y - (height - 1)))
    matte = np.clip(0.5 - np.minimum(head, body), 0.0, 1.0).astype(np.float32)
    light = np.clip(1.15 - (x / width) * 0.9, 0.35, 1.0)[..., None]
    return np.clip(rgb("#3ec6b0") * light * np.ones((height, width, 3), np.float32), 0.0, 1.0), matte


def fade() -> np.ndarray:
    """A picture the size of the canvas that is white at the figure's feet and black 220 pixels below them: what a
    reflection is read through (`use="luminance"`)."""
    row = np.clip(1.0 - (np.arange(H, dtype=np.float32) - FEET) / 220.0, 0.0, 1.0)[:, None, None]
    return np.broadcast_to(row, (H, 4, 3)).copy()


def build() -> bts.Scene:
    """The evening: the sun with its glow behind it, the figure with its shadow and reflection on the floor, and the
    headline behind the figure. The references are written before the layers they name."""
    skin, matte = figure()
    glow = bts.Layer.of("sun").named("sun_glow").look(GLOW).as_light().blend("screen")
    shadow = (
        bts.Layer.fill("#000000")
        .named("shadow")
        .mask(bts.Layer.of("figure").place(aspect=0.18, anchor=(0.5, 1.0), skew=(-55, 0), dy=4), feather=10)
        .opacity(0.75)
    )
    reflection = (
        bts.Layer.of("figure")
        .named("reflection")
        .place(flip=(False, True), anchor=(0.5, 1.0))
        .opacity(0.4)
        .mask(bts.Layer.picture(fade()).fit((0, 0, W, H), "stretch"), use="luminance")
    )
    headline = (
        bts.Layer.text("EVENING", font=FONT, size=210, axes={"wght": 800}, align="center", color=PAPER)
        .named("headline")
        .place(dx=W / 2, dy=610)
        .mask(bts.Layer.of("figure"), invert=True)  # hidden where the figure is: the type passes behind it
    )
    return bts.Scene(W, H).add(
        bts.Layer.fill("#4a3772").named("floor"),  # the whole canvas: the sky lies over its top, so no seam shows
        bts.Layer.picture(sky()).named("sky").fit((0, 0, W, HORIZON), "stretch"),
        glow,
        bts.Layer.picture(sun()).named("sun").fit((620, 40, 500, 500), "stretch").blend("screen"),
        shadow,
        reflection,
        headline,
        bts.Layer.picture(skin, maps={"subject": matte})
        .named("figure")
        .mask("subject")
        .place(dx=W / 2 - 180, dy=FEET - 520),
    )


if __name__ == "__main__":
    from bullterrier_image import imageio

    imageio.save(build().render(W), "references.png")
