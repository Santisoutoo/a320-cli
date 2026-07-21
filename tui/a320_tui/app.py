"""A320TuiApp: the Textual cockpit over ``a320_sim``.

One core, N frontends: this app renders the same ``Sim`` the REPL and the
MCP server drive. All sim access lives on the main event-loop thread
(``Sim`` is unsendable — see ``sim_bridge.py``); the tick is a Textual
``set_interval`` timer, never a thread worker.

Screen: a 2x2 grid of independently scrollable quadrants — OVERHEAD (NW),
GLARESHIELD over MAIN PANEL (NE), PEDESTAL (SW), and the observation pane
(SE: ELEC synoptic + E/WD + scenario/world controls). Status bar and the
embedded command line stay docked and shared. Wired controls actuate the
sim; every other control of the vendored model is interactable on local
state (the tick never refreshes those — only actuation does).
"""

from __future__ import annotations

import a320_sim
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Grid, Vertical
from textual.widgets import Footer, Input, Label, RichLog

from a320_tui.cockpit_state import CockpitRegistry
from a320_tui.commands import EmbeddedRepl
from a320_tui.controller import CockpitController
from a320_tui.layouts.overhead import OVERHEAD_ZONE
from a320_tui.model import load_model
from a320_tui.sim_bridge import SimBridge
from a320_tui.state import SimState
from a320_tui.widgets import ElecSynoptic, EwdPanel, KorryButton, StatusBar
from a320_tui.widgets.base import CockpitControlWidget
from a320_tui.widgets.quadrant import Quadrant
from a320_tui.widgets.zone_panel import ZonePanel
from a320_tui.wiring import WIRING, WORLD_SPECS

_TICK_PERIOD = 0.2   # ~5 Hz, the settling pattern used across the project
_TICK_DT_MS = 200
_MAX_SPEED = 32


class A320TuiApp(App):
    TITLE = "A320 systems twin"
    CSS_PATH = "styles/app.tcss"
    BINDINGS = [
        Binding("f1", "focus_quadrant('q-overhead')", "OVHD"),
        Binding("f2", "focus_quadrant('q-glare-main')", "GLARE·MAIN"),
        Binding("f3", "focus_quadrant('q-pedestal')", "PED"),
        Binding("f4", "focus_quadrant('q-observe')", "SD·E/WD"),
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
        model = load_model()
        self.controller = CockpitController(
            model, CockpitRegistry(model), wired_ids=frozenset(WIRING)
        )
        self._world_buttons: list[KorryButton] = []

    def compose(self) -> ComposeResult:
        yield StatusBar()
        with Grid(id="cockpit"):
            with Quadrant("OVERHEAD [F1]", id="q-overhead"):
                yield ZonePanel(OVERHEAD_ZONE, self.controller)
            with Quadrant("GLARESHIELD · MAIN PANEL [F2]", id="q-glare-main"):
                yield Label("glareshield + main panel — pending", classes="pending")
            with Quadrant("PEDESTAL [F3]", id="q-pedestal"):
                yield Label("pedestal — pending", classes="pending")
            with Quadrant("ELEC SD · E/WD [F4]", id="q-observe"):
                yield ElecSynoptic()
                yield EwdPanel(self.bridge.failure_catalog)
                yield Label("— SCENARIO (world) —", classes="section")
                for spec in WORLD_SPECS.values():
                    button = KorryButton(spec)
                    self._world_buttons.append(button)
                    yield button
        with Vertical(id="cmdline"):
            yield RichLog(id="cmdlog", markup=False, wrap=True)
            yield Input(
                placeholder=(
                    "command (set bat_1 on · fail elec.gen.1 · help)"
                    " — F1-F4 focus panels"
                ),
                id="cmdinput",
            )
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
        self.query_one(ElecSynoptic).update_state(state)
        self.query_one(EwdPanel).update_state(state)
        for zone in self.query(ZonePanel):
            zone.update_state(state)
        for button in self._world_buttons:
            button.update_state(state)

    # --- input: wired panel buttons -------------------------------------------
    def on_korry_button_pressed(self, message: KorryButton.Pressed) -> None:
        try:
            self.bridge.set(message.control_name, message.value)
        except a320_sim.SimError as exc:
            self._log_lines(f"error: {exc}")
            return
        self._log_lines(f"  {message.control_name} <- {message.value:g}")
        self._refresh_widgets(force=True)

    # --- input: local (unwired) cockpit controls ------------------------------
    def on_cockpit_control_widget_actuated(
        self, message: CockpitControlWidget.Actuated
    ) -> None:
        result = self.controller.actuate(
            message.control_id, message.action, message.payload
        )
        payload = f" {message.payload}" if message.payload is not None else ""
        self._log_lines(
            f"  {message.control_id}{payload}: {result.message} [local]"
        )
        for zone in self.query(ZonePanel):
            if zone.refresh_local(message.control_id):
                break

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

    # --- navigation -----------------------------------------------------------
    def action_focus_quadrant(self, quadrant_id: str) -> None:
        self.query_one(f"#{quadrant_id}", Quadrant).focus()

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
