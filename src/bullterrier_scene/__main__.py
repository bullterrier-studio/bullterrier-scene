"""`python -m bullterrier_scene`: the same command line as the `bt-scene` program."""

import sys

from .cli import main

sys.exit(main())
