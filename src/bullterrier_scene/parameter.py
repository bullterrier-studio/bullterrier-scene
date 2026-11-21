"""`Parameter`: one value of a scene that a caller may set, as `Scene.parameters()` describes it."""

from collections.abc import Callable, Mapping
from dataclasses import dataclass
from typing import Literal, Protocol

__all__ = ["Addressed", "LayerParams", "Parameter", "PartParams"]

Kind = Literal["float", "int", "bool", "choice", "color", "text", "picture"]


@dataclass(frozen=True, slots=True)
class Parameter:
    """What a scene says of one parameter: the fields of the ecosystem's parameter format, with `address` standing for
    its `name` (a dotted address is not a valid `name`), and the value the scene holds now.

    `kind` is `"float"`, `"int"`, `"bool"`, `"choice"`, `"color"`, `"text"` or `"picture"`. `unit` is `"px"`,
    `"fraction"`, `"degrees"`, `"ratio"`, `"cycles"`, `"count"`... for a number, else `None`; `min` and `max` bound a
    number (inclusive: for a cyclic one, where its turn starts and ends); `choices` are the names of a choice;
    `nullable` is true for a picture or a map (no value is `None`); `min_length` is the fewest characters of a text.
    For a text, a colour and the numbers of a look `default` is the value the scene holds (the value it was built with
    is not kept). `value` of a picture or a map held in memory is a handle that shares its pixels:
    `numpy.asarray(value)` makes the array, and `with_values` takes it back; of one read from a file, the `Path`.
    """

    address: str
    kind: Kind
    meaning: str
    default: object
    unit: str | None
    min: float | None
    max: float | None
    cyclic: bool
    choices: tuple[str, ...] | None
    nullable: bool
    min_length: int | None
    value: object

    def to_dict(self) -> dict[str, object]:
        """The description as the parameter format writes it (`address` for `name`; no `value`): what a slider builder
        reads."""
        written: dict[str, object] = {
            "address": self.address,
            "kind": self.kind,
            "meaning": self.meaning,
            "default": self.default,
            "unit": self.unit,
            "min": self.min,
            "max": self.max,
            "cyclic": self.cyclic,
        }
        if self.choices is not None:
            written["choices"] = list(self.choices)
        if self.min_length is not None:
            written["min_length"] = self.min_length
        if self.nullable:
            written["nullable"] = True
        return written


class Addressed(Protocol):
    """Anything with an `address`: a `Parameter`, or a handle of a caller's own. It stands wherever an address does."""

    @property
    def address(self) -> str:
        """The address, as `Scene.parameters()` lists it."""
        ...


class PartParams:
    """The parameters of one part of a layer that the caller names (a drawing's, one look's, one mask's): `part[name]`
    is the `Parameter` of `Scene.parameters()` at `<layer>.<part>.<name>`, or `InvalidError` with the nearest names."""

    def __init__(self, prefix: str, found: Mapping[str, Parameter], refuse: Callable[[str], None]) -> None:
        self._prefix, self._found, self._refuse = prefix, found, refuse

    def __getitem__(self, name: str) -> Parameter:
        address = f"{self._prefix}.{name}"
        if address not in self._found:
            self._refuse(address)  # raises the scene's own refusal, with the nearest addresses
            raise KeyError(address)
        return self._found[address]


class LayerParams(PartParams):
    """The parameters of one layer, from `Scene.layer(name)`: a property for each field a layer may have, each the
    `Parameter` of `Scene.parameters()`. A field the layer's kind lacks (a fill has no `text`) raises `InvalidError`
    with the nearest names. `drawing`, `look(n)`, `mask(n)` and `maps` look up the parts a caller names."""

    def __init__(self, name: str, found: Mapping[str, Parameter], refuse: Callable[[str], None]) -> None:
        super().__init__(name, found, refuse)
        self._part = lambda part: PartParams(f"{name}.{part}", found, refuse)

    @staticmethod
    def _numbered(part: str, n: int) -> str:
        return part if n == 1 else f"{part}#{n}"

    @property
    def opacity(self) -> Parameter:
        """`<layer>.opacity`."""
        return self["opacity"]

    @property
    def blend(self) -> Parameter:
        """`<layer>.blend`."""
        return self["blend"]

    @property
    def light(self) -> Parameter:
        """`<layer>.light`."""
        return self["light"]

    @property
    def dx(self) -> Parameter:
        """`<layer>.dx`."""
        return self["dx"]

    @property
    def dy(self) -> Parameter:
        """`<layer>.dy`."""
        return self["dy"]

    @property
    def scale(self) -> Parameter:
        """`<layer>.scale`."""
        return self["scale"]

    @property
    def aspect(self) -> Parameter:
        """`<layer>.aspect`."""
        return self["aspect"]

    @property
    def rotate(self) -> Parameter:
        """`<layer>.rotate`."""
        return self["rotate"]

    @property
    def skew_x(self) -> Parameter:
        """`<layer>.skew_x`."""
        return self["skew_x"]

    @property
    def skew_y(self) -> Parameter:
        """`<layer>.skew_y`."""
        return self["skew_y"]

    @property
    def flip_x(self) -> Parameter:
        """`<layer>.flip_x`."""
        return self["flip_x"]

    @property
    def flip_y(self) -> Parameter:
        """`<layer>.flip_y`."""
        return self["flip_y"]

    @property
    def picture(self) -> Parameter:
        """`<layer>.picture` (a picture layer)."""
        return self["picture"]

    @property
    def text(self) -> Parameter:
        """`<layer>.text` (a text layer)."""
        return self["text"]

    @property
    def color(self) -> Parameter:
        """`<layer>.color` (a text or fill layer)."""
        return self["color"]

    @property
    def drawing(self) -> PartParams:
        """The drawing's parameters: `layer.drawing["ring_on"]` is `<layer>.drawing.ring_on`."""
        return self._part("drawing")

    @property
    def maps(self) -> PartParams:
        """The picture's maps: `layer.maps["matte"]` is `<layer>.map.matte`."""
        return self._part("map")

    def look(self, n: int = 1) -> PartParams:
        """The `n`th look's parameters (from 1): `layer.look()["swell.amount"]` is `<layer>.look.swell.amount`."""
        return self._part(self._numbered("look", n))

    def mask(self, n: int = 1) -> PartParams:
        """The fields of the layer the `n`th `mask` call made: `layer.mask()["dx"]` is `<layer>.mask.dx`."""
        return self._part(self._numbered("mask", n))
