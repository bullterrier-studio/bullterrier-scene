"""C7: a headline sliding in over 24 frames from one scene.

One value set per frame; the loop and the easing are the caller's, the scene has no time. Proves: `with_values` as
the only way to move something. Only the headline changes between frames (`cache_probe.py` shows the layers that
did not change are not drawn again: C7).
"""

from pathlib import Path

import numpy as np

import bullterrier_scene as bts

W, H = 1280, 720
FRAMES = 24


def build() -> bts.Scene:
    """A ground and a headline at rest."""
    return bts.Scene(W, H).add(
        bts.Layer.fill("#12101f").named("ground"),
        bts.Layer.text("SLIDING IN", font="bricolage_grotesque", size=170, color="#ffffff", axes={"wght": 800})
        .named("headline")
        .place(dx=100, dy=420),  # the baseline's left end, where the headline comes to rest
    )


def ease_out(t: float) -> float:
    """The caller's easing, 0..1 to 0..1."""
    return 1 - (1 - t) ** 3


def frame_values(i: int) -> dict[str, float]:
    """The headline's addresses at frame `i` of `FRAMES`."""
    e = ease_out(i / (FRAMES - 1))
    return {"headline.dx": 100 - 700 * (1 - e), "headline.opacity": e}


def frame(scene: bts.Scene, i: int, size: int = W) -> np.ndarray:
    """Frame `i`, drawn at `size`."""
    return scene.with_values(frame_values(i)).render(size)


if __name__ == "__main__":
    from bullterrier_image import imageio

    scene = build()
    Path("frames").mkdir(exist_ok=True)
    for i in range(FRAMES):
        imageio.save(frame(scene, i), f"frames/{i:02d}.png")
