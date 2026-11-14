"""`Parameter`: one value of a scene that a caller may set, as `Scene.parameters()` describes it."""

from dataclasses import dataclass
from typing import Literal

__all__ = ["Parameter"]

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
