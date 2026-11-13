"""Command line: `bt-scene --version`. Commands arrive with batch P1.

A thin layer over the library: it parses the arguments, calls it and prints the answer. A failure the user can fix
prints `error: <what is wrong>` and makes the exit code 1.
"""

import argparse

from . import __version__

_EXIT_CODES = """\
exit codes:
  0    success
  2    wrong usage: an unknown command or option (argparse explains)

Errors go to stderr, results to stdout.
"""


def main(argv: list[str] | None = None) -> int:
    """Run the command line with `argv` (default: `sys.argv[1:]`) and return the exit code.

    Results go to stdout, errors to stderr. Without arguments it prints the help. See the exit codes in `--help`.
    """
    parser = argparse.ArgumentParser(
        prog="bt-scene",
        description="Puts layers into one picture. The commands arrive with the command-line batch (P1).",
        epilog=_EXIT_CODES,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--version", action="version", version=f"bt-scene {__version__}")
    parser.parse_args(argv)
    parser.print_help()
    return 0
