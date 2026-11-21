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
    probe = tmp_path / "probe.py"
    probe.write_text(PROBE.format(field=field), encoding="utf-8")
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
