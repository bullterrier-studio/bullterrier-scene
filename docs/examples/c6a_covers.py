"""C6a: one scene, six covers.

A cover template with a hole for the photo, a title, an accent colour and a placement (how far and how much the photo
is moved and zoomed in its frame), set six times by `with_values` and drawn six times: a contact sheet of the results.
The six photos are made here from arithmetic (no third-party picture). Proves: a picture filled into a hole, a text, a
colour and numbers set by address, the template left as it was, and one scene as the source of every cover.
"""

import numpy as np

import bullterrier_scene as bts

W, H = 256, 160  # one cover, in design pixels
GUTTER = 10
FONT = "bricolage_grotesque"
PAPER = "#f4efe6"

# what each cover sets: the title, the accent colour and where the photo sits (`photo.dx`, `.scale`, `.rotate`)
COVERS = [
    ("Dusk", "#c2410c", -40.0, 1.0, 0.0),
    ("Low tide", "#0e7490", 30.0, 1.15, 0.0),
    ("Night shift", "#7e22ce", 0.0, 1.3, -4.0),
    ("Green room", "#15803d", -60.0, 1.0, 3.0),
    ("Red eye", "#b91c1c", 20.0, 1.4, 0.0),
    ("Paper moon", "#a16207", 0.0, 1.0, 8.0),
]


def build() -> bts.Scene:
    """The template: a ground, a hole for the photo, an accent band, a rule and a title."""
    return bts.Scene(W, H).add(
        bts.Layer.fill("#14110f").named("ground"),
        bts.Layer.picture(None).named("photo").fit((0, 0, W, H), "cover"),
        bts.Layer.fill("#c2410c").named("band").fit((0, 112, W, 48), "stretch").opacity(0.9),
        bts.Layer.fill(PAPER).named("rule").fit((18, 104, 44, 4), "stretch"),
        bts.Layer.text("Title", font=FONT, size=24, color=PAPER, axes={"wght": 800})
        .named("title")
        .place(dx=18, dy=144),
    )


def photos() -> list[np.ndarray]:
    """Six pictures of 320 by 200 pixels, each its own pattern and palette, made from arithmetic."""
    height, width = 200, 320
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    u, v = x / width, y / height

    def palette(pattern: np.ndarray, dark: str, light: str) -> np.ndarray:
        low = np.array([int(dark[at : at + 2], 16) for at in (1, 3, 5)], np.float32) / 255
        high = np.array([int(light[at : at + 2], 16) for at in (1, 3, 5)], np.float32) / 255
        return (low + (high - low) * np.clip(pattern, 0, 1)[..., None]).astype(np.float32)

    rings = 0.5 + 0.5 * np.sin(np.hypot(x - 200, y - 90) * 0.22)
    stripes = 0.5 + 0.5 * np.sin((x + y * 0.6) * 0.16)
    checks = ((x // 26 + y // 26) % 2).astype(np.float32)
    blobs = 0.5 + 0.25 * np.sin(u * 14) + 0.25 * np.sin(v * 9 + u * 5)
    sun = np.clip(1.4 - np.hypot(x - 160, y - 70) / 70, 0, 1) + 0.35 * v
    waves = 0.5 + 0.5 * np.sin(v * 38 + np.sin(u * 7) * 3)
    return [
        palette(sun, "#1b1040", "#f59e0b"),
        palette(waves, "#083344", "#67e8f9"),
        palette(rings, "#2e1065", "#e879f9"),
        palette(blobs, "#052e16", "#86efac"),
        palette(checks * 0.6 + 0.2 * u, "#450a0a", "#fca5a5"),
        palette(stripes, "#422006", "#fde68a"),
    ]


def cover_values(photo: np.ndarray, title: str, accent: str, dx: float, zoom: float, turn: float) -> dict[str, object]:
    """The addresses of one cover."""
    return {
        "photo.picture": photo,
        "photo.dx": dx,
        "photo.scale": zoom,
        "photo.rotate": turn,
        "title.text": title,
        "band.color": accent,
    }


def covers() -> list[np.ndarray]:
    """The six covers, each the template with its values set, drawn at the size of the canvas."""
    template = build()
    return [
        template.with_values(cover_values(photo, *cover)).render(W)
        for photo, cover in zip(photos(), COVERS, strict=True)
    ]


def contact_sheet() -> bts.Scene:
    """The six covers on a sheet, three across and two down, each laid as a picture on a paper ground."""
    columns, rows = 3, 2
    sheet = bts.Scene(columns * W + (columns + 1) * GUTTER, rows * H + (rows + 1) * GUTTER)
    tiles = [
        bts.Layer.picture(cover)
        .named(f"cover{n}")
        .place(dx=GUTTER + (n % columns) * (W + GUTTER), dy=GUTTER + (n // columns) * (H + GUTTER))
        for n, cover in enumerate(covers())
    ]
    return sheet.add(bts.Layer.fill("#e7e0d3").named("table"), *tiles)
