"""A320TuiApp: the Textual cockpit over ``a320_sim``.

One core, N frontends: this app renders the same ``Sim`` the REPL and the
(future) MCP server drive. All sim access lives on the main event-loop thread
(``Sim`` is unsendable — see ``sim_bridge.py``); the tick is a Textual
``set_interval`` timer, never a thread worker.
"""

from __future__ import annotations

import a320_sim
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.widgets import Footer, Input, RichLog

from a320_tui.commands import EmbeddedRepl
from a320_tui.manifest import button_specs
from a320_tui.sim_bridge import SimBridge
from a320_tui.state import SimState
from a320_tui.widgets import (
    ElecSynoptic,
    EwdPanel,
    KorryButton,
    OverheadPanel,
    StatusBar,
)

_TICK_PERIOD = 0.2   # ~5 Hz, the settling pattern used across the project
_TICK_DT_MS = 200
_MAX_SPEED = 32


class A320TuiApp(App):
    TITLE = "A320 systems twin"
    CSS_PATH = "styles/app.tcss"
    BINDINGS = [
        Binding("space", "toggle_pause", "Pause/Resume"),
        Binding("+", "faster", "Faster"),
        Binding("-", "slower", "Slower"),
        Binding("ctrl+q", "quit", "Quit"),
    ]

    def __init__(self, sim: "a320_sim.Sim | None" = None) -> None:
        super().__init__()
        # Created here (main thread) and only ever used from the event loop.
        self.bridge = SimBridge(sim)
        self.repl = EmbeddedRepl(self.bridge.sim)
        self.paused = False
        self.speed = 1
        self._last_state: "SimState | None" = None
        specs = button_specs(self.bridge.controls)
        self._cockpit_specs = [s for s in specs if s.style != "world"]
        self._world_specs = [s for s in specs if s.style == "world"]

    def compose(self) -> ComposeResult:
        yield StatusBar()
        with Horizontal(id="main"):
            yield OverheadPanel(self._cockpit_specs, self._world_specs)
            with Vertical(id="right"):
                yield ElecSynoptic()
                yield EwdPanel(self.bridge.failure_catalog)
        with Vertical(id="cmdline"):
            yield RichLog(id="cmdlog", markup=False, wrap=True)
            yield Input(placeholder="command (set bat_1 on · fail elec.gen.1 · help) — Tab to focus panels", id="cmdinput")
        yield Footer()

    def on_mount(self) -> None:
        if self.bridge.missing_vars:
            self._log_lines(
                "warning: vars missing from the registry (vendor drift?): "
                + ", ".join(self.bridge.missing_vars)
            )
        self.set_interval(_TICK_PERIOD, self._tick)
        self._refresh_widgets(force=True)

    # --- tick loop (main thread only) ----------------------------------------
    def _tick(self) -> None:
        if not self.paused:
            self.bridge.step(_TICK_DT_MS * self.speed)
        self._refresh_widgets()

    def _refresh_widgets(self, force: bool = False) -> None:
        state = self.bridge.read_state()
        if not force and state == self._last_state:
            # Nothing moved; StatusBar still tracks t/mode cheaply below.
            self.query_one(StatusBar).update_status(state, self.paused, self.speed)
            return
        self._last_state = state
        self.query_one(StatusBar).update_status(state, self.paused, self.speed)
        self.query_one(OverheadPanel).update_state(state)
        self.query_one(ElecSynoptic).update_state(state)
        self.query_one(EwdPanel).update_state(state)

    # --- input: panel buttons -------------------------------------------------
    def on_korry_button_pressed(self, message: KorryButton.Pressed) -> None:
        try:
            self.bridge.set(message.control_name, message.value)
        except a320_sim.SimError as exc:
            self._log_lines(f"error: {exc}")
            return
        self._log_lines(f"  {message.control_name} <- {message.value:g}")
        self._refresh_widgets(force=True)

    # --- input: embedded command line ----------------------------------------
    def on_input_submitted(self, event: Input.Submitted) -> None:
        line = event.value.strip()
        event.input.value = ""
        if not line:
            return
        self._log_lines(f"a320> {line}")
        output = self.repl.execute(line)
        if output:
            self._log_lines(output)
        if self.repl.quit_requested:
            self.exit()
            return
        self._refresh_widgets(force=True)

    def _log_lines(self, text: str) -> None:
        log = self.query_one("#cmdlog", RichLog)
        for line in text.splitlines():
            log.write(line)

    # --- time controls --------------------------------------------------------
    def action_toggle_pause(self) -> None:
        self.paused = not self.paused
        self._refresh_widgets(force=True)

    def action_faster(self) -> None:
        self.speed = min(self.speed * 2, _MAX_SPEED)
        self._refresh_widgets(force=True)

    def action_slower(self) -> None:
        self.speed = max(self.speed // 2, 1)
        self._refresh_widgets(force=True)


def main() -> int:
    """Console-script / ``python -m a320_tui`` entry point."""
    A320TuiApp().run()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
