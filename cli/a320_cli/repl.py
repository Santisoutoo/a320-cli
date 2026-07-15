"""The A320 REPL: a ``cmd.Cmd`` shell over ``a320_sim.Sim``.

Design notes
------------
- **Stdlib only** (``cmd`` + ``readline``). ``cmd.Cmd`` already gives us the read
  loop, per-command ``help_*``/docstrings, and readline-backed tab-completion via
  ``complete_*`` hooks. On Windows the ``readline`` module is provided by
  ``pyreadline3`` (declared as an environment marker dependency). If neither is
  present the REPL still runs — you just lose tab-completion.
- **No simulation logic here.** Every command is a 1:1 mapping onto the API
  (``set``/``get``/``step``/``run``/``set_environment``/``snapshot``/
  ``list_variables``/``list_controls``). This is the human window onto the same
  core the MCP server drives.
- **Errors never leak a traceback.** Every core call is wrapped; ``SimError``
  (unknown control, bad value) and bad user input print a one-line, actionable
  message and return to the prompt.
"""

from __future__ import annotations

import cmd
import shlex
import sys
import time

# readline powers tab-completion. On Windows it comes from pyreadline3, which
# registers itself under the name ``readline``. Missing readline is not fatal:
# the REPL runs without completion.
try:  # pragma: no cover - platform dependent
    import readline  # noqa: F401

    _HAS_READLINE = True
except ImportError:  # pragma: no cover - platform dependent
    _HAS_READLINE = False

try:
    import a320_sim
except ImportError as exc:  # pragma: no cover - install-time guard
    sys.stderr.write(
        "error: cannot import the 'a320_sim' extension.\n"
        "       Build the bindings first, in a virtualenv:\n"
        "           pip install -e bindings/\n"
        f"       (import error: {exc})\n"
    )
    raise SystemExit(1)


# Values a human is likely to type for a boolean pushbutton. Everything else is
# parsed as a plain float and handed to the core, which validates the range.
_TRUTHY = {"on", "true", "yes", "auto", "1"}
_FALSY = {"off", "false", "no", "0"}

# Sensible defaults for `watch` (issue #12: "advance at ~5 Hz and refresh").
_WATCH_HZ = 5.0
_WATCH_DT_MS = 200


def _parse_value(token: str) -> float:
    """Parse a user value token into the float the core expects.

    Accepts friendly aliases (on/off/true/false/yes/no/auto) plus any numeric
    literal. Raises ``ValueError`` with a helpful message otherwise.
    """
    low = token.lower()
    if low in _TRUTHY:
        return 1.0
    if low in _FALSY:
        return 0.0
    try:
        return float(token)
    except ValueError:
        raise ValueError(
            f"'{token}' is not a value: use a number, or on/off (true/false, 1/0)"
        )


def _fmt_value(value: float) -> str:
    """Render a float compactly: integers without a trailing '.0'."""
    if value == int(value):
        return str(int(value))
    return f"{value:g}"


class A320Repl(cmd.Cmd):
    """Interactive shell driving a single ``a320_sim.Sim`` instance."""

    intro = (
        "A320 systems twin - interactive REPL\n"
        "Cold & dark on the apron. Type 'help' for commands, "
        "'help <command>' for details, 'quit' to exit.\n"
    )

    def __init__(self, sim: "a320_sim.Sim | None" = None, stdout=None) -> None:
        super().__init__(stdout=stdout)
        self.sim = sim if sim is not None else a320_sim.Sim()
        # Whether we can drive the cursor with ANSI escapes for in-place redraw.
        # When stdout is piped (automation, capture), fall back to a plain log.
        self._interactive = bool(getattr(self.stdout, "isatty", lambda: False)())
        # Cache the curated catalog for prompt + completion.
        self._controls = self.sim.list_controls()
        self._control_names = sorted(c["name"] for c in self._controls)
        # Both friendly names and raw LVARs are valid `set` targets.
        self._control_lvars = sorted(c["lvar"] for c in self._controls)
        if not _HAS_READLINE:
            self.stdout.write(
                "note: readline not available; tab-completion is disabled.\n"
            )

    # --- prompt shows sim time so you can see the clock advance --------------
    @property
    def prompt(self) -> str:  # type: ignore[override]
        return f"a320 [t={self.sim.sim_time():6.1f}s]> "

    @prompt.setter
    def prompt(self, _value) -> None:
        # cmd.Cmd assigns to self.prompt in __init__; ignore, we compute it.
        pass

    # --- helpers ------------------------------------------------------------
    def _error(self, msg: str) -> None:
        self.stdout.write(f"error: {msg}\n")

    def _split(self, arg: str) -> list[str]:
        try:
            return shlex.split(arg)
        except ValueError:
            return arg.split()

    def _render_table(self, state: dict) -> list[str]:
        """Format a name->value mapping as aligned lines."""
        if not state:
            return []
        width = max(len(name) for name in state)
        return [f"  {name:<{width}}  {_fmt_value(val)}" for name, val in state.items()]

    # --- commands -----------------------------------------------------------
    def do_set(self, arg: str) -> None:
        """set <control> <value>  -- actuate a control (switch/pushbutton).

        <control> is a friendly name (e.g. bat_1, ext_pwr, bus_tie) or the raw
        LVAR. <value> is a number, or a friendly boolean: on/off, true/false,
        yes/no, auto (auto = on). Tab-completes control names.

        Examples:
          set bat_1 on
          set bus_tie auto
          set ext_pwr_avail 1
        """
        parts = self._split(arg)
        if len(parts) != 2:
            self._error("usage: set <control> <value>   (e.g. set bat_1 on)")
            return
        control, raw_value = parts
        try:
            value = _parse_value(raw_value)
        except ValueError as exc:
            self._error(str(exc))
            return
        try:
            self.sim.set(control, value)
        except a320_sim.SimError as exc:
            self._error(str(exc))
            return
        self.stdout.write(f"  {control} <- {_fmt_value(value)}\n")

    def complete_set(self, text, line, begidx, endidx):
        # Complete only the first argument (the control name).
        parts = line[:begidx].split()
        # parts[0] == 'set'; if we're past the control arg, no completion.
        if len(parts) > 1:
            return []
        names = self._control_names + self._control_lvars
        return [n for n in names if n.startswith(text)]

    def do_get(self, arg: str) -> None:
        """get <var> [<var> ...]  -- read one or more variables.

        Variables are LVAR names (e.g. ELEC_DC_BAT_BUS_IS_POWERED). Use 'vars'
        to discover names, or 'controls' for the actuable ones. Tab-completes.
        """
        names = self._split(arg)
        if not names:
            self._error("usage: get <var> [<var> ...]")
            return
        try:
            state = self.sim.get(names)
        except a320_sim.SimError as exc:
            self._error(str(exc))
            return
        for line in self._render_table(state):
            self.stdout.write(line + "\n")

    def complete_get(self, text, line, begidx, endidx):
        return self._complete_variable(text)

    def _complete_variable(self, text):
        if not text:
            # Avoid dumping the whole registry on an empty tab.
            return self._control_lvars[:]
        return [v for v in self.sim.list_variables() if v.startswith(text)]

    def do_step(self, arg: str) -> None:
        """step [<ms>]  -- advance the sim by one tick of <ms> milliseconds.

        Default 1000 ms. A single tick; for settling use 'run' or 'watch'.
        """
        parts = self._split(arg)
        dt_ms = 1000
        if parts:
            try:
                dt_ms = int(parts[0])
            except ValueError:
                self._error("usage: step [<ms>]   (<ms> must be an integer)")
                return
            if dt_ms <= 0:
                self._error("<ms> must be positive")
                return
        self.sim.step(dt_ms)
        self.stdout.write(f"  stepped {dt_ms} ms  (t={self.sim.sim_time():.3f}s)\n")

    def do_run(self, arg: str) -> None:
        """run <seconds> [<rate_hz>]  -- advance <seconds> at <rate_hz> ticks/s.

        Rate defaults to 5 Hz (the settling pattern used across the core). This
        does not sleep: it advances sim time as fast as it computes.

        Example:  run 2        (2 s of settling at 5 Hz)
        """
        parts = self._split(arg)
        if not parts:
            self._error("usage: run <seconds> [<rate_hz>]   (e.g. run 2)")
            return
        try:
            seconds = float(parts[0])
            rate = float(parts[1]) if len(parts) > 1 else _WATCH_HZ
        except ValueError:
            self._error("usage: run <seconds> [<rate_hz>]   (numbers)")
            return
        if seconds <= 0 or rate <= 0:
            self._error("<seconds> and <rate_hz> must be positive")
            return
        self.sim.run(seconds, rate)
        self.stdout.write(
            f"  ran {seconds:g}s at {rate:g} Hz  (t={self.sim.sim_time():.3f}s)\n"
        )

    def do_env(self, arg: str) -> None:
        """env <alt_ft> <ias_kt> <oat_c> <qnh_hpa>  -- set the outside world.

        Feeds the UpdateContext: altitude (ft), indicated airspeed (kt), outside
        air temperature (C), QNH (hPa). On the ground for the electrical slice a
        typical value is:  env 0 0 15 1013.25
        """
        parts = self._split(arg)
        if len(parts) != 4:
            self._error("usage: env <alt_ft> <ias_kt> <oat_c> <qnh_hpa>")
            return
        try:
            alt, ias, oat, qnh = (float(p) for p in parts)
        except ValueError:
            self._error("all four arguments must be numbers")
            return
        self.sim.set_environment(alt, ias, oat, qnh)
        self.stdout.write(
            f"  env set: alt={alt:g}ft ias={ias:g}kt oat={oat:g}C qnh={qnh:g}hPa\n"
        )

    def do_snapshot(self, arg: str) -> None:
        """snapshot [<substr>]  -- dump the full state (optionally filtered).

        With no argument, prints how many variables exist and a hint (the full
        dump is hundreds of vars). With <substr>, prints every variable whose
        name contains it.

        Example:  snapshot ELEC_AC        (all AC-network variables)
        """
        snap = self.sim.snapshot()
        substr = arg.strip()
        if not substr:
            self.stdout.write(
                f"  {len(snap)} variables in the registry. "
                "Filter with 'snapshot <substr>' or list names with 'vars'.\n"
            )
            return
        filtered = {k: v for k, v in snap.items() if substr in k}
        if not filtered:
            self.stdout.write(f"  no variable name contains '{substr}'\n")
            return
        for line in self._render_table(filtered):
            self.stdout.write(line + "\n")
        self.stdout.write(f"  ({len(filtered)} match)\n")

    def do_controls(self, arg: str) -> None:
        """controls  -- list the curated, actuable controls (from list_controls).

        These are the switches/pushbuttons you can 'set', with their friendly
        name, type, valid values and the underlying LVAR.
        """
        controls = sorted(self._controls, key=lambda c: (c["group"], c["name"]))
        name_w = max(len(c["name"]) for c in controls)
        valid_w = max(len(c["valid_values"]) for c in controls)
        for c in controls:
            dom = "" if c["domain"] == "cockpit" else "  [world]"
            self.stdout.write(
                f"  {c['name']:<{name_w}}  {c['kind']:<5}  "
                f"{c['valid_values']:<{valid_w}}  {c['group']}{dom}\n"
                f"      {c['description']}\n"
                f"      lvar: {c['lvar']}\n"
            )

    def do_vars(self, arg: str) -> None:
        """vars [<substr>]  -- list known variable names (optionally filtered).

        The raw registry is large; pass a substring to narrow it.
        Example:  vars ELEC_DC
        """
        names = self.sim.list_variables()
        substr = arg.strip()
        if substr:
            names = [n for n in names if substr in n]
        if not names:
            self.stdout.write(f"  no variable name contains '{substr}'\n")
            return
        for name in sorted(names):
            self.stdout.write(f"  {name}\n")
        self.stdout.write(f"  ({len(names)} variables)\n")

    def do_watch(self, arg: str) -> None:
        """watch <var> [<var> ...]  -- live view while time advances.

        Advances the sim at ~5 Hz and re-renders the given variables in place,
        so you can watch buses come alive as contactors sequence. Press Ctrl+C
        to stop watching and return to the prompt (the sim keeps its state).

        Example:  watch ELEC_DC_BAT_BUS_IS_POWERED ELEC_AC_1_BUS_IS_POWERED
        """
        names = self._split(arg)
        if not names:
            self._error("usage: watch <var> [<var> ...]")
            return
        # Validate the names up front so a typo fails cleanly, not mid-loop.
        try:
            state = self.sim.get(names)
        except a320_sim.SimError as exc:
            self._error(str(exc))
            return

        period = 1.0 / _WATCH_HZ
        width = max(len(n) for n in names)
        n_lines = len(names) + 1  # +1 for the status line

        self.stdout.write(
            f"watching {len(names)} var(s) at {_WATCH_HZ:g} Hz - Ctrl+C to stop\n"
        )
        drawn = False
        try:
            while True:
                if self._interactive:
                    if drawn:
                        # Move the cursor up over the previously drawn block to
                        # redraw the same lines in place (a live table).
                        self.stdout.write(f"\033[{n_lines}A")
                    for name in names:
                        val = state[name]
                        mark = "*" if val != 0.0 else " "
                        # \033[K clears to end of line (shrinking values).
                        self.stdout.write(
                            f"{mark} {name:<{width}}  {_fmt_value(val)}\033[K\n"
                        )
                    self.stdout.write(f"  t={self.sim.sim_time():8.2f}s\033[K\n")
                else:
                    # Non-TTY (piped/captured): one compact log line per refresh,
                    # no cursor movement, so the transitions read cleanly.
                    cells = " ".join(
                        f"{name}={_fmt_value(state[name])}" for name in names
                    )
                    self.stdout.write(f"  t={self.sim.sim_time():6.2f}s  {cells}\n")
                self.stdout.flush()
                drawn = True

                self.sim.step(_WATCH_DT_MS)
                state = self.sim.get(names)
                time.sleep(period)
        except KeyboardInterrupt:
            self.stdout.write(f"\n  [watch stopped at t={self.sim.sim_time():.2f}s]\n")

    def complete_watch(self, text, line, begidx, endidx):
        return self._complete_variable(text)

    # --- exit ---------------------------------------------------------------
    def do_quit(self, arg: str) -> bool:
        """quit  -- leave the REPL."""
        self.stdout.write("bye\n")
        return True

    def do_exit(self, arg: str) -> bool:
        """exit  -- leave the REPL (alias of quit)."""
        return self.do_quit(arg)

    def do_EOF(self, arg: str) -> bool:
        """Ctrl+D / EOF leaves the REPL."""
        self.stdout.write("\n")
        return self.do_quit(arg)

    # --- polish -------------------------------------------------------------
    def emptyline(self) -> bool:
        # Default cmd behavior repeats the last command; do nothing instead.
        return False

    def default(self, line: str) -> None:
        cmd_name = line.split()[0] if line.split() else line
        self._error(f"unknown command '{cmd_name}'. Type 'help' for the list.")


def main(argv: "list[str] | None" = None) -> int:
    """Console-script / ``python -m a320_cli`` entry point."""
    repl = A320Repl()
    try:
        repl.cmdloop()
    except KeyboardInterrupt:
        # Ctrl+C at the bare prompt: exit cleanly, no traceback.
        repl.stdout.write("\nbye\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
