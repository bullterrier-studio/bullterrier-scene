"""The exceptions bullterrier-scene raises: catch `BullterrierSceneError` for every failure it reports on purpose.

Library code raises a subclass of it, never a bare builtin, for anything a caller or an input file can get wrong; the
message says what was wrong and what is valid. A subclass also inherits the builtin it replaces (`ValueError`, ...), so
code that already catches that builtin keeps working.

Dependency direction: the bottom layer. It imports nothing from bullterrier_scene, so every layer may raise them.
"""

__all__ = ["BullterrierSceneError", "InvalidError"]


class BullterrierSceneError(Exception):
    """The base of every exception bullterrier-scene raises on purpose."""


class InvalidError(BullterrierSceneError, ValueError):
    """A value breaks a rule: the message names the argument and what is valid."""
