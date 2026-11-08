"""The command line: what it prints, and the exit codes it promises."""

import subprocess
import sys

import pytest

from bullterrier_scene import __version__
from bullterrier_scene.cli import main


@pytest.mark.parametrize(
    ("argv", "code", "out", "err"),
    [
        pytest.param(["scale", "3", "2"], 0, "6.0\n", "", id="a product"),
        pytest.param(["scale", "nan", "2"], 1, "", "error: value must be a finite number", id="a library refusal"),
        pytest.param(["scale", "1e200", "1e200"], 1, "", "error: the result is too large", id="an overflow"),
        pytest.param([], 0, "usage: bt-scene", "", id="no arguments print the help"),
    ],
)
def test_the_command_prints_its_answer_or_its_refusal(capsys, argv, code, out, err):
    assert main(argv) == code
    captured = capsys.readouterr()
    assert out in captured.out
    assert err in captured.err


def test_a_text_that_is_not_a_number_is_a_usage_error(capsys):
    with pytest.raises(SystemExit) as stopped:
        main(["scale", "three", "2"])
    assert stopped.value.code == 2
    assert "invalid float value" in capsys.readouterr().err


def test_the_version_flag_prints_the_package_version(capsys):
    with pytest.raises(SystemExit) as stopped:
        main(["--version"])
    assert stopped.value.code == 0
    assert capsys.readouterr().out.strip() == f"bt-scene {__version__}"


def test_python_dash_m_runs_the_same_command():
    done = subprocess.run(
        [sys.executable, "-m", "bullterrier_scene", "scale", "1.5", "4"],
        capture_output=True,
        encoding="utf-8",
        check=False,
    )
    assert (done.returncode, done.stdout) == (0, "6.0\n")
