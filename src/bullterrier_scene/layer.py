"""`Layer`: one layer of a scene, and what it takes of the objects the sibling packages write.

A look or a drawing arrives by its serialised form (`to_dict()`, `to_json()`), never as the other package's object: this
package links its own copy of image and vector and imports neither. This module turns what the caller holds into the
data the Rust core takes; the core checks it.
"""

import json
import math
from collections.abc import Callable, Mapping, Sequence
from numbers import Real
from pathlib import Path
from typing import Any, Self

import numpy as np
import numpy.typing as npt

from ._native import _core
from .errors import InvalidError, ReadError

__all__ = ["Layer"]


def _document(what: str, data: Mapping[str, Any]) -> str:
    """`data` as JSON text; a value that is not plain data (a `Picture` or `Lut` parameter) is an `InvalidError`."""
    try:
        return json.dumps(data, allow_nan=False)
    except (TypeError, ValueError) as error:
        raise InvalidError(f"{what}: not plain data ({error}); give a path in a recipe file") from error


def _read(what: str, path: Path) -> str:
    """The text of the file `path`; one that cannot be read is a `ReadError`."""
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise ReadError(f"{what}: {path}: {error.strerror or error}") from error


def _shown(value: object) -> str:
    """`value` as text for a message, shortened: an integer of hundreds of digits is not printed whole."""
    text = repr(value)
    return text if len(text) <= 40 else f"{text[:30]}... ({len(text)} characters)"


def _number(name: str, value: object) -> float:
    """`value` as a finite float. A `bool` or a non-number is a `TypeError` naming `name`; an integer beyond the range
    of a float, or a number that is not finite, is an `InvalidError` naming `name` and the value."""
    if isinstance(value, (bool, np.bool_)) or not isinstance(value, Real):
        raise TypeError(f"{name}: a number is wanted, got {type(value).__name__} {_shown(value)}")
    try:
        number = float(value)
    except OverflowError:
        raise InvalidError(f"{name}: {_shown(value)} is beyond the range of a float") from None
    if not math.isfinite(number):
        raise InvalidError(f"{name}: {number} is not a finite number")
    return number


def _flag(name: str, value: object) -> bool:
    """`value` as a bool; anything else, a number included, is a `TypeError` naming `name`."""
    if not isinstance(value, (bool, np.bool_)):
        raise TypeError(f"{name}: a bool is wanted, got {type(value).__name__} {_shown(value)}")
    return bool(value)


def _items[T](name: str, value: object, wanted: str, one: Callable[[str, object], T], size: int) -> tuple[T, ...]:
    """`value` as a tuple of `size` items, each converted by `one`. Anything that is not a list, a tuple or an array of
    that length is a `TypeError` naming `name` and `wanted`, the shape it takes."""
    if not isinstance(value, (list, tuple, np.ndarray)) or len(value) != size:
        raise TypeError(f"{name}: {wanted} is wanted, got {_shown(value)}")
    return tuple(one(name, item) for item in value)


def _pair(name: str, value: object) -> tuple[float, ...]:
    """`value` as an `(x, y)` pair of finite floats; anything else is an error naming `name`."""
    return _items(name, value, "(x, y), two numbers", _number, 2)


class Layer:
    """One layer of a scene: a source, and how it joins the layers below it.

    A layer never changes: every call returns a new one, and a layer can be used in several scenes. Make one with
    `Layer.picture`, `Layer.drawing`, `Layer.text` or `Layer.fill`.
    """

    __slots__ = ("_core",)

    def __init__(self, core: _core.Layer) -> None:
        """Wrap the core's layer; callers make layers with the constructors below."""
        self._core = core

    @classmethod
    def picture(
        cls, source: npt.NDArray[Any] | Path, *, maps: Mapping[str, npt.NDArray[Any] | Path | None] | None = None
    ) -> Self:
        """A picture: an array or a `pathlib.Path` read when the scene renders (upright, sRGB, alpha kept).

        The array is (height, width, 3 or 4) of `uint8`, `uint16` or floats 0 to 1; four channels are straight alpha.
        Its box is its pixels, one design pixel each, at the canvas origin; what lies outside the canvas is cut. A `str`
        is a `TypeError`: pass `Path(...)`. An array of another shape, or a value outside 0 to 1, is an `InvalidError`.

        `maps` are the picture's named maps, each a value 0 to 1 for each pixel of the picture: an array (height, width)
        of `uint8`, `uint16` or floats, a `Path` to a grey picture read when the scene renders, or `None`, a hole that
        a render refuses until parameters fill holes. A map is cut and fitted with the picture; a look sees the maps
        its recipe uses (`subject`, `depth`, a `given` region) and `mask("name")` multiplies by one. A map of another
        size than its picture, or a hole, is an `InvalidError` at render naming the layer and the map; a name that
        is not letters, digits and `_` starting with a letter, an `InvalidError` at the call.
        """
        if isinstance(source, str):
            raise TypeError(f"a picture is an array or a Path, got the text {source!r}: pass Path({source!r})")
        if maps is not None and not isinstance(maps, Mapping):
            raise TypeError(
                "maps: a mapping from names to arrays, Paths or None is wanted, "
                f"got {type(maps).__name__} {_shown(maps)}"
            )
        made = cls(_core.Layer.picture_file(source) if isinstance(source, Path) else _core.Layer.picture(source))
        for name, given in (maps or {}).items():
            if not isinstance(name, str):
                raise TypeError(f"maps: a name is text, got {type(name).__name__} {_shown(name)}")
            if isinstance(given, str):
                raise TypeError(f"map {name!r} is an array or a Path, got the text {given!r}: pass Path({given!r})")
            if given is None:
                made = type(made)(made._core.map_hole(name))
            elif isinstance(given, Path):
                made = type(made)(made._core.map_file(name, given))  # noqa: SLF001  (a layer of this class)
            else:
                made = type(made)(made._core.map(name, given))  # noqa: SLF001  (a layer of this class)
        return made

    @classmethod
    def drawing(cls, source: Any) -> Self:
        """A vector drawing: anything with `to_json()` (a vector `Drawing`), its JSON text or dict, or a `Path`.

        Its box is the drawing's frame at the canvas origin: a drawing without a frame is an `InvalidError`. A document
        the linked vector crate cannot read is a `FormatError`. Anything else, a bare vector path included, is a
        `TypeError`: wrap it in `Drawing`.
        """
        if isinstance(source, Path):
            text = _read("drawing", source)
        elif isinstance(source, str):
            text = source
        elif isinstance(source, Mapping):
            text = _document("drawing", source)
        elif callable(getattr(source, "to_json", None)):
            text = source.to_json()
        else:
            raise TypeError(
                "a drawing is a vector Drawing (anything with to_json()), its JSON text or dict, or a Path to a file, "
                f"got {type(source).__name__}: wrap a vector path in Drawing(...)"
            )
        return cls(_core.Layer.drawing(text))

    @classmethod
    def text(cls, content: str, *, font: str | Path, color: str = "#000000", **text_options: Any) -> Self:
        """`content` set in type: a bundled font's name (a `str`) or a `Path` to a font file read now, and a colour.

        `text_options` are vector's `text` arguments as plain data (`size`, `axes`, `tracking`, `leading`, `align`,
        `width`, `fit`, `features`), checked by the linked vector crate now; scene restates none of its defaults.
        `width` and `fit` are vector's: the width the text wraps or fits to, not a placement (`fit(...)` on the layer
        is the placement). `anchor`, `inside`, `runs`, expressions and Params are an `InvalidError`: scene places
        the text. The layer's origin is the text's anchor, the left end, middle or right end of the first baseline by
        `align`, so `place(dx=100, dy=420)` puts it there; its box is the glyphs' bounds. An unknown bundled font is
        an `InvalidError` listing the fonts there are; a font file that cannot be read, a `ReadError`.
        """
        if not isinstance(content, str):
            raise TypeError(f"content: text is wanted, got {type(content).__name__} {_shown(content)}")
        if not isinstance(color, str):
            raise TypeError(f'color: a text like "#rrggbb" is wanted, got {type(color).__name__} {_shown(color)}')
        try:
            content.encode("utf-8")
        except UnicodeEncodeError as error:
            raise InvalidError(
                f"content: text that UTF-8 can hold is wanted ({error.reason} at {error.start})"
            ) from error
        options = _document("text", text_options)
        if isinstance(font, Path):
            return cls(_core.Layer.text_file(content, font, color, options))
        if isinstance(font, str):
            return cls(_core.Layer.text(content, font, color, options))
        raise TypeError(f"font: a bundled font's name or a Path to a font file is wanted, got {type(font).__name__}")

    @classmethod
    def fill(cls, color: str) -> Self:
        """A flat colour over the whole canvas: `"#rrggbb"` or `"#rrggbbaa"`, sRGB values, straight alpha."""
        return cls(_core.Layer.fill(color))

    def named(self, name: str) -> Self:
        """This layer called `name`: letters, digits and `_`, starting with a letter, unique in its scene.

        A layer without a name is `layer1`, `layer2`... by its place in the scene. Another text is an `InvalidError`.
        """
        return type(self)(self._core.named(name))

    def fit(
        self, box: tuple[float, float, float, float], how: str = "cover", *, align: tuple[float, float] = (0.5, 0.5)
    ) -> Self:
        """This layer's box fitted into `box` = `(x, y, width, height)` in design pixels; the last `fit` wins.

        `how`: `"cover"` scales it, its aspect kept, to fill `box` and cuts the overflow off at `box`'s edge (the
        delivery crop); `"contain"` scales it to lie wholly inside; `"stretch"` fills `box` exactly, its aspect not
        kept. `align` is two fractions from 0 to 1 of the layer's box: which part stays when cover cuts, and where a
        contained layer sits in the slack. It is resolved when the scene renders, from the source's real size. `place`
        then acts on the fitted box. `InvalidError` for another word, a `box` with a width or height not above 0 or
        a number that is not finite, an `align` outside 0 to 1.
        """
        if not isinstance(how, str):
            raise TypeError(f'how: "cover", "contain" or "stretch" is wanted, got {type(how).__name__} {_shown(how)}')
        area = _items("box", box, "(x, y, width, height), four numbers", _number, 4)
        return type(self)(self._core.fit(area, how, _items("align", align, "(x, y), two numbers", _number, 2)))

    def place(
        self,
        *,
        dx: float | None = None,
        dy: float | None = None,
        scale: float | None = None,
        aspect: float | None = None,
        rotate: float | None = None,
        skew: tuple[float, float] | None = None,
        flip: tuple[bool, bool] | None = None,
        anchor: tuple[float, float] | None = None,
    ) -> Self:
        """This layer with the placement fields given set and the others kept.

        The finished raster is flipped by `flip` `(x, y)` (booleans), scaled by `scale` and by `aspect` along y (the
        height ratio: y scales by `scale` times `aspect`), skewed by `skew` `(x, y)` degrees (x, then y), and rotated
        by `rotate` (degrees clockwise, any size, wrapping), all about `anchor` (fractions 0 to 1 of the fitted box,
        default the centre), then moved by `dx`, `dy` design pixels. Defaults: no change. Each number has a range (the
        Placement table of `docs/api.md`); one outside it, or not finite, is an `InvalidError` that names the range.
        """
        return type(self)(
            self._core.place(
                dx=None if dx is None else _number("dx", dx),
                dy=None if dy is None else _number("dy", dy),
                scale=None if scale is None else _number("scale", scale),
                aspect=None if aspect is None else _number("aspect", aspect),
                rotate=None if rotate is None else _number("rotate", rotate),
                skew=None if skew is None else _items("skew", skew, "(x, y) degrees, two numbers", _number, 2),
                flip=None if flip is None else _items("flip", flip, "(x, y), two bools", _flag, 2),
                anchor=None if anchor is None else _items("anchor", anchor, "(x, y), two numbers", _number, 2),
            )
        )

    def perspective(self, corners: Sequence[tuple[float, float]]) -> Self:
        """This layer with the corners of its fitted box put on `corners`, replacing what `place` says.

        `corners` are four `(x, y)` points in design pixels of the canvas, where the fitted box's top left, top right,
        bottom right and bottom left land. The last of `place` and `perspective` called wins; `fit` comes first. The
        corners must make a strictly convex quadrilateral (no dart, no bowtie, no three on a line, no corner twice);
        one that turns counter-clockwise is accepted, the box seen from behind. The layer is drawn at the largest scale
        the quadrilateral gives it, so its near side is sharp (cut down if that would pass the 2^27 pixels of a layer),
        and its far side is filtered. A wrong shape or type is a `TypeError`, and a number beyond float range, beyond
        -1e6 to 1e6 or not finite, or corners that are no such quadrilateral, an `InvalidError` naming `corners`.
        """
        rows = _items("corners", corners, "four (x, y) pairs", _pair, 4)
        return type(self)(self._core.perspective(rows))

    def mask(self, by: Self | str, *, use: str = "alpha", feather: float = 0, invert: bool = False) -> Self:
        """This layer with its alpha multiplied by a mask; call it again to multiply another in.

        `by` is a `Layer`, drawn on the canvas as its own `fit` and `place` say, or the name of one of this picture
        layer's `maps`, cut and fitted with the picture. A mask layer's own alpha includes its masks, its opacity
        multiplies the mask, and it has no name (`InvalidError`); masks nest at most `MAX_DEPTH` deep. `use` reads
        the mask layer as `"alpha"` or as `"luminance"` (0.2126 R + 0.7152 G + 0.0722 B of the sRGB values, times the
        alpha: white keeps, black hides); a map is its own values, `"alpha"` only. `feather` blurs the mask's edge by a
        Gaussian of that standard deviation in design pixels (0 to 1,000,000; 0 is hard; three box passes within 0.0099
        of the Gaussian's distribution function), before `invert` makes the mask 1 minus itself. `InvalidError` names
        the field for a name that is no map of this layer, `use` another word, a `feather` out of range; a `TypeError`
        for a wrong type.
        """
        if not isinstance(use, str):
            raise TypeError(f'use: "alpha" or "luminance" is wanted, got {type(use).__name__} {_shown(use)}')
        blur, flipped = _number("feather", feather), _flag("invert", invert)
        if isinstance(by, Layer):
            return type(self)(self._core.mask(by._core, use, blur, flipped))  # noqa: SLF001  (a layer)
        if isinstance(by, str):
            return type(self)(self._core.mask(by, use, blur, flipped))
        raise TypeError(f"by: a Layer or the name of one of this layer's maps is wanted, got {type(by).__name__}")

    def opacity(self, value: float) -> Self:
        """This layer at `value` of its own alpha, from 0 (hidden) to 1 (the default); `InvalidError` outside it."""
        return type(self)(self._core.opacity(_number("opacity", value)))

    def blend(self, mode: str) -> Self:
        """This layer joined to what is below by `mode`, one of the W3C compositing modes by its CSS name.

        `"normal"` (the default), `"multiply"`, `"screen"`, `"overlay"`, `"darken"`, `"lighten"`, `"color-dodge"`,
        `"color-burn"`, `"hard-light"`, `"soft-light"`, `"difference"`, `"exclusion"`, `"hue"`, `"saturation"`,
        `"color"` or `"luminosity"`, in lower case with hyphens. Colours are blended as sRGB values, at the layer's
        alpha after its masks and its opacity. Another word, `"Multiply"` and `"soft_light"` included, is an
        `InvalidError` naming `blend`, the sixteen names and the nearest; a `mode` that is not a `str` is a `TypeError`.
        A look's own `blend_mode` (image's spelling, `soft_light`) is inside the look and stays image's.
        """
        if not isinstance(mode, str):
            raise TypeError(
                f'blend: a mode name such as "multiply" is wanted, got {type(mode).__name__} {_shown(mode)}'
            )
        return type(self)(self._core.blend(mode))

    def as_light(self, on: bool = True) -> Self:
        """This layer made light (`on=True`, the default), or not (`on=False`, which is no call).

        A light layer's premultiplied colour, after its looks and masks, is its colour over black, and its alpha is the
        largest of the red, green and blue of that colour: black adds nothing, a `"screen"` of the layer over black
        gives that colour exactly, and white is opaque white. A layer that is not light keeps the alpha of its source. A
        mask layer cannot be light (`InvalidError` at `mask`). A value that is not a `bool` is a `TypeError`.
        """
        return type(self)(self._core.as_light(_flag("on", on)))

    def look(self, recipe: Any) -> Self:
        """This layer with an image look run on it, after the looks it has.

        `recipe` is anything with `to_dict()` (an image `Recipe`), the dict it writes, or a `Path` to a recipe file read
        now; a `str` is a `TypeError`: pass `Path(...)`. The look runs on the picture at the output size and keeps its
        alpha; only a picture layer takes one. A recipe image refuses, or that sets `size` or `frame`, is an
        `InvalidError` in image's words; a recipe `format` the linked image crate does not read, a `FormatError`.
        """
        if isinstance(recipe, Path):
            return type(self)(self._core.look_file(recipe))
        if isinstance(recipe, str):
            raise TypeError(f"a look is a Recipe, its dict or a Path, got the text {recipe!r}: pass Path({recipe!r})")
        if not isinstance(recipe, Mapping):
            if not callable(getattr(recipe, "to_dict", None)):
                raise TypeError(
                    f"a look is a Recipe (anything with to_dict()), its dict or a Path, got {type(recipe).__name__}"
                )
            recipe = recipe.to_dict()
        return type(self)(self._core.look(_document("look", recipe)))
