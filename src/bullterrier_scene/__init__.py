"""bullterrier-scene: puts layers (pictures, image looks, vector drawings) together into one picture.

A `Scene` is a canvas and a stack of `Layer`s; `Scene.render(size)` draws the whole picture at any size as a float32
array. The public names are listed in `__all__`.
"""

from importlib import metadata

from ._native._core import MAX_LAYERS
from .errors import BullterrierSceneError, FormatError, InvalidError, ReadError
from .layer import Layer
from .scene import Scene

__version__ = metadata.version("bullterrier-scene")

__all__ = [
    "MAX_LAYERS",
    "BullterrierSceneError",
    "FormatError",
    "InvalidError",
    "Layer",
    "ReadError",
    "Scene",
    "__version__",
]
