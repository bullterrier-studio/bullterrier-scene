"""`poe look`: render docs/examples/c1_first_picture.py and write the pictures to open.

Writes dist/look/c1.png (1280 pixels wide, git-ignored) and the gallery piece docs/gallery/c1-first-picture.png (480
wide, committed), and prints the first path. The PNGs are written with the standard library, so the task needs nothing
but the package.
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
EXAMPLE = ROOT / "docs" / "examples" / "c1_first_picture.py"
PIECES = [(1280, ROOT / "dist" / "look" / "c1.png"), (480, ROOT / "docs" / "gallery" / "c1-first-picture.png")]


def png(picture: npt.NDArray[np.float32]) -> bytes:
    """`picture` (height, width, 4; floats 0 to 1, straight alpha) as an 8-bit RGBA PNG."""
    levels = np.rint(picture * 255).astype(np.uint8)
    height, width, _ = levels.shape

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    rows = b"".join(b"\x00" + row.tobytes() for row in levels)
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


def main(pieces: Sequence[tuple[int, Path]] = PIECES) -> int:
    """Render the example at each `(size, path)` of `pieces` and write the PNGs; return the exit code."""
    spec = importlib.util.spec_from_file_location("c1_first_picture", EXAMPLE)
    if spec is None or spec.loader is None:
        print(f"look: cannot load {EXAMPLE}", file=sys.stderr)
        return 1
    example = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(example)
    scene = example.build(example.synthetic_photo())
    for size, path in pieces:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(png(scene.render(size)))
    print(pieces[0][1])
    return 0


if __name__ == "__main__":
    sys.exit(main())
