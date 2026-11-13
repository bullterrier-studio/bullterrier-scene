"""C6: one design, many pictures.

One scene with a photo and a title as parameters, written to JSON and read back, then rendered six times with a
different photo, title and colour each. Proves: addresses, `with_values`, `to_json` and `from_json`, a scene built
from paths.
"""

from pathlib import Path

from bullterrier_image import imageio

import bullterrier_scene as bts

W, H = 1280, 720

cover = bts.Scene(W, H).add(
    bts.Layer.picture("photos/placeholder.jpg").named("photo").fit((0, 0, W, H), "cover"),
    bts.Layer.fill("#101018").named("band").fit((0, 480, W, 240), "stretch").opacity(0.82),
    bts.Layer.text(
        "Episode title", font="bricolage_grotesque", size=84, color="#ffffff", width=1080, axes={"wght": 800}
    )
    .named("title")
    .place(dx=100, dy=590),
)

# What a template run may set: `cover.parameters()` lists `photo.picture`, `band.color`, `title.text`, `title.dx`, ...
Path("cover.json").write_text(cover.to_json(), encoding="utf-8")
template = bts.Scene.from_json(Path("cover.json").read_text(encoding="utf-8"), base=".")

episodes = [
    ("Pilot", "photos/a.jpg", "#101018"),
    ("The long way round", "photos/b.jpg", "#2b0a3d"),
    ("Night shift", "photos/c.jpg", "#0a2b3d"),
    ("Loud and clear", "photos/d.jpg", "#3d0a1f"),
    ("Second wind", "photos/e.jpg", "#0a3d26"),
    ("Last call", "photos/f.jpg", "#3d2f0a"),
]
Path("covers").mkdir(exist_ok=True)
for number, (title, photo, colour) in enumerate(episodes, start=1):
    card = template.with_values({"title.text": title, "photo.picture": photo, "band.color": colour})
    imageio.save(card.render(W), f"covers/{number}.png")
