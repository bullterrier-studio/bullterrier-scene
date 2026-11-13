"""bullterrier-scene: puts layers into one picture (see README.md and CLAUDE.md)."""

from importlib import metadata

from .errors import BullterrierSceneError, InvalidError

__version__ = metadata.version("bullterrier-scene")

__all__ = ["BullterrierSceneError", "InvalidError", "__version__"]
