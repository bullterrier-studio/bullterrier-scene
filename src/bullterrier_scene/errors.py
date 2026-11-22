"""The exceptions bullterrier-scene raises: catch `BullterrierSceneError` for every failure it reports on purpose.

Library code raises a subclass of it, never a bare builtin, for anything a caller or an input file can get wrong; the
message says what was wrong and what is valid. A subclass also inherits the builtin it replaces (`ValueError`, ...), so
code that already catches that builtin keeps working.

Dependency direction: the bottom layer. It imports nothing from bullterrier_scene, so every layer may raise them.
"""

__all__ = ["BullterrierSceneError", "FormatError", "GpuError", "InvalidError", "ReadError", "WriteError"]


class BullterrierSceneError(Exception):
    """The base of every exception bullterrier-scene raises on purpose."""


class InvalidError(BullterrierSceneError, ValueError):
    """A value or an argument breaks a rule: the message names the argument or layer, the rule and the value."""


class FormatError(BullterrierSceneError, ValueError):
    """A document cannot be read: a saved scene (`Scene.from_json`), or a look or a drawing the linked sibling crate
    does not read, a version it does not read included. The message starts with the JSON Pointer of the value that is
    wrong."""


class ReadError(BullterrierSceneError, OSError):
    """A file the scene names cannot be read, or is not the file the scene was saved with (its SHA-256 differs): the
    message names the layer, the file and the reason."""


class WriteError(BullterrierSceneError, OSError):
    """A file cannot be written: the folder is not there, the path cannot be written to, or the file is no format the
    writer knows (the message names the path and the reason). No file is made."""


class GpuError(BullterrierSceneError, RuntimeError):
    """This machine cannot do what was asked on the GPU: no adapter, a limit it lacks, a picture larger than the device
    holds, or a render the GPU does not do (print output). Never a caller's mistake: `except GpuError` and render on the
    CPU hides no bug."""
