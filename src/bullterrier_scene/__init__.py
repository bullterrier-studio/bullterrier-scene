"""bullterrier-scene: puts layers (pictures, image looks, vector drawings) together into one picture.

A `Scene` is a canvas and a stack of `Layer`s; `Scene.render(size)` draws the whole picture at any size as a float32
array. The public names are listed in `__all__`.
"""

from importlib import metadata

from ._native._core import MAX_DEPTH, MAX_LAYERS
from .errors import BullterrierSceneError, FormatError, InvalidError, ReadError
from .layer import Layer
from .parameter import Parameter
from .plugin import layer_kind, layer_kinds, register_layer
from .scene import Scene


def __getattr__(name: str) -> str:
    """`__version__`, the installed distribution's version, read when it is asked for: a source checkout that is not
    installed imports and works, and only asking its version fails (`PackageNotFoundError`)."""
    if name == "__version__":
        return metadata.version("bullterrier-scene")
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


__all__ = [
    "MAX_DEPTH",
    "MAX_LAYERS",
    "BullterrierSceneError",
    "FormatError",
    "InvalidError",
    "Layer",
    "Parameter",
    "ReadError",
    "Scene",
    "layer_kind",
    "layer_kinds",
    "register_layer",
]
