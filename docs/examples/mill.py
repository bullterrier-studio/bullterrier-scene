"""M3: the mill piece as layers.

A night-graded photo of a mill, its windows and its sign lit as layers of their own, a logo drawn on with its own
glow, four words landing, a 9:16 delivery crop by `fit`. Every moving value is set per frame by `with_values`.
Proves: per-layer looks (the logo's glow does not light the windows), a region of image as a map, a vector
parameter as an address, a crop without an outside tool.

Where the numbers come from (a song, a clock, a slider) is the caller's business; `frame_values` takes plain
numbers.
"""

from pathlib import Path

import bullterrier_vector as btvec
import numpy as np
from bullterrier_image import Band, ColorRange, Combine, LinearGradient, Recipe, fx, imageio, subject_matte, trace

import bullterrier_scene as bts

W, H = 720, 1280  # the delivery shape
PINK, CYAN = "#ff3fd0", "#8ffcff"
WORDS = [("35+", 0.30, 0.855), ("CLASSES", 0.62, 0.855), ("EVERY", 0.36, 0.90), ("WEEK", 0.64, 0.90)]

regions = {
    "down": LinearGradient(angle=90),
    "brick": ColorRange(target="#8c4a3c", tolerance=0.14, feather=0.04),
    "glass": Band(of="brick", low=0.0, high=0.3, feather=0.05),  # what is not brick: windows and sign
    "above": Band(of="down", low=0.0, high=0.6, feather=0.05),
    "sky": Combine(of=["background", "above"]),
    "windows": Combine(of=["glass", "subject"]),
}
night_look = Recipe(
    [
        fx.gradient(colors=["#03010a"], mask="sky"),
        fx.gradient_map(stops=["#03010a", "#09062c", "#201670", "#6a5acc"]),
        fx.halftone(shape="line", cell=4, angle=45, ink="#8a4bff", paper="#04020f", blend_mode="screen", opacity=0.5),
    ],
    name="night",
    regions=regions,
)
swell = Recipe([fx.bloom(radius=16, threshold=0.5, amount=0.1, name="swell")], name="swell")  # light that grows
halo = Recipe([fx.bloom(radius=10, threshold=0.8, amount=0.3, color=CYAN, name="halo")], name="halo")


def region(photo: np.ndarray, matte: np.ndarray, name: str) -> np.ndarray:
    """A region of the look as a map the size of the photo: image shows each region its steps use as a trace stage."""
    probe = Recipe([fx.grade(mask=name)], regions=regions, name="probe")
    stage = next(s for s in trace(photo, probe, maps={"subject": matte}) if s.label.startswith(f"region '{name}'"))
    return stage.image[..., 0]


def box(x: float, y: float, w: float, h: float) -> bts.Layer:
    """A white rectangle on the canvas, to use as a mask."""
    return bts.Layer.drawing(btvec.Drawing(btvec.Path.rect(x, y, w, h).fill("#ffffff"), size=(W, H)))


def build(photo: np.ndarray, matte: np.ndarray, logo_svg: str) -> bts.Scene:
    """The scene from the photo, the matte of the mill and the tagged SVG text of the logo."""
    maps = {"subject": matte, "windows": region(photo, matte, "windows")}  # the look sees `subject`; masks see both
    # `crop`: 9:16 from a wide photo, the mill kept in frame by the alignment
    night = (
        bts.Layer.picture(photo, maps=maps).named("night").fit((0, 0, W, H), "cover", align=(0.5, 0.4)).look(night_look)
    )
    sign_box = box(0.30 * W, 0.045 * H, 0.38 * W, 0.08 * H)

    # The logo: parts tagged by group in the SVG, drawn on and filled in by two vector parameters.
    logo_parts = btvec.Drawing.from_svg(logo_svg)
    ring_on = btvec.Param(
        "ring_on", default=0, min=0, max=1, unit="fraction", meaning="how much of the ring is drawn on"
    )
    landed = btvec.Param("landed", default=0, min=0, max=1, unit="fraction", meaning="the filled logo shows")
    logo = btvec.Drawing(
        logo_parts.only(group="ring").stroke(PINK, width=3).slice(0, ring_on),
        logo_parts.only(group=["swoosh", "text"]).fill(CYAN).opacity(landed),
        size=logo_parts.size,
    )

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
        # `lit`: the night photo again, bloomed, shown only where a mask says, screened over itself
        bts.Layer.of("night").named("windows").look(swell).mask("windows", feather=3).blend("screen"),
        bts.Layer.of("night").named("sign_glow").look(swell).mask(sign_box, feather=6).blend("screen"),
        bts.Layer.fill(PINK).named("sign").mask(sign_box).blend("multiply"),
        bts.Layer.drawing(logo).named("logo").fit((0.17 * W, 0.45 * H, 0.66 * W, 0.25 * H), "contain"),
        # `glow`: the logo's own light; it follows the logo's parameters and lights nothing else
        bts.Layer.of("logo").named("logo_glow").look(halo).alpha("light").blend("screen"),
        *words,
    )


def frame_values(level: float, hit: float, impact: float, ring: float, filled: float, landing: list[float]) -> dict:
    """The scene's addresses for one moment, from plain numbers in 0..1."""
    values: dict[str, object] = {
        "windows.look.swell.amount": 0.05 + 0.35 * level + 0.2 * hit,
        "sign_glow.look.swell.amount": 0.05 + 0.35 * level + 0.2 * hit,
        "logo_glow.look.halo.amount": 0.2 + 0.2 * level + 0.3 * hit + 0.3 * impact,
        "logo.scale": 1 + 0.28 * impact + 0.04 * hit,  # the glow follows: it is the logo's layer again
        "logo.drawing.ring_on": ring,
        "logo.drawing.landed": filled,
    }
    for n, a in enumerate(landing, start=1):
        values |= {f"word{n}.opacity": a, f"word{n}.scale": 1 + 0.35 * (1 - a)}
    return values


if __name__ == "__main__":
    picture = imageio.load("inputs/mill.jpg")  # wider than 9:16
    mill_matte = subject_matte(
        picture, segmenter="sam2", points=[(0.45, 0.4, True), (0.8, 0.62, True), (0.5, 0.01, False)]
    )
    mill = build(picture, mill_matte, Path("inputs/logo.svg").read_text(encoding="utf-8"))
    values = frame_values(level=0.7, hit=0.4, impact=0.0, ring=1.0, filled=1.0, landing=[1, 0.6, 0.2, 0])
    imageio.save(mill.with_values(values).render(H), "mill-frame.png")
