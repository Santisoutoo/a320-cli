"""OverheadPanel: the 35VU ELEC panel, laid out like the real aircraft.

Geometry transcribed from the A32NX overhead reference image (FBW docs,
``ELEC-Panel.jpg`` — used as a reference while building, not committed: the
repo is GPLv3 and a photo would not be manipulable in a terminal anyway):

    COMMERCIAL   [27.7V] BAT1 BAT2 [27.9V]  AC ESS FEED
    GALY & CAB   DC BUS 1 ^  AC ESS BUS  ^ DC BUS 2      <- painted mimic
    IDG1  GEN1  APU GEN  BUS TIE  EXT PWR  GEN2  IDG2

Three slot kinds beyond the curated catalog:

- ``EXTRA_PANEL_SPECS`` — hardware FBW models but the catalog does not expose
  yet (AC ESS FEED, COMMERCIAL, GALY & CAB). They actuate through the raw-LVAR
  path of ``sim.set`` (D-008/D-009), same as typing ``set OVHD_...`` in the REPL.
- ``BatteryDisplay`` — the live voltmeters between the BAT pushbuttons.
- ``PanelProp`` — real-panel positions FBW does not model (IDG 1/2). Inert and
  visibly so; pretending function the sim lacks would be worse than a gap.

Catalog controls the fixed geometry does not place land in an OTHER section,
so a Phase-4 control still appears without touching the TUI (the data-driven
promise of the manifest).
"""

from __future__ import annotations

from rich.text import Text
from textual.app import ComposeResult
from textual.containers import Grid, Horizontal, Vertical
from textual.widgets import Label, Static

from a320_tui.manifest import (
    BAT_DISPLAY_VARS,
    EXTRA_PANEL_SPECS,
    PANEL_LEFT_STACK,
    PANEL_SOURCES_ROW,
    PANEL_TOP_ROW,
    PLACED_CONTROLS,
    ButtonSpec,
)
from a320_tui.state import SimState
from a320_tui.widgets.korry_button import KorryButton

_GREEN = "green3"
_AMBER_7SEG = "bold orange1"
_DARK = "grey35"


class BatteryDisplay(Static):
    """The BAT voltmeter: a live 7-segment-style readout, e.g. ``27.7V``."""

    DEFAULT_CSS = """
    BatteryDisplay {
        width: 9;
        height: 4;
        border: round $primary-darken-2;
        content-align: center middle;
        text-align: center;
    }
    """

    def __init__(self, number: int) -> None:
        super().__init__()
        self.border_title = f"BAT {number}"
        self.styles.width = 11
        self._var = BAT_DISPLAY_VARS[number - 1]

    def update_state(self, state: SimState) -> None:
        volts = state.value(self._var)
        text = Text(justify="center")
        if volts > 0.0:
            text.append(f"{volts:4.1f}V", _AMBER_7SEG)
        else:
            text.append("--.-", _DARK)
        self.update(text)


class PanelProp(Static):
    """A real-panel position the sim does not model (IDG 1/2). Inert.

    Rendered guarded-red like the aircraft, but dark and unfocusable: it is
    scenery, and it should read as scenery.
    """

    DEFAULT_CSS = """
    PanelProp {
        width: 12;
        height: 4;
        border: round red;
        color: $text-muted;
        content-align: center middle;
        text-align: center;
    }
    """

    def __init__(self, legend: str) -> None:
        super().__init__()
        self.border_title = legend
        self.styles.width = max(12, len(legend) + 6)
        self.update(Text("[guard]\nnot modeled", style=_DARK, justify="center"))


class BusMimic(Static):
    """The bus diagram painted on the real panel, as green text.

    Paint, not instruments: the real 35VU lines do not light up with bus
    state, and the live picture already belongs to the ELEC synoptic. Keeping
    this static keeps the two panels' roles clean.
    """

    DEFAULT_CSS = """
    BusMimic {
        height: 2;
        color: green;
        content-align: center middle;
        text-align: center;
    }
    """

    def on_mount(self) -> None:
        text = Text(justify="center")
        text.append(
            "DC BUS 1 ↑                                    ↑ DC BUS 2\n", _GREEN
        )
        text.append(
            "AC BUS 1 ───────────→ AC ESS BUS ←─────────── AC BUS 2", _GREEN
        )
        self.update(text)


class OverheadPanel(Vertical):
    DEFAULT_CSS = """
    OverheadPanel {
        width: 92;
        border: round $primary-darken-2;
        padding: 0 1;
    }
    OverheadPanel .row {
        height: auto;
    }
    OverheadPanel .left-stack {
        width: auto;
        height: auto;
    }
    OverheadPanel Grid {
        grid-size: 4;
        grid-rows: 4;
        grid-columns: 15;
        height: auto;
    }
    OverheadPanel .section {
        color: $text-muted;
        margin: 1 0 0 0;
    }
    """

    def __init__(self, specs: list[ButtonSpec], world_specs: list[ButtonSpec]) -> None:
        super().__init__()
        self.border_title = "OVERHEAD · ELEC (35VU)"
        self._by_name = {s.control: s for s in specs}
        self._other = [s for s in specs if s.control not in PLACED_CONTROLS]
        self._world_specs = world_specs
        self._buttons: list[KorryButton] = []
        self._displays: list[BatteryDisplay] = []

    # --- slot -> widget -------------------------------------------------------
    def _make(self, slot: str):
        kind, _, name = slot.partition(":")
        if kind == "catalog":
            # A curated control can vanish from the catalog (vendor drift);
            # render the hole as a prop rather than crash the cockpit.
            spec = self._by_name.get(name)
            if spec is None:
                return PanelProp(name.upper())
            return self._track(KorryButton(spec))
        if kind == "extra":
            return self._track(KorryButton(EXTRA_PANEL_SPECS[name]))
        if kind == "bat_display":
            display = BatteryDisplay(int(name))
            self._displays.append(display)
            return display
        if kind == "prop":
            return PanelProp(name)
        raise ValueError(f"unknown panel slot kind: {slot!r}")

    def _track(self, button: KorryButton) -> KorryButton:
        self._buttons.append(button)
        return button

    def compose(self) -> ComposeResult:
        with Horizontal(classes="row"):
            with Vertical(classes="left-stack"):
                for slot in PANEL_LEFT_STACK:
                    yield self._make(slot)
            with Vertical(classes="row"):
                with Horizontal(classes="row top-row"):
                    for slot in PANEL_TOP_ROW:
                        yield self._make(slot)
                yield BusMimic()
        with Horizontal(classes="row sources-row"):
            for slot in PANEL_SOURCES_ROW:
                yield self._make(slot)
        if self._other:
            yield Label("— OTHER (not on the 35VU yet) —", classes="section")
            with Grid():
                for spec in self._other:
                    yield self._track(KorryButton(spec))
        if self._world_specs:
            yield Label("— WORLD (scenario) —", classes="section")
            with Grid():
                for spec in self._world_specs:
                    yield self._track(KorryButton(spec))

    def update_state(self, state: SimState) -> None:
        for button in self._buttons:
            button.update_state(state)
        for display in self._displays:
            display.update_state(state)
