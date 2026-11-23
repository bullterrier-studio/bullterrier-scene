"""The native answers the WebAssembly package is compared with: each case's scene (`cases/*.json`) read by the Python
package from a folder of the files it names, and rendered.

    uv run python tests/wasm/oracle.py pkg

Writes into OUT/oracle/ the files the scenes name (small pictures and a map made with Pillow, the font of the Rust
tests), the 8-bit RGBA pixels `<case>.rgba` of each render (rounded as a file keeps them: `level * 255 + 0.5` cut) and
`oracle.json`: for each case its scene file, size, the files it names and the width and height of the render. The Node
test (compare.test.mjs) hands the package the same files as bytes and compares its pixels as looking the same.
"""

import json
import shutil
import sys
from pathlib import Path

import numpy as np
import numpy.typing as npt
from PIL import Image, ImageDraw

from bullterrier_scene import Scene

HERE = Path(__file__).parent
FONT = HERE.parent.parent / "crates" / "core" / "tests" / "it" / "fonts" / "marks.ttf"
# case: (the scene file in cases/, the long side it renders at or None for the canvas's own, the files it names)
CASES: dict[str, tuple[str, int | None, list[str]]] = {
    "masked-picture": ("masked-picture.json", 400, ["photo.png", "maps/matte.png"]),
    "drawing-and-type": ("drawing-and-type.json", 300, ["marks.ttf"]),
    "rotated-group": ("rotated-group.json", 360, ["tile.jpg"]),
    "own-size": ("drawing-and-type.json", None, ["marks.ttf"]),
}


def photo(width: int, height: int) -> Image.Image:
    """A picture with gradients, edges and shapes: a resample or a look that goes wrong shows in it."""
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    rgb = np.stack([x / width, y / height, 0.5 + 0.5 * np.sin(x / 9.0) * np.cos(y / 7.0)], axis=-1)
    picture = Image.fromarray((rgb * 255).astype(np.uint8), "RGB")
    draw = ImageDraw.Draw(picture)
    draw.ellipse((width * 0.2, height * 0.15, width * 0.6, height * 0.75), fill=(240, 200, 40))
    draw.rectangle((width * 0.55, height * 0.4, width * 0.9, height * 0.9), fill=(20, 30, 160))
    draw.line((0, height - 1, width - 1, 0), fill=(255, 255, 255), width=3)
    return picture


def matte(width: int, height: int) -> Image.Image:
    """A grey map: white in the middle, falling to black at the edges."""
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    distance = np.hypot((x - width / 2) / (width * 0.45), (y - height / 2) / (height * 0.45))
    return Image.fromarray((np.clip(1.4 - distance, 0.0, 1.0) * 255).astype(np.uint8), "L")


def write_files(folder: Path) -> None:
    """The files the cases name, at the paths their scenes write."""
    (folder / "maps").mkdir(parents=True, exist_ok=True)
    photo(240, 160).save(folder / "photo.png")
    matte(240, 160).save(folder / "maps" / "matte.png")
    photo(180, 120).save(folder / "tile.jpg", quality=90)
    shutil.copy(FONT, folder / "marks.ttf")


def eight_bit(picture: npt.NDArray[np.float32]) -> bytes:
    """The 8-bit samples a file keeps of `picture`: `level * 255 + 0.5` cut to a whole number."""
    return np.clip(picture * 255 + 0.5, 0, 255).astype(np.uint8).tobytes()


def main(out: Path) -> None:
    """Write the oracle into `out`/oracle."""
    folder = out / "oracle"
    folder.mkdir(parents=True, exist_ok=True)
    write_files(folder)
    answers = {}
    for name, (document, size, files) in CASES.items():
        scene = Scene.from_json((HERE / "cases" / document).read_text(encoding="utf-8"), base=folder)
        picture = scene.render(size if size is not None else max(scene.width, scene.height))
        (folder / f"{name}.rgba").write_bytes(eight_bit(picture))
        answers[name] = {
            "scene": document,
            "size": size,
            "files": files,
            "width": picture.shape[1],
            "height": picture.shape[0],
        }
    (folder / "oracle.json").write_text(json.dumps(answers, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
