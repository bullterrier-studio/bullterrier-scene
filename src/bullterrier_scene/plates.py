"""`Plates`: one plane of ink coverage for each ink of a scene, with the print page they are written on."""

from dataclasses import dataclass
from os import PathLike
from pathlib import Path
from typing import Literal, Self

import numpy as np
import numpy.typing as npt

from ._native._core import Plates as _NativePlates
from ._native._core import check_page, check_underbase

__all__ = ["PdfOptions", "Plates", "Underbase"]


@dataclass(frozen=True, slots=True)
class PdfOptions:
    """The print page the plates are written on: `dpi` fixes the physical size (the picture's width over `dpi` inches),
    `bleed_mm` grows the page on every side with the picture's edge (`bleed_fill` `"repeat"`: the edge pixels stretched;
    `"mirror"`: the picture folded at its edge), and `crop_marks` and `registration_marks` draw marks in an 8 mm margin
    outside the bleed. A number out of its range (`dpi` 1 to 2400, `bleed_mm` 0 to 50) or a `bleed_fill` that is neither
    word is an `InvalidError` naming it where the options are made."""

    dpi: float = 300.0
    bleed_mm: float = 0.0
    bleed_fill: Literal["repeat", "mirror"] = "repeat"
    crop_marks: bool = False
    registration_marks: bool = False

    def __post_init__(self) -> None:
        check_page(self._arguments())

    def _arguments(self) -> tuple[float, float, str, bool, bool]:
        return (self.dpi, self.bleed_mm, self.bleed_fill, self.crop_marks, self.registration_marks)


@dataclass(frozen=True, slots=True)
class Underbase:
    """A white plate for the inks to print on, for a dark garment or film: where ink is, white is laid first so the inks
    show their own colours, except where the picture is the garment's colour (`garment`, `"#rrggbb"`, no alpha). A value
    out of its range, or a `garment` or `name` that is not allowed, is an `InvalidError` naming it where the underbase
    is made.

    `choke` draws the white in from the edge of the ink by that many design pixels, so no white shows past the ink where
    the plates are a little out of register (0 to 10,000). `tolerance` (0 to 1, the distance between two colours as a
    share of black to white) is how far from `garment` a colour may be and still count as the garment's: there the white
    is knocked out. `feather` blurs the edge of the knockout by that many design pixels. `name` is the plate's name.
    """

    garment: str
    choke: float = 0.0
    tolerance: float = 0.0
    feather: float = 0.0
    name: str = "white"

    def __post_init__(self) -> None:
        check_underbase(self._arguments())

    def _arguments(self) -> tuple[str, float, float, float, str]:
        return (self.garment, self.choke, self.tolerance, self.feather, self.name)


class Plates:
    """The ink coverage of a scene, one plane for each ink, at the size the scene was drawn at: `Scene.plates(size)`.

    `plane(name)` is a float32 array (height, width), 0 to 1: how much of the ink lies on each pixel. Laid over the
    paper
    by the overprint rule (`paper * prod(1 - coverage * (1 - ink / paper))`) the plates are the picture. A plates value
    never changes.
    """

    __slots__ = ("_core",)
    _core: _NativePlates

    def __init__(self) -> None:
        """Plates are made by `Scene.plates`."""
        raise TypeError("Plates are made by Scene.plates(size)")

    @classmethod
    def _of(cls, core: _NativePlates) -> Self:
        made = cls.__new__(cls)
        made._core = core  # noqa: SLF001  (a new instance of this class)
        return made

    def __eq__(self, other: object) -> bool:
        """Plates are equal when they hold the same inks (names and order) and the same values."""
        return self._core == other._core if isinstance(other, Plates) else NotImplemented

    __hash__ = None  # type: ignore[assignment]  # equal by content

    @property
    def names(self) -> tuple[str, ...]:
        """The inks, in plate order: a white underbase first, then the inks as the layers name them, bottom layer
        first."""
        return tuple(self._core.names)

    @property
    def width(self) -> int:
        """The columns of every plane."""
        return self._core.width

    @property
    def height(self) -> int:
        """The rows of every plane."""
        return self._core.height

    def plane(self, name: str) -> npt.NDArray[np.float32]:
        """The coverage of the ink `name` as a new float32 array (height, width), 0 to 1, rows from the top.
        `InvalidError` naming `name` for an ink the plates do not have."""
        return self._core.plane(name)

    def write_png(self, stem: str | PathLike[str], options: PdfOptions | None = None) -> list[Path]:
        """Writes each plate as an 8-bit grey PNG, `<stem>-<ink>.png` (`out/poster` gives `out/poster-pink.png`), and
        returns the paths in plate order. Ink is black on white paper. A plate is its plane with the bleed of
        `options`, and
        with marks 8 mm of margin with the marks, at the same pixels on every plate; the file says the `dpi`. Default
        `PdfOptions()`: 300 dpi, no bleed, no marks. `InvalidError` naming the argument for a value out of range, a page
        past 200 in (`dpi`) or a plate with its bleed past 2^27 pixels (`bleed_mm`); a path that cannot be written is
        a `WriteError` naming it (the plates before it stay written)."""
        used = options or PdfOptions()
        return [Path(path) for path in self._core.write_png(Path(stem), used._arguments())]  # noqa: SLF001

    def write_pdf(self, path: str | PathLike[str], options: PdfOptions | None = None) -> None:
        """Writes the plates as one PDF with a page for each, in plate order, named by its ink (the page labels a PDF
        reader shows): the page is the physical size, the bleed, the boxes and the marks as vector paths of
        `options`, the
        plate one 8-bit grey image (black ink on white). The file is made whole or not at all."""
        used = options or PdfOptions()
        self._core.write_pdf(Path(path), used._arguments())  # noqa: SLF001

    def __repr__(self) -> str:
        return f"Plates({self.width} x {self.height}, inks={list(self.names)})"
