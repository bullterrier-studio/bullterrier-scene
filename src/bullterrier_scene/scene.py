"""`Scene`: a canvas and a stack of layers, drawn into one picture at any size."""

from collections.abc import Callable, Mapping
from pathlib import Path
from typing import Any, Self, TypeVar

import numpy as np
import numpy.typing as npt

from ._native import _core
from .layer import Layer
from .parameter import Addressed, LayerParams, Parameter
from .plates import Plates, Underbase

__all__ = ["Scene"]

# A TypeVar: `Mapping[str | Addressed, Any]` would refuse a `dict[str, ...]` (a mapping is invariant in its key).
_Key = TypeVar("_Key", bound=str | Addressed)


def _number(value: object, whole: bool) -> Any:
    """`value` as an `int` where the parameter is a whole number and the core gave a float; anything else as it is."""
    return int(value) if whole and isinstance(value, float) else value


class Scene:
    """A canvas of `width` by `height` design pixels and a stack of layers, bottom first.

    The sides are whole numbers from 1 to 100,000 (`InvalidError` otherwise). A design pixel is what a length in the
    scene means: `render(size)` draws it at `size / max(width, height)` output pixels. A scene never changes: every call
    returns a new one, and the old one can be used again.
    """

    __slots__ = ("_core",)

    def __init__(self, width: int, height: int) -> None:
        """An empty canvas."""
        self._core = _core.Scene(width, height)

    @classmethod
    def _of(cls, core: _core.Scene) -> Self:
        made = cls.__new__(cls)
        made._core = core  # noqa: SLF001  (a new instance of this class)
        return made

    def __eq__(self, other: object) -> bool:
        """Scenes are equal when they write the same document ("Saved scene" in `docs/api.md`): the same canvas and
        layers, a picture or map in memory by its pixels, a file by its path, a look and a drawing by their documents.
        Equal scenes render the same pictures and list the same parameters. Not equal to anything but a `Scene`."""
        return self._core == other._core if isinstance(other, Scene) else NotImplemented

    __hash__ = None  # type: ignore[assignment]  # a scene is equal by its content, which a hash cannot afford

    def add(self, *layers: Layer) -> Self:
        """This scene with `layers` on top of its stack, the first lowest.

        A `Layer.of(name)` finds the layer called `name` among those of the scene and of this call, in either order;
        it is drawn where it is written. `InvalidError` for a name used twice (the message names both layers), a
        default name (`layer3`) anywhere but at its own place, a reference that names no layer or whose chain leads
        back to itself (also through masks), more than `MAX_LAYERS` layers drawn (the layers, their mask layers and
        references and the layers of each scene used as a layer, each use counted) or leaning on other layers (masks,
        references, scenes as layers) deeper than `MAX_DEPTH`; `TypeError` for anything that is not a `Layer`.
        """
        for layer in layers:
            if not isinstance(layer, Layer):
                raise TypeError(
                    "add takes layers (Layer.picture, Layer.drawing, Layer.text, Layer.fill, Layer.scene, Layer.of; "
                    f"wrap a Scene with Layer.scene), got {type(layer).__name__}"
                )
        return self._of(self._core.add([layer._core for layer in layers]))  # noqa: SLF001  (the layers' own module)

    @property
    def layers(self) -> tuple[Layer, ...]:
        """The layers, bottom first."""
        return tuple(Layer(core) for core in self._core.layers)

    @property
    def width(self) -> int:
        """The canvas's width in design pixels."""
        return self._core.width

    @property
    def height(self) -> int:
        """The canvas's height in design pixels."""
        return self._core.height

    def parameters(self) -> dict[str, Parameter]:
        """Every parameter of the scene by address, in order: layer by layer, the layer's own fields, its source's, its
        looks' and its masks'. The address is `<layer>.<field>` (`title.dx`, `photo.picture`,
        `windows.look.swell.amount`, `logo.drawing.ring_on`, `badge.scene.label.text`, `shadow.mask.dx`); a layer
        without a name is `layer1`, `layer2`... by its place. Each `Parameter` carries the value the scene holds.
        `docs/api.md`, "Values", lists the fields.
        """
        found: dict[str, Parameter] = {}
        for described in self._core.parameters():
            whole = described["kind"] == "int"
            choice = described["kind"] == "choice"
            made = Parameter(
                address=described["address"],
                kind=described["kind"],
                meaning=described["meaning"],
                default=_number(described["default"], whole),
                unit=described["unit"],
                min=_number(described["min"], whole),
                max=_number(described["max"], whole),
                cyclic=described["cyclic"],
                choices=tuple(described["choices"]) if choice else None,
                nullable=described["nullable"],
                min_length=described["min_length"],
                value=_number(described["value"], whole),
            )
            found[made.address] = made
        return found

    def layer(self, name: str) -> LayerParams:
        """The parameters of the layer called `name` as typed handles: `scene.layer("glow").opacity` is
        `scene.parameters()["glow.opacity"]`, and any handle stands for its address in `with_values`. A name no layer
        has is `InvalidError`, with the nearest names; so is a field the layer's kind lacks, when it is asked for.
        """
        found = self.parameters()
        if not any(address.startswith(f"{name}.") for address in found):
            self._refuse(f"{name}.opacity")
        return LayerParams(name, found, self._refuse)

    def _refuse(self, address: str) -> None:
        """Raise the core's refusal of `address` (names and nearest) if no parameter has it."""
        self._core.with_values({address: 0.0})

    def with_values(self, values: Mapping[_Key, Any]) -> Self:
        """A new scene with the parameters at the addresses of `values` set; this scene is not changed.

        A value is a number (a `float`, an `int` or a numpy number; a whole-number parameter takes `3.0`), a colour
        (`"#rrggbb"` or `"#rrggbbaa"`), a string, a choice's name, a bool, or for a picture or a map a `Path`, an
        array (shape (height, width, 3 or 4) for a picture, (height, width) for a map) or `None` (a hole again). A
        value outside its range is refused, never clamped; a cyclic one wraps. Nothing is set unless every value is
        accepted: `InvalidError` names the address, the rule and the value, and for an address no parameter has, the
        addresses there are and the nearest. An address in a look or a drawing is checked by image's or vector's own
        rules. A key is an address or any object with an `address` (an `Addressed`, such as a `Parameter`). A
        `TypeError` for a mapping that is none, a key that is neither, or a value of a type no parameter takes.
        """
        if not isinstance(values, Mapping):
            raise TypeError(f"values: a mapping from addresses to values is wanted, got {type(values).__name__}")
        given: dict[str, Any] = {}
        for key, value in values.items():
            address: str
            if isinstance(key, str):
                address = key
            elif isinstance(getattr(key, "address", None), str):
                address = key.address
            else:
                raise TypeError(f"values: an address is text like 'title.text', got {type(key).__name__}")
            given[address] = value.item() if isinstance(value, np.generic) else value
        return self._of(self._core.with_values(given))

    def to_json(self) -> str:
        """The scene as one self-contained JSON document ("Saved scene" in `docs/api.md`): the canvas and the layers, a
        key only where it differs from its default, a look and a drawing inline, a picture, a map and a font file by its
        path as the scene holds it and the SHA-256 of the file as it is now. The same scene gives the same text; the
        scene writes nothing. `InvalidError` for a picture or a map that is an array (it has no file: save it, or
        leave a hole); `ReadError` for a file whose digest cannot be made.
        """
        return self._core.to_json()

    @classmethod
    def from_json(cls, text: str, *, base: Path | None = None) -> Self:
        """The scene a document written by `to_json` describes.

        A relative path in it is relative to the folder `base` (default: the working folder). No picture or map is read
        now: they are read when the scene renders, and a file that is not there, or whose SHA-256 is not the one saved,
        is a `ReadError` then. A font file, a recipe file and a drawing file named by a path are read now. A document
        that cannot be read is a `FormatError` carrying the JSON Pointer of the value that is wrong: text that is not
        JSON, a key written twice, a version this release does not read, an unknown key, a value of the wrong kind or
        range, a layer the scene refuses, a look or a drawing its crate does not read.
        """
        if not isinstance(text, str):
            raise TypeError(f"text: the JSON text of a scene is wanted, got {type(text).__name__}")
        if base is not None and not isinstance(base, Path):
            raise TypeError(f"base: a Path is wanted, got {type(base).__name__}: pass Path({base!r})")
        return cls._of(_core.Scene.from_json(text, Path() if base is None else base))

    def render(self, size: int) -> npt.NDArray[np.float32]:
        """The picture: a new float32 array (height, width, 4), 0 to 1, straight alpha, rows from the top, sRGB values.

        `size` is the long side in output pixels, a whole number from 1 to 65,000, and the picture has at most 2^27
        pixels; the canvas keeps its aspect and the short side is rounded up. Transparent where nothing is drawn. A
        smaller size shows the same picture in less detail. `InvalidError` for a size out of range, or for a look or a
        drawing its sibling crate refuses at this size (naming the layer); `ReadError` for a file that cannot be read.
        """
        return self._core.render(size)

    def plates(self, size: int, *, underbase: Underbase | None = None) -> Plates:
        """The plates at `size` (the long side in output pixels, as `render`): one plane of ink coverage for each ink
        the
        layers name (`Layer.inks`), in the order the inks first appear, bottom layer first. Plate `i` is what the scene
        draws with only the layers of ink `i` on it, each as the shape of its ink (the plane its look separates, or its
        alpha), placed, masked and faded as in the picture, and joined over the others of that ink as an overprint
        (`a` over `b` is `a + b (1 - a)`); a layer's blend mode is not asked. Laid over the paper by the overprint
        rule the
        plates are the picture. With `underbase` a white plate comes first. Planes of more than 2^27 values in all
        (columns times rows times inks) are refused before anything is drawn. `InvalidError` naming `size`, or `inks`
        for a scene that names no ink, a light layer, a layer with no look naming more than one ink and a look that
        separates no inks or another number than the names, or `name` for an underbase named as an ink.
        """
        arguments = None if underbase is None else underbase._arguments()  # noqa: SLF001
        return Plates._of(self._core.plates(size, arguments))  # noqa: SLF001

    def render_bands(self, size: int, band_rows: int, sink: Callable[[int, npt.NDArray[np.float32]], object]) -> None:
        """The picture of `render`, drawn a band of rows at a time, so that the whole picture is never held.

        `sink(first_row, band)` is called with each band in order, top to bottom: the row it starts at and its float32
        array (rows, width, 4), `band_rows` rows (the last has what is left). The bands put together are
        `render(size)` bit for bit, whatever `band_rows`. A layer with no look is drawn for the rows of a band alone
        where its source can be; a layer with a look is drawn whole once and cut per band. An exception of `sink` ends
        the render and is raised as it is. The 2^27 pixel limit of `render` is on a band here, so a picture past it can
        be drawn this way. `InvalidError` naming `band_rows` for 0, more than the picture's rows or a band of more than
        2^27 pixels, and as `render` for the rest.
        """
        self._core.render_bands(size, band_rows, sink)
