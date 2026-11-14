"""`Scene`: a canvas and a stack of layers, drawn into one picture at any size."""

from typing import Self

import numpy as np
import numpy.typing as npt

from ._native import _core
from .layer import Layer

__all__ = ["Scene"]


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

    def render(self, size: int) -> npt.NDArray[np.float32]:
        """The picture: a new float32 array (height, width, 4), 0 to 1, straight alpha, rows from the top, sRGB values.

        `size` is the long side in output pixels, a whole number from 1 to 65,000, and the picture has at most 2^27
        pixels; the canvas keeps its aspect and the short side is rounded up. Transparent where nothing is drawn. A
        smaller size shows the same picture in less detail. `InvalidError` for a size out of range, or for a look or a
        drawing its sibling crate refuses at this size (naming the layer); `ReadError` for a file that cannot be read.
        """
        return self._core.render(size)
