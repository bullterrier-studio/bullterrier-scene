"""Command line: `bt-scene scale VALUE FACTOR`.

A thin layer over the library: it parses the arguments, calls it and prints the answer. A failure the user can fix
prints `error: <what is wrong>` and makes the exit code 1.
"""

import argparse
import sys

from . import __version__
from ._native import _core
from .errors import BullterrierSceneError

_EXIT_CODES = """\
exit codes:
  0    success
  1    the library refused the numbers ("error:" on stderr)
  2    wrong usage: an unknown command or option, or a text that is not a number (argparse explains)

Errors go to stderr, results to stdout.
"""


def main(argv: list[str] | None = None) -> int:
    """Run the command line with `argv` (default: `sys.argv[1:]`) and return the exit code.

    Results go to stdout, errors to stderr. Without arguments it prints the help. See the exit codes in `--help`.
    """
    parser = argparse.ArgumentParser(
        prog="bt-scene",
        description="A placeholder command line over the Rust core.",
        epilog=_EXIT_CODES,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--version", action="version", version=f"bt-scene {__version__}")
    commands = parser.add_subparsers(dest="command", metavar="COMMAND")
    scaling = commands.add_parser("scale", help="multiply a value by a factor")
    scaling.add_argument("value", type=float, help="the number to scale")
    scaling.add_argument("factor", type=float, help="what to multiply it by")
    args = parser.parse_args(argv)
    if args.command is None:
        parser.print_help()
        return 0
    try:
        print(_core.scale(args.value, args.factor))
    except BullterrierSceneError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0
