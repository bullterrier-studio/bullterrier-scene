"""The command line: what it prints, and the exit codes it promises."""

import subprocess
import sys

import pytest

from bullterrier_scene import __version__
from bullterrier_scene.cli import main


def test_without_arguments_the_command_prints_its_help(capsys):
    assert main([]) == 0
    assert "usage: bt-scene" in capsys.readouterr().out


def test_an_unknown_option_is_a_usage_error(capsys):
    with pytest.raises(SystemExit) as stopped:
        main(["--frobnicate"])
    assert stopped.value.code == 2
    assert "unrecognized arguments" in capsys.readouterr().err


def test_the_version_flag_prints_the_package_version(capsys):
    with pytest.raises(SystemExit) as stopped:
        main(["--version"])
    assert stopped.value.code == 0
    assert capsys.readouterr().out.strip() == f"bt-scene {__version__}"


def test_python_dash_m_runs_the_same_command():
    done = subprocess.run(
        [sys.executable, "-m", "bullterrier_scene", "--version"],
        capture_output=True,
        encoding="utf-8",
        check=False,
    )
    assert (done.returncode, done.stdout.strip()) == (0, f"bt-scene {__version__}")
