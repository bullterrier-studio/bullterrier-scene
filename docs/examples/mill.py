"""M3: the mill piece as layers.

A night-graded photo of a mill, its windows and its sign lit as layers of their own, a logo drawn on with its own
glow, four words landing, a 9:16 delivery crop by `fit`. Every moving value is set per frame by `with_values`.
Proves: per-layer looks (the logo's glow does not light the windows), a region of image as a mask, a vector
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

photo = imageio.load("inputs/mill.jpg")  # wider than 9:16
matte = subject_matte(photo, segmenter="sam2", points=[(0.45, 0.4, True), (0.8, 0.62, True), (0.5, 0.01, False)])

regions = {
    "down": LinearGradient(angle=90),
    "brick": ColorRange(target="#8c4a3c", tolerance=0.14, feather=0.04),
    "glass": Band(of="brick", low=0.0, high=0.3, feather=0.05),  # what is not brick: windows and sign
    "above": Band(of="down", low=0.0, high=0.6, feather=0.05),
    "sky": Combine(of=["background", "above"]),
    "windows": Combine(of=["glass", "subject"]),
}
night = Recipe(
    [
        fx.gradient(colors=["#03010a"], mask="sky"),
        fx.gradient_map(stops=["#03010a", "#09062c", "#201670", "#6a5acc"]),
        fx.halftone(shape="line", cell=4, angle=45, ink="#8a4bff", paper="#04020f", blend_mode="screen", opacity=0.5),
    ],
    name="night",
    regions=regions,
)
swell = fx.bloom(radius=16, threshold=0.5, amount=0.1, name="swell")  # light that grows with the bass


def region(name: str) -> np.ndarray:
    """A region of the look as a map the size of the photo: image shows each region its steps use as a trace stage."""
    probe = Recipe([fx.grade(mask=name)], regions=regions, name="probe")
    stage = next(s for s in trace(photo, probe, maps={"subject": matte}) if s.label.startswith(f"region '{name}'"))
    return stage.image[..., 0]


def box(x: float, y: float, w: float, h: float) -> bts.Layer:
    """A white rectangle on the canvas, to use as a mask."""
    return bts.Layer.drawing(btvec.Drawing(btvec.Path.rect(x, y, w, h).fill("#ffffff"), size=(W, H)))


base = bts.Layer.picture(photo, maps={"subject": matte}).fit(
    (0, 0, W, H), "cover", anchor=(0.5, 0.4)
)  # 9:16 from a wide photo
lit = base.look(night.replace(steps=[*night.steps, swell]))  # the night photo, bloomed


def lighter(name: str, where: bts.Layer | np.ndarray, feather: float) -> bts.Layer:
    """The bloomed night photo again, shown only `where`, screened over the photo."""
    return lit.named(name).mask(where, feather=feather).blend("screen")


# The logo: parts tagged by group in the SVG, drawn on and filled in by two vector parameters.
logo_svg = btvec.Drawing.from_svg(Path("inputs/logo.svg").read_text(encoding="utf-8"))
ring_on = btvec.Param("ring_on", default=0, min=0, max=1, unit="fraction", meaning="how much of the ring is drawn on")
landed = btvec.Param("landed", default=0, min=0, max=1, unit="fraction", meaning="the filled logo shows")
logo_drawing = btvec.Drawing(
    logo_svg.only(group="ring").stroke(PINK, width=3).slice(0, ring_on),
    logo_svg.only(group=["swoosh", "text"]).fill(CYAN).opacity(landed),
    size=logo_svg.size,
)
logo = bts.Layer.drawing(logo_drawing).named("logo").fit((0.17 * W, 0.45 * H, 0.66 * W, 0.25 * H), "contain")
logo_glow = (
    logo.named("logo_glow")
    .look(Recipe([fx.bloom(radius=10, threshold=0.8, amount=0.3, color=CYAN, name="halo")], name="logo-glow"))
    .blend("screen")
)  # the glow is the logo's only: nothing else is lit by it

WORDS = [("35+", 0.30, 0.855), ("CLASSES", 0.62, 0.855), ("EVERY", 0.36, 0.90), ("WEEK", 0.64, 0.90)]
words = [
    bts.Layer.text(
        word, font="bricolage_grotesque", size=60, color="#ffffff", align="center", axes={"wdth": 80, "wght": 800}
    )
    .named(f"word{n}")
    .place(dx=x * W, dy=y * H)
    .opacity(0)
    for n, (word, x, y) in enumerate(WORDS, start=1)
]

sign_box = box(0.30 * W, 0.045 * H, 0.38 * W, 0.08 * H)
mill = bts.Scene(W, H).add(
    base.look(night).named("night"),
    lighter("windows", region("windows"), feather=3),  # a region of image's as a map: no hand-made rectangle
    lighter("sign_glow", sign_box, feather=6),
    bts.Layer.fill(PINK).named("sign").mask(sign_box).blend("multiply"),
    logo,
    logo_glow,
    *words,
)


def frame_values(
    bass: float, kick: float, slam: float, ring: float, filled: float, landing: list[float]
) -> dict[str, float]:
    """The scene's addresses for one moment, from plain numbers in 0..1."""
    pop = 1 + 0.28 * slam + 0.04 * kick
    values = {
        "windows.look.swell.amount": 0.05 + 0.35 * bass + 0.2 * kick,
        "sign_glow.look.swell.amount": 0.05 + 0.35 * bass + 0.2 * kick,
        "logo_glow.look.halo.amount": 0.2 + 0.2 * bass + 0.3 * kick + 0.3 * slam,
    }
    for name in ("logo", "logo_glow"):  # one logo drawn twice has its parameters twice
        values |= {f"{name}.scale_x": pop, f"{name}.scale_y": pop}
        values |= {f"{name}.drawing.ring_on": ring, f"{name}.drawing.landed": filled}
    for n, a in enumerate(landing, start=1):
        values |= {
            f"word{n}.opacity": a,
            f"word{n}.scale_x": 1 + 0.35 * (1 - a),
            f"word{n}.scale_y": 1 + 0.35 * (1 - a),
        }
    return values


frame = mill.with_values(frame_values(bass=0.7, kick=0.4, slam=0.0, ring=1.0, filled=1.0, landing=[1, 0.6, 0.2, 0]))
imageio.save(frame.render(H), "mill-frame.png")
