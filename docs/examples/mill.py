"""M3: the mill piece as layers.

A night-graded photo of a mill, its windows and its sign lit as layers of their own, a logo drawn on with its own
glow, four words landing, a 9:16 delivery crop by `fit`. Every moving value is set per frame by `with_values`.
Proves: per-layer looks (the logo's glow does not light the windows), a region of image as a map, a vector
parameter as an address, a crop without an outside tool.

`build` takes the photo, the matte of the mill, the `windows` map (a region of the night look; `region` finds it with
image) and the logo (a vector `Drawing`, or anything with `to_json()`), so a test builds it from arrays and a
document. Where the numbers of a frame come from (a clock, a slider, a file of numbers) is the caller's business;
`frame_values` takes plain numbers. **Not built yet:** the look's room (C5b: the glow's halo is cut at its layer's
box) and the cache (C7: a frame draws every layer again).
"""

from pathlib import Path
from typing import Any

import numpy as np

import bullterrier_scene as bts

W, H = 720, 1280  # the delivery shape
PINK, CYAN = "#ff3fd0", "#8ffcff"
WORDS = [("35+", 0.30, 0.855), ("CLASSES", 0.62, 0.855), ("EVERY", 0.36, 0.90), ("WEEK", 0.64, 0.90)]

# The recipes as image's `Recipe(...).to_dict()` writes them (`fx` is image's effect namespace):
# night: gradient(colors=["#03010a"], mask="sky"), gradient_map(stops=[...]), halftone(shape="line", cell=4, ...)
# over the regions down (a gradient), brick (a colour range), glass (a band of brick), above, sky and windows.
NIGHT = {
    "format": 3,
    "steps": [
        {
            "effect": "gradient",
            "params": {"colors": ["#03010a"]},
            "opacity": 1.0,
            "blend_mode": "normal",
            "mask": "sky",
        },
        {
            "effect": "gradient_map",
            "params": {"stops": ["#03010a", "#09062c", "#201670", "#6a5acc"]},
            "opacity": 1.0,
            "blend_mode": "normal",
        },
        {
            "effect": "halftone",
            "params": {"shape": "line", "cell": 4, "angle": 45, "ink": "#8a4bff", "paper": "#04020f"},
            "opacity": 0.5,
            "blend_mode": "screen",
        },
    ],
    "name": "night",
    "regions": {
        "down": {"kind": "linear_gradient", "angle": 90.0},
        "brick": {
            "kind": "color_range",
            "target": [0.5490196347236633, 0.29019609093666077, 0.23529411852359772],
            "tolerance": 0.14,
            "feather": 0.04,
        },
        "glass": {"kind": "band", "of": "brick", "low": 0.0, "high": 0.3, "feather": 0.05},
        "above": {"kind": "band", "of": "down", "low": 0.0, "high": 0.6, "feather": 0.05},
        "sky": {"kind": "combine", "op": "multiply", "of": ["background", "above"]},
        "windows": {"kind": "combine", "op": "multiply", "of": ["glass", "subject"]},
    },
}
# `Recipe([fx.bloom(radius=16, threshold=0.5, amount=0.1, name="swell")])`: light that grows
SWELL = {
    "format": 3,
    "steps": [
        {
            "effect": "bloom",
            "params": {"radius": 16, "threshold": 0.5, "amount": 0.1},
            "opacity": 1.0,
            "blend_mode": "normal",
            "name": "swell",
        }
    ],
    "name": "swell",
}
# `Recipe([fx.bloom(radius=10, threshold=0.8, amount=0.3, color=CYAN, name="halo")])`
HALO = {
    "format": 3,
    "steps": [
        {
            "effect": "bloom",
            "params": {"radius": 10, "threshold": 0.8, "amount": 0.3, "color": "#8ffcff"},
            "opacity": 1.0,
            "blend_mode": "normal",
            "name": "halo",
        }
    ],
    "name": "halo",
}


def box(x: float, y: float, w: float, h: float) -> bts.Layer:
    """A white rectangle on the canvas, to use as a mask."""
    return bts.Layer.fill("#ffffff").fit((x, y, w, h), "stretch")


def build(photo: np.ndarray, matte: np.ndarray, windows: np.ndarray, logo: Any) -> bts.Scene:
    """The scene from the photo, the matte of the mill, its `windows` map and the logo drawing (anything with
    `to_json()`, reading the Params `ring_on` and `landed`, both 0..1)."""
    maps = {"subject": matte, "windows": windows}  # the look sees `subject`; masks see both
    # the 9:16 delivery crop of a wide photo is `fit` with "cover": `align` keeps the mill in frame
    night = bts.Layer.picture(photo, maps=maps).named("night").fit((0, 0, W, H), "cover", align=(0.5, 0.4)).look(NIGHT)
    sign_box = box(0.30 * W, 0.045 * H, 0.38 * W, 0.08 * H)
    words = [
        bts.Layer.text(
            w, font="bricolage_grotesque", size=60, color="#ffffff", align="center", axes={"wdth": 80, "wght": 800}
        )
        .named(f"word{n}")
        .place(dx=x * W, dy=y * H)
        .opacity(0)
        for n, (w, x, y) in enumerate(WORDS, start=1)
    ]
    return bts.Scene(W, H).add(
        night,
        # the night photo again (picture, fit and look inherited), bloomed, shown only where a mask says
        bts.Layer.of("night").named("windows").look(SWELL).mask("windows", feather=3).blend("screen"),
        bts.Layer.of("night").named("sign_glow").look(SWELL).mask(sign_box, feather=6).blend("screen"),
        bts.Layer.fill(PINK).named("sign").mask(sign_box).blend("multiply"),
        bts.Layer.drawing(logo).named("logo").fit((0.17 * W, 0.45 * H, 0.66 * W, 0.25 * H), "contain"),
        # the logo's own light: it follows the logo's parameters and lights nothing else
        bts.Layer.of("logo").named("logo_glow").look(HALO).as_light().blend("screen"),
        *words,
    )


def frame_values(level: float, pulse: float, bump: float, ring: float, filled: float, landing: list[float]) -> dict:
    """The scene's addresses for one moment, from plain numbers in 0..1."""
    swell_amount = 0.05 + 0.35 * level + 0.2 * pulse  # the windows and the sign swell together
    values: dict[str, object] = {
        "windows.look.swell.amount": swell_amount,
        "sign_glow.look.swell.amount": swell_amount,
        "logo_glow.look.halo.amount": 0.2 + 0.2 * level + 0.3 * pulse + 0.3 * bump,
        "logo.scale": 1 + 0.28 * bump + 0.04 * pulse,  # the glow follows: it is the logo's layer again
        "logo.drawing.ring_on": ring,
        "logo.drawing.landed": filled,
    }
    for n, a in enumerate(landing, start=1):
        values |= {f"word{n}.opacity": a, f"word{n}.scale": 1 + 0.35 * (1 - a)}
    return values


def frame(scene: bts.Scene, values: dict[str, object], size: int = H) -> np.ndarray:
    """One frame: the scene with `values` set, drawn at `size`."""
    return scene.with_values(values).render(size)


def region(photo: np.ndarray, matte: np.ndarray, name: str) -> np.ndarray:
    """A region of the look as a map the size of the photo: image shows each region its steps use as a trace stage."""
    from bullterrier_image import Recipe, trace

    step = {"effect": "grade", "params": {}, "opacity": 1.0, "blend_mode": "normal", "mask": name}
    probe = Recipe.from_dict({**NIGHT, "steps": [step], "name": "probe"})
    stage = next(s for s in trace(photo, probe, maps={"subject": matte}) if s.label.startswith(f"region '{name}'"))
    return stage.image[..., 0]


def logo_drawing(logo_svg: str) -> Any:
    """The logo from its tagged SVG: parts drawn on and filled in by two vector parameters."""
    import bullterrier_vector as btvec

    logo_parts = btvec.Drawing.from_svg(logo_svg)
    ring_on = btvec.Param(
        "ring_on", default=0, min=0, max=1, unit="fraction", meaning="how much of the ring is drawn on"
    )
    landed = btvec.Param("landed", default=0, min=0, max=1, unit="fraction", meaning="the filled logo shows")
    return btvec.Drawing(
        logo_parts.only(group="ring").stroke(PINK, width=3).slice(0, ring_on),
        logo_parts.only(group=["swoosh", "text"]).fill(CYAN).opacity(landed),
        size=logo_parts.size,
    )


if __name__ == "__main__":
    from bullterrier_image import imageio, subject_matte

    picture = imageio.load("inputs/mill.jpg")  # wider than 9:16
    mill_matte = subject_matte(
        picture, segmenter="sam2", points=[(0.45, 0.4, True), (0.8, 0.62, True), (0.5, 0.01, False)]
    )
    mill = build(
        picture,
        mill_matte,
        region(picture, mill_matte, "windows"),
        logo_drawing(Path("inputs/logo.svg").read_text(encoding="utf-8")),
    )
    values = frame_values(level=0.7, pulse=0.4, bump=0.0, ring=1.0, filled=1.0, landing=[1, 0.6, 0.2, 0])
    imageio.save(frame(mill, values), "mill-frame.png")
