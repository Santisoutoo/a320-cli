"""EmbeddedRepl: the REPL grammar, re-hosted inside the TUI's command line.

One grammar to maintain: instead of writing a parser, subclass ``A320Repl``
(the ``cmd.Cmd`` shell from the ``a320-cli`` package), point its stdout at a
buffer, and feed it lines via ``onecmd``. All commands (set/get/step/env/
snapshot/controls/vars...) behave exactly as in the terminal REPL, including
its one-line, traceback-free error handling.

Overrides:
- ``watch`` would loop forever and freeze the event loop — the TUI *is* a
  watch, so it explains that instead.
- ``run`` is capped: a ``run 600`` would block the UI for its whole duration.
- ``quit``/``exit`` request an app shutdown instead of ending a cmdloop that
  is not running.

``fail``/``unfail``/``failures``/``ecam`` need no overrides: Phase 2 added them
to the REPL itself, and the embedded grammar inherits them (the duplicates this
module carried while feat/14 was in flight are gone).
"""

from __future__ import annotations

import io

import a320_sim
from a320_cli.repl import A320Repl

_MAX_RUN_SECONDS = 30.0


class EmbeddedRepl(A320Repl):
    def __init__(self, sim: "a320_sim.Sim") -> None:
        self._buffer = io.StringIO()
        super().__init__(sim=sim, stdout=self._buffer)
        self.quit_requested = False

    def execute(self, line: str) -> str:
        """Run one command line; return whatever it printed."""
        self._buffer.seek(0)
        self._buffer.truncate()
        try:
            self.onecmd(line)
        except a320_sim.SimError as exc:
            # A320Repl catches SimError per command; this is a net for
            # commands added here.
            self._error(str(exc))
        return self._buffer.getvalue().rstrip("\n")

    # --- commands that clash with a live UI ----------------------------------
    def do_watch(self, arg: str) -> None:
        self.stdout.write(
            "the TUI is already a live watch: see the synoptic. "
            "('watch' belongs to the terminal REPL: a320-cli --repl)\n"
        )

    def do_run(self, arg: str) -> None:
        parts = self._split(arg)
        if parts:
            try:
                seconds = float(parts[0])
            except ValueError:
                seconds = 0.0
            if seconds > _MAX_RUN_SECONDS:
                self._error(
                    f"run is capped at {_MAX_RUN_SECONDS:g}s inside the TUI "
                    "(it blocks the UI while it computes); use the speed "
                    "controls (+/-) for long settles"
                )
                return
        super().do_run(arg)

    def do_quit(self, arg: str) -> bool:
        self.quit_requested = True
        self.stdout.write("bye\n")
        return True
