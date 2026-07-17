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
- ``fail``/``unfail``/``failures`` are provided here while feat/14 (which adds
  them to the REPL itself) is unmerged; once it lands these become overrides
  of the same commands and can be dropped.
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

    # --- failures (mirrors feat/14; drop when it merges) ----------------------
    def _require_failures(self) -> bool:
        if not hasattr(self.sim, "inject_failure"):
            self._error("this a320_sim build has no failure support")
            return False
        return True

    def do_fail(self, arg: str) -> None:
        """fail <id>  -- inject a failure by its stable id (see 'failures')."""
        parts = self._split(arg)
        if len(parts) != 1:
            self._error("usage: fail <id>   (e.g. fail elec.tr.1)")
            return
        if not self._require_failures():
            return
        try:
            self.sim.inject_failure(parts[0])
        except a320_sim.SimError as exc:
            self._error(str(exc))
            return
        self.stdout.write(f"  injected {parts[0]}\n")

    def do_unfail(self, arg: str) -> None:
        """unfail <id>  -- clear an injected failure by its id."""
        parts = self._split(arg)
        if len(parts) != 1:
            self._error("usage: unfail <id>   (e.g. unfail elec.tr.1)")
            return
        if not self._require_failures():
            return
        try:
            self.sim.clear_failure(parts[0])
        except a320_sim.SimError as exc:
            self._error(str(exc))
            return
        self.stdout.write(f"  cleared {parts[0]}\n")

    def do_failures(self, arg: str) -> None:
        """failures  -- list injectable failures, marking the active ones."""
        if not self._require_failures():
            return
        active = set(self.sim.active_failures())
        catalog = sorted(self.sim.list_failures(), key=lambda f: f["id"])
        id_w = max(len(f["id"]) for f in catalog)
        for f in catalog:
            mark = "*" if f["id"] in active else " "
            self.stdout.write(f"{mark} {f['id']:<{id_w}}  {f['description']}\n")
