"""C5c: room for light. A glow past the box of the layer that makes it.

Three things that glow, each drawn twice. Above, the layer is tight: its frame is exactly what it draws (a disc in a
frame its own size, a line of type, a figure in its bounding box) and the glow is a reference to it, made light, with a
look: the look's room is image's. Below, the same layers on a frame padded by hand, the way a glow had to be made before
the room. The two rows are the same picture, to the rounding of vector's 8-bit edges: the layer's look runs on a raster
grown by the room image says the look needs, and a light layer shows it.

Every picture is made here from drawings and type, so the example needs nothing but the package.
"""

import json

import bullterrier_scene as bts

W, H = 1200, 660
FONT = "bricolage_grotesque"
GROUND, INK = "#0d1020", "#c9cde0"
KAPPA = 0.5522847498  # the control distance of a cubic that follows a quarter of a circle, in units of the radius
CENTRES = (210, 600, 990)  # the middle of each column
ROW_Y = (190, 480)  # the middle of each row
PAD = 90  # the hand-made frame's margin on each side: less than the glow needs, so the look's room is at work there too
# `Recipe([fx.bloom(radius=18, threshold=0.2, amount=2.0)]).to_dict()`: light bleeding from the sun
SUN_GLOW = {
    "steps": [
        {
            "effect": "bloom",
            "params": {"radius": 18.0, "threshold": 0.2, "amount": 2.0},
            "opacity": 1.0,
            "blend_mode": "normal",
        }
    ]
}
# `Recipe([fx.bloom(radius=7, threshold=0.2, amount=3.0)]).to_dict()`: a neon tube's glow
NEON_GLOW = {
    "steps": [
        {
            "effect": "bloom",
            "params": {"radius": 7.0, "threshold": 0.2, "amount": 3.0},
            "opacity": 1.0,
            "blend_mode": "normal",
        }
    ]
}
# `Recipe([fx.gaussian_blur(radius=9)]).to_dict()`: a soft halo
SOFT_HALO = {
    "steps": [
        {"effect": "gaussian_blur", "params": {"radius": 9.0}, "opacity": 1.0, "blend_mode": "normal"},
    ]
}


def ellipse(cx: float, cy: float, r: float) -> str:
    """The path of a circle of radius `r` about (`cx`, `cy`), as four cubics."""
    k = KAPPA * r
    return (
        f"M{cx + r} {cy} C{cx + r} {cy + k} {cx + k} {cy + r} {cx} {cy + r} "
        f"C{cx - k} {cy + r} {cx - r} {cy + k} {cx - r} {cy} C{cx - r} {cy - k} {cx - k} {cy - r} {cx} {cy - r} "
        f"C{cx + k} {cy - r} {cx + r} {cy - k} {cx + r} {cy} Z"
    )


def drawing(path: str, color: str, size: tuple[float, float]) -> str:
    """A drawing document of one path in a frame of `size`, in the form vector's `Drawing.to_json()` writes."""
    stack = {"items": [[{"path": path}]], "size": list(size)}
    return json.dumps({"root": [{"stack": stack}, {"fill": {"color": color}}], "version": 1})


def sun(pad: float) -> bts.Layer:
    """A disc of radius 60 in a square frame of 120 + 2 `pad`, put so that its middle is at the column's middle."""
    frame = 120 + 2 * pad
    return bts.Layer.drawing(drawing(ellipse(frame / 2, frame / 2, 60), "#ffc060", (frame, frame)))


def figure(pad: float) -> bts.Layer:
    """A head and shoulders (80 by 120, its bounding box) in a frame `pad` larger on each side."""
    body = (
        f"M{pad} {pad + 120} L{pad} {pad + 90} C{pad} {pad + 64} {pad + 80} {pad + 64} {pad + 80} {pad + 90} "
        f"L{pad + 80} {pad + 120} Z"
    )
    path = ellipse(pad + 40, pad + 24, 24) + " " + body
    return bts.Layer.drawing(drawing(path, "#ffe3b8", (80 + 2 * pad, 120 + 2 * pad)))


def neon(pad: float) -> bts.Layer:
    """The word NEON: bare when `pad` is 0, else inside a group whose frame is `pad` larger on each side."""
    word = {"font": FONT, "size": 110, "color": "#ff5fa8", "align": "center", "axes": {"wght": 800}}
    if pad == 0:
        return bts.Layer.text("NEON", **word)
    inner = bts.Scene(2 * 190 + 2 * pad, 2 * 90 + 2 * pad).add(
        bts.Layer.text("NEON", **word).place(dx=190 + pad, dy=110 + pad)
    )
    return bts.Layer.scene(inner)


def glowing(name: str, look: dict) -> bts.Layer:
    """The glow of the layer `name`: a reference to it with a look, made light and screened."""
    return bts.Layer.of(name).named(name + "_glow").look(look).as_light().blend("screen")


def row(index: int, pad: float) -> list[bts.Layer]:
    """The three things of one row, each with its glow, tight (`pad` 0) or padded by hand."""
    y = ROW_Y[index]
    suffix = str(index)
    sun_layer = sun(pad).named("sun" + suffix).place(dx=CENTRES[0] - 60 - pad, dy=y - 60 - pad)
    figure_layer = figure(pad).named("figure" + suffix).place(dx=CENTRES[1] - 40 - pad, dy=y - 60 - pad)
    # the word's baseline is 40 below the row's middle; its centre column is the column's middle
    neon_layer = neon(pad).named("neon" + suffix)
    neon_layer = neon_layer.place(
        dx=CENTRES[2] - (0 if pad == 0 else 190 + pad), dy=y + 40 - (0 if pad == 0 else 110 + pad)
    )
    return [
        sun_layer,
        figure_layer,
        neon_layer,
        glowing("sun" + suffix, SUN_GLOW),
        glowing("figure" + suffix, SOFT_HALO),
        glowing("neon" + suffix, NEON_GLOW),
    ]


def caption(words: str, y: float) -> bts.Layer:
    """A line of small type, left aligned."""
    return bts.Layer.text(words, font=FONT, size=26, color=INK, axes={"wght": 500}).place(dx=40, dy=y)


def build() -> bts.Scene:
    """The sheet: a tight layer and its glow above, the same on a frame padded by hand below."""
    return bts.Scene(W, H).add(
        bts.Layer.fill(GROUND).named("ground"),
        *row(0, 0),
        *row(1, PAD),
        caption("a layer as tight as what it draws: the look's room is image's", 328),
        caption("the same layers on a frame padded by hand", 618),
    )


if __name__ == "__main__":
    from bullterrier_image import imageio

    imageio.save(build().render(W), "room.png")
