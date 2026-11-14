"""Command line: `bt-scene render|parameters|validate`.

The command line is the Rust core's (`crates/cli`); this is the entry point that starts it.
"""

import signal
import sys

import bullterrier_scene

from ._native import _core


def run(argv: list[str]) -> int:
    """Run `bt-scene` with `argv` (the arguments after the program's name) and return its exit code: 0 success, 1 the
    command failed (`error: <what is wrong>` on stderr), 2 wrong usage. Results go to stdout and errors to stderr, as
    the process's own."""
    return _core.main(argv, bullterrier_scene.__version__)


def main() -> None:
    """The console script `bt-scene`: run the command line with `sys.argv` and exit with its code. Ctrl-C ends it at
    once (the shell reports 130): Python's handler would wait for the Rust code, which does not look for it."""
    signal.signal(signal.SIGINT, signal.SIG_DFL)
    sys.exit(run(sys.argv[1:]))
