"""The exceptions: one base to catch, each also the builtin it replaces."""

import pytest

import bullterrier_scene
from bullterrier_scene import BullterrierSceneError, FormatError, InvalidError, ReadError


def test_the_base_error_is_an_exception_a_caller_can_catch():
    with pytest.raises(BullterrierSceneError, match="no luck"):
        raise BullterrierSceneError("no luck")


def test_the_errors_are_exported_from_the_package():
    exported = (
        bullterrier_scene.BullterrierSceneError,
        bullterrier_scene.InvalidError,
        bullterrier_scene.FormatError,
        bullterrier_scene.ReadError,
    )
    assert exported == (BullterrierSceneError, InvalidError, FormatError, ReadError)


@pytest.mark.parametrize(
    ("error", "builtin"),
    [(InvalidError, ValueError), (FormatError, ValueError), (ReadError, OSError)],
    ids=["invalid", "format", "read"],
)
def test_each_error_is_a_library_error_and_the_builtin_it_replaces(error, builtin):
    raised = error("at /look: no luck")
    assert isinstance(raised, BullterrierSceneError)
    assert isinstance(raised, builtin)
    assert "no luck" in str(raised)
