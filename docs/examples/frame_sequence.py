"""C7: a headline sliding in over 24 frames from one scene.

One value set per frame; the loop and the easing are the caller's, the scene has no time. Proves: `with_values` as
the only way to move something, and the shape of the cached re-render: only the headline changes between frames.
"""

from pathlib import Path

from bullterrier_image import imageio

import bullterrier_scene as bts

W, H = 1280, 720
FRAMES = 24

scene = bts.Scene(W, H).add(
    bts.Layer.fill("#12101f").named("ground"),
    bts.Layer.text("SLIDING IN", font="bricolage_grotesque", size=170, color="#ffffff", axes={"wght": 800})
    .named("headline")
    .place(dx=100, dy=420),  # the baseline's left end, where the headline comes to rest
)


def ease_out(t: float) -> float:
    """The caller's easing, 0..1 to 0..1."""
    return 1 - (1 - t) ** 3


Path("frames").mkdir(exist_ok=True)
for i in range(FRAMES):
    e = ease_out(i / (FRAMES - 1))
    frame = scene.with_values({"headline.dx": 100 - 700 * (1 - e), "headline.opacity": e})
    imageio.save(frame.render(W), f"frames/{i:02d}.png")
