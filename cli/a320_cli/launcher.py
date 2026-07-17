"""`a320-cli` entry point: launch the TUI when installed, else the REPL.

The TUI (package ``a320-tui``, Phase T) is the default human frontend when
present; ``--repl`` forces the classic terminal REPL. This module stays
stdlib-only (D-011): the TUI import is optional and degrades silently to the
REPL when the package is missing.
"""

from __future__ import annotations

import sys


def main(argv: "list[str] | None" = None) -> int:
    argv = list(sys.argv[1:]) if argv is None else list(argv)
    if "--repl" not in argv:
        try:
            from a320_tui.app import main as tui_main
        except ImportError:
            pass
        else:
            return tui_main()
    from a320_cli.repl import main as repl_main

    return repl_main()


if __name__ == "__main__":
    raise SystemExit(main())
