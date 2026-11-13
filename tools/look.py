"""`poe look`: render the example scenes of docs/examples/ and write the pictures to open.

Each piece is an example file, the function in it that returns the scene to draw, a size and a path. It writes the
large ones to dist/look/ (git-ignored) and the small ones to docs/gallery/ (committed), and prints the first path. The
PNGs are written with the standard library, so the task needs nothing but the package.
"""

import importlib.util
import struct
import sys
import zlib
from collections.abc import Sequence
from pathlib import Path

import numpy as np
import numpy.typing as npt

ROOT = Path(__file__).resolve().parent.parent
EXAMPLES = ROOT / "docs" / "examples"
LOOK = ROOT / "dist" / "look"
GALLERY = ROOT / "docs" / "gallery"
# (example file, the function in it that returns the scene, size, path)
Piece = tuple[str, str, int, Path]
PIECES: list[Piece] = [
    ("c1_first_picture.py", "first_picture", 1280, LOOK / "c1.png"),
    ("c1_first_picture.py", "first_picture", 480, GALLERY / "c1-first-picture.png"),
    ("c2_prints.py", "pile_of_prints", 1280, LOOK / "c2-pile-of-prints.png"),
    ("c2_prints.py", "pile_of_prints", 800, GALLERY / "c2-pile-of-prints.png"),
    ("c2_prints.py", "delivery_crop", 1140, LOOK / "c2-delivery-crop.png"),
    ("c2_prints.py", "delivery_crop", 760, GALLERY / "c2-delivery-crop.png"),
]


def png(picture: npt.NDArray[np.float32]) -> bytes:
    """`picture` (height, width, 4; floats 0 to 1, straight alpha) as an 8-bit RGBA PNG."""
    levels = np.rint(picture * 255).astype(np.uint8)
    height, width, _ = levels.shape

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    rows = b"".join(b"\x00" + row.tobytes() for row in levels)
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


def main(pieces: Sequence[Piece] = PIECES) -> int:
    """Render each of `pieces` and write its PNG; print the first path and return the exit code."""
    modules = {}
    for file, function, size, path in pieces:
        if file not in modules:
            spec = importlib.util.spec_from_file_location(Path(file).stem, EXAMPLES / file)
            if spec is None or spec.loader is None:
                print(f"look: cannot load {file}", file=sys.stderr)
                return 1
            modules[file] = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(modules[file])
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(png(getattr(modules[file], function)().render(size)))
    print(pieces[0][3])
    return 0


if __name__ == "__main__":
    sys.exit(main())
