"""The exceptions: one base to catch, each also the builtin it replaces."""

import pytest

import bullterrier_scene
from bullterrier_scene import BullterrierSceneError, InvalidError


def test_the_base_error_is_an_exception_a_caller_can_catch():
    with pytest.raises(BullterrierSceneError, match="no luck"):
        raise BullterrierSceneError("no luck")


def test_the_errors_are_exported_from_the_package():
    exported = (bullterrier_scene.BullterrierSceneError, bullterrier_scene.InvalidError)
    assert exported == (BullterrierSceneError, InvalidError)


def test_an_invalid_error_is_a_library_error_and_a_value_error():
    error = InvalidError("factor: must be finite")
    assert isinstance(error, BullterrierSceneError)
    assert isinstance(error, ValueError)
