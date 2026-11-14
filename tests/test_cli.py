"""The command line at the process level: it is the Rust crate's (`crates/cli`, tested in-process there), so what is
tested here is what only Python adds: the two entry points, the version, and that the JSON it prints is the package's
own."""

import json
import subprocess
import sys
import tomllib
from pathlib import Path

import pytest

import bullterrier_scene as bts
from bullterrier_scene import __version__
from bullterrier_scene.cli import run

ROOT = Path(__file__).resolve().parents[1]
COVER = ROOT / "docs" / "examples" / "cover.scene.json"  # a saved scene, with its photo beside it
CONSOLE_SCRIPT = Path(sys.executable).with_name("bt-scene")  # `uv run bt-scene`, as `uv sync` installs it


def python_dash_m(*args: str) -> subprocess.CompletedProcess[bytes]:
    """`python -m bullterrier_scene ARGS` in a fresh interpreter, the way a user starts it."""
    return subprocess.run(
        [sys.executable, "-m", "bullterrier_scene", *args], capture_output=True, check=False, timeout=120
    )


def console_script(*args: str) -> subprocess.CompletedProcess[bytes]:
    """`bt-scene ARGS`, the program the package installs."""
    return subprocess.run([str(CONSOLE_SCRIPT), *args], capture_output=True, check=False, timeout=120)


def test_the_package_imports_where_no_distribution_is_installed():
    """A source checkout on `PYTHONPATH` has no package metadata: it imports, and only asking the version fails."""
    code = (
        "import importlib.metadata as m\n"
        "def gone(name):\n"
        "    raise m.PackageNotFoundError(name)\n"
        "m.version = gone\n"
        "import bullterrier_scene as bts\n"
        "print(bts.Scene(2, 2).render(2).shape)\n"
        "try:\n"
        "    bts.__version__\n"
        "except m.PackageNotFoundError:\n"
        "    print('no version')\n"
    )
    done = subprocess.run([sys.executable, "-c", code], capture_output=True, check=False, timeout=120)
    assert (done.returncode, done.stdout.decode().split("\n")[:2]) == (0, ["(2, 2, 4)", "no version"]), (
        done.stderr.decode()
    )


def test_version_is_the_one_in_pyproject():
    declared = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))["project"]["version"]
    done = python_dash_m("--version")
    assert (done.returncode, done.stdout.decode().strip()) == (0, f"bt-scene {declared}")
    assert __version__ == declared


def test_the_help_lists_the_commands_and_the_exit_codes():
    done = python_dash_m("--help")
    assert done.returncode == 0
    for word in ("render", "parameters", "validate", "exit codes:", "130"):
        assert word in done.stdout.decode(), word


@pytest.mark.parametrize("start", [python_dash_m, console_script], ids=["python -m", "console script"])
def test_a_user_mistake_is_one_error_line_with_exit_1_and_no_traceback(start, tmp_path):
    done = start("render", str(tmp_path / "gone.json"), str(tmp_path / "out.png"))
    assert done.returncode == 1
    assert done.stderr.decode().splitlines()[-1].startswith("error: ")
    assert "Traceback" not in done.stderr.decode()


@pytest.mark.parametrize("start", [python_dash_m, console_script], ids=["python -m", "console script"])
def test_a_wrong_command_line_is_usage_error_2(start):
    done = start("frobnicate")
    assert done.returncode == 2
    assert "Traceback" not in done.stderr.decode()


def test_run_returns_the_exit_code_and_writes_to_the_process_streams(capfd):
    assert run(["--version"]) == 0
    assert capfd.readouterr().out.strip() == f"bt-scene {__version__}"


def test_the_console_script_and_python_dash_m_write_the_same_picture(tmp_path):
    outs = [tmp_path / "script.png", tmp_path / "module.png"]
    shown = [
        start("render", str(COVER), str(out), "--size", "160", "--set", "title.text=Hello")
        for start, out in zip([console_script, python_dash_m], outs, strict=True)
    ]
    assert [done.returncode for done in shown] == [0, 0], [done.stderr for done in shown]
    assert outs[0].read_bytes() == outs[1].read_bytes()
    assert outs[0].read_bytes().startswith(b"\x89PNG\r\n\x1a\n")


def test_the_parameters_it_prints_as_json_are_the_ones_the_package_describes():
    scene = bts.Scene.from_json(COVER.read_text(encoding="utf-8"), base=COVER.parent)
    done = python_dash_m("parameters", str(COVER), "--json")
    assert done.returncode == 0, done.stderr
    assert json.loads(done.stdout) == [parameter.to_dict() for parameter in scene.parameters().values()]


def test_the_table_lists_every_address_of_the_scene():
    scene = bts.Scene.from_json(COVER.read_text(encoding="utf-8"), base=COVER.parent)
    done = python_dash_m("parameters", str(COVER))
    assert done.returncode == 0, done.stderr
    assert [line.split()[0] for line in done.stdout.decode().splitlines()[1:]] == list(scene.parameters())
