"""a320_cli: a human REPL over the headless A320 systems core.

One of the two frontends onto the same control/observe API (the other is the
MCP server). This package is a thin terminal shell over the ``a320_sim``
extension: it does not contain any simulation logic — it maps typed commands to
``a320_sim.Sim`` calls and renders the results legibly.

Entry points:
  - ``python -m a320_cli``   (run from the ``cli/`` directory, or after install)
  - ``a320-cli``             (console script, after ``pip install -e cli/``)
"""

from a320_cli.repl import A320Repl, main

__all__ = ["A320Repl", "main"]
__version__ = "0.1.0"
