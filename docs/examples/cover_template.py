"""C6: one design, many pictures.

One scene with a photo hole, a band colour and a title as parameters, rendered six times with a different photo, title
and colour each by `with_values`. Proves: addresses, `with_values`, a picture filled into a hole. `Scene.parameters()`
lists what a run may set: `photo.picture`, `band.color`, `title.text`... Saving the template as JSON and reading it
back is C6b.
"""

from pathlib import Path

import numpy as np

import bullterrier_scene as bts

W, H = 1280, 720


def build() -> bts.Scene:
    """The template: a hole for the photo, a band and a title."""
    return bts.Scene(W, H).add(
        bts.Layer.picture(None).named("photo").fit((0, 0, W, H), "cover"),
        bts.Layer.fill("#101018").named("band").fit((0, 480, W, 240), "stretch").opacity(0.82),
        bts.Layer.text(
            "Episode title", font="bricolage_grotesque", size=84, color="#ffffff", width=1080, axes={"wght": 800}
        )
        .named("title")
        .place(dx=100, dy=590),
    )


EPISODES = [
    ("Pilot", Path("photos/a.jpg"), "#101018"),
    ("The long way round", Path("photos/b.jpg"), "#2b0a3d"),
    ("Night shift", Path("photos/c.jpg"), "#0a2b3d"),
    ("Loud and clear", Path("photos/d.jpg"), "#3d0a1f"),
    ("Second wind", Path("photos/e.jpg"), "#0a3d26"),
    ("Last call", Path("photos/f.jpg"), "#3d2f0a"),
]


def episode(title: str, photo: Path | np.ndarray, colour: str) -> dict[str, object]:
    """The addresses of one episode's cover."""
    return {"title.text": title, "photo.picture": photo, "band.color": colour}


def frame(template: bts.Scene, values: dict[str, object], size: int = W) -> np.ndarray:
    """The template with `values` set, drawn at `size`."""
    return template.with_values(values).render(size)


if __name__ == "__main__":
    from bullterrier_image import imageio

    template = build()
    Path("covers").mkdir(exist_ok=True)
    for number, (title, photo, colour) in enumerate(EPISODES, start=1):
        imageio.save(frame(template, episode(title, photo, colour)), f"covers/{number}.png")
