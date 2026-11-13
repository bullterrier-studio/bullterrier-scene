"""C3: masks, maps and text.

A landscape clipped into big letters (a mask by a text layer), a line of type tucked behind a cut-out figure (a mask by
the figure's own matte, inverted, on the type), and a soft glow, a contact shadow and a vignette (masks with a feather).
Every picture is made here from arithmetic, so the example needs nothing but the package. Proves: a text layer; a mask
by a layer on the canvas (stage 6); a mask by a picture's own map (stage 4); `invert`; `feather`; a mask layer that has
a mask of its own.

The figure is drawn twice, once as the layer in front and once as the mask that hides the type: a layer that draws
another layer again (`Layer.of`) arrives with C4, and then the copy goes.
"""

import numpy as np

import bullterrier_scene as bts

W, H = 1200, 800
FONT = "bricolage_grotesque"
GROUND, PAPER, SUN = "#1b1830", "#f2ece0", "#ffd36b"


def rgb(color: str) -> np.ndarray:
    """A `"#rrggbb"` as a float32 triple."""
    return np.array([int(color[at : at + 2], 16) for at in (1, 3, 5)], np.float32) / 255


def landscape(width: int = 1200, height: int = 800) -> np.ndarray:
    """A made-up photograph, opaque: a sky from violet to orange, a low sun, three ridges and a lake with its glints."""
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    row = (y / height)[..., None]
    picture = rgb("#4a2c8a") * (1 - row) + rgb("#ff8a4c") * row
    sun = np.clip(1.0 - np.hypot(x - 0.68 * width, y - 0.46 * height) / (0.1 * width), 0.0, 1.0)[..., None]
    picture = picture * (1 - sun) + rgb(SUN) * sun
    for ridge, (phase, base, tone) in enumerate(
        [(0.4, 0.55, "#6a3c7a"), (1.9, 0.63, "#3d2a62"), (3.1, 0.72, "#201a3f")]
    ):
        crest = (base + 0.05 * np.sin(x / (0.11 * width) + phase) + 0.02 * np.sin(x / (0.037 * width) + ridge)) * height
        picture = np.where((y > crest)[..., None], rgb(tone), picture)
    lake = y > 0.8 * height
    glints = 0.5 + 0.5 * np.sin(x / 9.0 + 14.0 * np.sin(y / 23.0))
    water = (
        rgb("#2a2150") * (0.8 + 0.2 * glints[..., None])
        + rgb(SUN) * 0.25 * np.exp(-(((x - 0.68 * width) / 90) ** 2))[..., None] * glints[..., None]
    )
    return np.clip(np.where(lake[..., None], water, picture), 0.0, 1.0).astype(np.float32)


def figure(width: int = 360, height: int = 520) -> tuple[np.ndarray, np.ndarray]:
    """A made-up cut-out: a round head over a body that widens to the base, shaded from the left, and its matte (1 where
    the figure is, with an edge a pixel wide)."""
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    head = np.hypot(x - width / 2, y - 0.2 * height) - 0.16 * height
    half = 0.07 * width + 0.36 * width * np.clip((y - 0.32 * height) / (0.68 * height), 0.0, 1.0) ** 0.8
    body = np.maximum(np.abs(x - width / 2) - half, np.maximum(0.33 * height - y, y - (height - 1)))
    matte = np.clip(0.5 - np.minimum(head, body), 0.0, 1.0).astype(np.float32)
    light = np.clip(1.15 - (x / width) * 0.9, 0.35, 1.0)[..., None]
    skin = rgb("#e2412e") * light
    return np.clip(skin * np.ones((height, width, 3), np.float32), 0.0, 1.0).astype(np.float32), matte


def ellipse(cx: float, cy: float, rx: float, ry: float) -> dict:
    """A drawing (as `Drawing.to_json()` writes it, a dict) of a white ellipse on a frame the size of the canvas."""
    angle = np.linspace(0.0, 2.0 * np.pi, 120, endpoint=False)
    points = " L".join(f"{cx + rx * np.cos(a):.2f} {cy + ry * np.sin(a):.2f}" for a in angle)
    return {
        "root": [{"stack": {"items": [[{"path": f"M{points} Z"}]], "size": [W, H]}}, {"fill": {"color": "#ffffff"}}],
        "version": 1,
    }


def clipped_type() -> bts.Scene:
    """The poster: the landscape in the letters of CLIP, the type BEHIND tucked behind the figure, a soft glow and
    shadow on the floor, and a soft vignette."""
    skin, matte = figure()
    feet = H - 30

    def figure_layer() -> bts.Layer:
        return bts.Layer.picture(skin, maps={"subject": matte}).mask("subject").place(dx=W / 2 - 180, dy=feet - 520)

    letters = bts.Layer.text("CLIP", font=FONT, size=400, axes={"wght": 800}, align="center").place(dx=W / 2, dy=330)
    behind = (
        bts.Layer.text("BEHIND", font=FONT, size=250, axes={"wght": 800}, align="center", color=PAPER)
        .place(dx=W / 2, dy=H - 120)
        .mask(figure_layer(), invert=True)  # hidden where the figure is: the type passes behind it
    )
    inset = bts.Layer.fill("#ffffff").fit((90, 70, W - 180, H - 140), "stretch")
    return bts.Scene(W, H).add(
        bts.Layer.fill(GROUND).named("ground"),
        bts.Layer.fill("#ff8a4c")
        .named("glow")
        .opacity(0.5)
        .mask(bts.Layer.drawing(ellipse(W / 2, feet, 460, 130)), feather=70),
        # the landscape, with its sun and ridges in the band the letters stand in, shows only in the letters
        bts.Layer.picture(landscape())
        .named("photo")
        .fit((60, 30, W - 120, 340), "cover", align=(0.5, 0.62))
        .mask(letters),
        behind.named("behind"),
        bts.Layer.fill("#000000")
        .named("shadow")
        .opacity(0.7)
        .mask(bts.Layer.drawing(ellipse(W / 2, feet, 190, 16)), feather=12),
        figure_layer().named("figure"),
        bts.Layer.fill("#0b0817").named("vignette").opacity(0.7).mask(inset, feather=110, invert=True),
    )


if __name__ == "__main__":
    from bullterrier_image import imageio

    imageio.save(clipped_type().render(W), "clipped_type.png")
