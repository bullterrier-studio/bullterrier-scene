"""`Scene`: a canvas and a stack of layers, drawn into one picture at any size."""

from collections.abc import Mapping
from typing import Any, Self

import numpy as np
import numpy.typing as npt

from ._native import _core
from .layer import Layer
from .parameter import Parameter

__all__ = ["Scene"]


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

    def with_values(self, values: Mapping[str, Any]) -> Self:
        """A new scene with the parameters at the addresses of `values` set; this scene is not changed.

        A value is a number (a `float`, an `int` or a numpy number; a whole-number parameter takes `3.0`), a colour
        (`"#rrggbb"` or `"#rrggbbaa"`), a string, a choice's name, a bool, or for a picture or a map a `Path`, an
        array (shape (height, width, 3 or 4) for a picture, (height, width) for a map) or `None` (a hole again). A
        value outside its range is refused, never clamped; a cyclic one wraps. Nothing is set unless every value is
        accepted: `InvalidError` names the address, the rule and the value, and for an address no parameter has, the
        addresses there are and the nearest. An address in a look or a drawing is checked by image's or vector's own
        rules. A `TypeError` for a mapping that is none, a key that is not text, or a value of a type no parameter
        takes.
        """
        if not isinstance(values, Mapping):
            raise TypeError(f"values: a mapping from addresses to values is wanted, got {type(values).__name__}")
        given: dict[str, Any] = {}
        for address, value in values.items():
            if not isinstance(address, str):
                raise TypeError(f"values: an address is text like 'title.text', got {type(address).__name__}")
            given[address] = value.item() if isinstance(value, np.generic) else value
        return self._of(self._core.with_values(given))

    def render(self, size: int) -> npt.NDArray[np.float32]:
        """The picture: a new float32 array (height, width, 4), 0 to 1, straight alpha, rows from the top, sRGB values.

        `size` is the long side in output pixels, a whole number from 1 to 65,000, and the picture has at most 2^27
        pixels; the canvas keeps its aspect and the short side is rounded up. Transparent where nothing is drawn. A
        smaller size shows the same picture in less detail. `InvalidError` for a size out of range, or for a look or a
        drawing its sibling crate refuses at this size (naming the layer); `ReadError` for a file that cannot be read.
        """
        return self._core.render(size)
