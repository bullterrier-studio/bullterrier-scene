"""A misspelt fixed field of a layer's handles is a type error; the right name passes (`Scene.layer`)."""

import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
pytestmark = [pytest.mark.gate, pytest.mark.typecheck]
PROBE = (
    "from bullterrier_scene import Scene\n\n\n"
    "def opacity(scene: Scene) -> str:\n"
    '    return scene.layer("a").{field}.address\n'
)


def mypy(tmp_path: Path, field: str) -> subprocess.CompletedProcess[str]:
    return mypy_source(tmp_path, PROBE.format(field=field))


def mypy_source(tmp_path: Path, source: str) -> subprocess.CompletedProcess[str]:
    probe = tmp_path / "probe.py"
    probe.write_text(source, encoding="utf-8")
    command = [
        sys.executable,
        "-m",
        "mypy",
        "--config-file",
        str(ROOT / "pyproject.toml"),
        "--no-incremental",
        str(probe),
    ]
    return subprocess.run(
        command, cwd=ROOT, env={"MYPYPATH": str(ROOT / "src"), "PATH": ""}, capture_output=True, encoding="utf-8"
    )


def test_a_right_field_name_passes_the_type_check(tmp_path):
    verdict = mypy(tmp_path, "opacity")
    assert verdict.returncode == 0, verdict.stdout


def test_a_misspelt_field_name_fails_the_type_check(tmp_path):
    verdict = mypy(tmp_path, "opacty")
    assert verdict.returncode != 0
    assert '"LayerParams" has no attribute "opacty"' in verdict.stdout, verdict.stdout


WITH_VALUES = (
    "from bullterrier_scene import Parameter, Scene\n\n\n"
    "def call(scene: Scene, by_name: dict[str, float], by_parameter: dict[Parameter, float]) -> None:\n"
    "    {call}\n"
)


@pytest.mark.parametrize(
    "call",
    [
        "scene.with_values(by_name)",
        "scene.with_values(by_parameter)",
        'scene.with_values({"a.opacity": 0.5})',
        'scene.with_values({scene.layer("a").opacity: 0.5})',
    ],
)
def test_with_values_takes_addresses_or_handles_as_keys(tmp_path, call):
    verdict = mypy_source(tmp_path, WITH_VALUES.format(call=call))
    assert verdict.returncode == 0, verdict.stdout


def test_with_values_refuses_an_int_key_at_type_check(tmp_path):
    verdict = mypy_source(tmp_path, WITH_VALUES.format(call="scene.with_values({1: 0.5})"))
    assert verdict.returncode != 0
    assert "with_values" in verdict.stdout, verdict.stdout


def test_with_values_mixed_address_and_handle_keys_are_a_known_type_error(tmp_path):
    # mypy joins `str` and `Parameter` to `object`, which the key bound refuses:
    # a caller annotates `dict[str | Parameter, ...]` or uses one kind of key.
    call = 'scene.with_values({"a.x": 1.0, scene.layer("a").opacity: 0.5})'
    verdict = mypy_source(tmp_path, WITH_VALUES.format(call=call))
    assert verdict.returncode != 0
    assert "with_values" in verdict.stdout, verdict.stdout
