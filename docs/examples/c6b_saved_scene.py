"""C6b: save a scene, read it back, draw both.

A scene of a photo (a file), a band and a title is written with `to_json`, the picture's path made relative to the scene
file's folder (as a person moving a project would), read back with `Scene.from_json(text, base=folder)` and drawn. The
two scenes are equal and draw the same bits. Proves: the document is self-contained but for its picture files
(named by path and SHA-256), `==` is the same document, and nothing is read until `render`. The photo is made here from
arithmetic (no third-party picture).
"""

import json
import sys
import tempfile
from collections.abc import Callable
from pathlib import Path

import numpy as np

import bullterrier_scene as bts

W, H = 320, 180


def photo() -> np.ndarray:
    """A float32 picture of `H` by `W` pixels, made from arithmetic: a diagonal ramp with a disc."""
    y, x = np.mgrid[0:H, 0:W].astype(np.float32)
    disc = np.clip(1.5 - np.hypot(x - 0.7 * W, y - 0.4 * H) / (0.18 * H), 0, 1)
    return np.stack([x / W * (1 - disc) + disc, y / H * (1 - disc) + 0.8 * disc, 0.4 + 0.3 * disc], axis=-1).astype(
        np.float32
    )


def build(picture: Path) -> bts.Scene:
    """The scene: the photo in a file, a band over its lower third and a title on the band."""
    return bts.Scene(W, H).add(
        bts.Layer.picture(picture).named("photo").fit((0, 0, W, H), "cover"),
        bts.Layer.fill("#101018").named("band").fit((0, 120, W, 60), "stretch").opacity(0.82),
        bts.Layer.text("Saved scene", font="bricolage_grotesque", size=28, color="#ffffff", axes={"wght": 800})
        .named("title")
        .place(dx=16, dy=160),
    )


def save_png(pixels: np.ndarray, path: Path) -> Path:
    """`pixels` as a picture file, written by image's `imageio`."""
    from bullterrier_image import imageio

    imageio.save(pixels, path)
    return path


def cover_by_values() -> bts.Scene:
    """The committed `cover.scene.json` with three values set: what `bt-scene render` draws with the same three `--set`
    (`docs/cli.md`; the gallery piece `p1-cli.png`)."""
    cover = Path(__file__).with_name("cover.scene.json")
    saved = bts.Scene.from_json(cover.read_text(encoding="utf-8"), base=cover.parent)
    return saved.with_values({"title.text": "From the command line", "band.opacity": 0.6, "band.color": "#2b0a3d"})


def saved_and_loaded(
    folder: Path,
    save: Callable[[np.ndarray, Path], Path] = save_png,
    files: tuple[str, str] = ("photo.png", "scene.json"),
) -> tuple[bts.Scene, bts.Scene]:
    """Save the scene in `folder` and read it back there: the scene and the scene read back (equal). `files` names the
    picture file and the scene file."""
    picture_file, scene_file = files
    scene = build(save(photo(), folder / picture_file))
    document = json.loads(scene.to_json())
    document["layers"][0]["picture"]["path"] = picture_file  # relative to the folder the scene file is in
    (folder / scene_file).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    loaded = bts.Scene.from_json((folder / scene_file).read_text(encoding="utf-8"), base=folder)
    return scene, loaded


if __name__ == "__main__":
    if len(sys.argv) > 1:  # `python c6b_saved_scene.py docs/examples` writes cover-photo.png and cover.scene.json there
        saved_and_loaded(Path(sys.argv[1]), files=("cover-photo.png", "cover.scene.json"))
    else:
        with tempfile.TemporaryDirectory() as folder:
            scene, loaded = saved_and_loaded(Path(folder))
            save_png(loaded.render(W), Path("saved-scene.png"))  # the picture of the scene read back
