"""Add a kind of layer to bullterrier-scene: a function that draws one rectangle of the canvas.

```python
import bullterrier_scene as bts

@bts.layer_kind("noise")
def noise(x, y, width, height, scale_x, scale_y, *, seed=0):
    '''A grey noise: `ceil(width * scale_x)` columns by `ceil(height * scale_y)` rows.'''
    ...
    return pixels  # a float32 array (rows, columns, 4): straight alpha, values from 0 to 1

layer = bts.Layer.make("noise", seed=3)
```

The scene asks for the rectangle `x, y, width, height` of the canvas, in design pixels, at `scale_x` by `scale_y` output
pixels per design pixel, and takes an array of `ceil(width * scale_x)` columns by `ceil(height * scale_y)` rows (a
product
within 1e-12 of a whole number counts as that number), as `Layer.picture` takes one: `float32` from 0 to 1 is the
contract. A scene refuses what is not that, naming the kind. The keyword arguments are the options of `Layer.make`,
which
a saved scene writes: they are plain JSON data, and the same name, options, rectangle and scale must give the same array
(a kind with a hidden input puts it in its options, or the scene serves a stale picture).

The name is the one `Layer.make`, `layer_kinds()` and a saved scene use (`^[a-z0-9][a-z0-9_.-]*$`, at most 64
characters,
none of the keys that say what a layer draws). Registering the same function again (a reloaded module) replaces it,
another
function under a taken name is refused. A function defined inside another (a closure) is the same function only as the
same
object.

To ship kinds as a package, name the module in an entry point; it is imported the first time the registry is asked
(never at
`import bullterrier_scene`):

```toml
[project.entry-points."bullterrier_scene.layers"]
noise = "my_package.layers"
```

An exception a kind raises reaches the caller of `render` as an `InvalidError` naming the kind and the exception.
"""

import sys
import threading
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from importlib import metadata
from typing import Any

from ._native._core import layer_kinds, registry_restore, registry_snapshot
from ._native._core import register_layer as _register
from .errors import InvalidError

__all__ = ["GROUP", "isolated", "layer_kind", "layer_kinds", "register_layer"]

GROUP = "bullterrier_scene.layers"
"""The entry-point group whose modules are imported to register layer kinds."""

_lock = threading.RLock()
_done = False
_registered_modules: list[str] = []  # the module of each function registered, in order


def _load() -> None:
    """Import the modules of the entry points of `GROUP`, sorted by name, once. One that fails to import leaves nothing
    registered, and the modules that registered kinds during it are forgotten so that the next call imports them
    again; it
    raises an `InvalidError` naming the entry point."""
    global _done  # noqa: PLW0603  (the one flag: the group is read once)
    with _lock:
        if _done:
            return
        for entry in sorted(metadata.entry_points(group=GROUP), key=lambda entry: entry.name):
            before = registry_snapshot()
            registered = len(_registered_modules)
            try:
                entry.load()
            except Exception as error:
                registry_restore(before)
                for module in _registered_modules[registered:]:
                    sys.modules.pop(module, None)
                del _registered_modules[registered:]
                raise InvalidError(f"layer plugin '{entry.name}' ({entry.value}) failed to load: {error}") from error
        _done = True


@contextmanager
def isolated() -> Iterator[None]:
    """For the duration of the block, no kind is registered but what the block registers; the registry as it was is put
    back after it. The registry is process-wide: use it in the tests of a plugin, not from several threads."""
    _load()
    saved = registry_snapshot()
    registry_restore(None)
    try:
        yield
    finally:
        registry_restore(saved)


def _identity(function: Callable[..., object]) -> str:
    """Which function this is: its module and qualified name (a module loaded again replaces itself), and for a
    closure or
    a lambda the object too, as two of them share module and name. The registry keeps the function, so its id cannot be
    reused while it is registered."""
    identity = f"{function.__module__}.{function.__qualname__}"
    return f"{identity}@{id(function):x}" if "<" in function.__qualname__ else identity


def register_layer[F: Callable[..., Any]](kind: str, draw: F) -> F:
    """Register `draw` as the layer kind `kind` (see the module's docstring) and return it unchanged.

    Raises `InvalidError` naming the kind for a bad name, a scene key, a function that cannot be named, or a name
    another
    function holds.
    """
    if not callable(draw):
        raise TypeError(f"layer kind '{kind}': a function is wanted, got {type(draw).__name__}")
    try:
        identity = _identity(draw)
    except AttributeError as error:
        raise InvalidError(f"layer kind '{kind}': cannot name {draw!r}: {error}") from error
    _register(draw, kind, identity)
    _registered_modules.append(draw.__module__)
    return draw


def layer_kind(kind: str) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    """The decorator form of `register_layer`: `@layer_kind("noise")`."""
    return lambda draw: register_layer(kind, draw)
