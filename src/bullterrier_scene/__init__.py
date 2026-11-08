"""bullterrier-scene: a placeholder Rust-first project with a Python interface (see README.md and CLAUDE.md)."""

from importlib import metadata

from ._native._core import scale
from .errors import BullterrierSceneError, InvalidError

__version__ = metadata.version("bullterrier-scene")

__all__ = ["BullterrierSceneError", "InvalidError", "__version__", "scale"]
