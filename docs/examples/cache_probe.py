"""C7: a layer that did not change is not drawn again.

A plugin layer kind that counts its `draw` calls is the test seam of the cache: the count is public. Proves: a
value that only moves the headline never redraws the static layer; a changed option of the static layer does.
Arrives with C7.
"""

import numpy as np

import bullterrier_scene as bts

W, H = 640, 360
calls: list[tuple[float, float, float, float, float]] = []


def draw(x: float, y: float, width: float, height: float, scale: float, *, tone: float) -> np.ndarray:
    """One rectangle of the canvas (design pixels) at `scale` output pixels per design pixel: a flat grey."""
    calls.append((x, y, width, height, scale))
    return np.full((round(height * scale), round(width * scale), 4), tone, np.float32)


def build(tone: float) -> bts.Scene:
    """A counted layer under a headline."""
    return bts.Scene(W, H).add(
        bts.Layer.make("counter", tone=tone).named("static"),
        bts.Layer.text("Hi", font="bricolage_grotesque", size=100, color="#ffffff").named("headline").place(dy=200),
    )


if __name__ == "__main__":
    bts.register_layer("counter", draw)  # a plugin registers itself by entry point; a test registers in process
    scene = build(tone=0.2)
    scene.render(W)
    first = len(calls)
    for dx in (10, 20, 30):
        scene.with_values({"headline.dx": dx}).render(W)  # derived scenes share the cache
    if len(calls) != first:
        raise SystemExit(f"the static layer was drawn {len(calls) - first} more times")
    build(tone=0.4).render(W)  # a changed option is another layer
    if len(calls) == first:
        raise SystemExit("a changed option did not redraw")
