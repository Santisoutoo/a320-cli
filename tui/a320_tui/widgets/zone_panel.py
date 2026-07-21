"""ZonePanel: composes one cockpit zone from its layout data.

Slot resolution, per canonical id:

- id in ``WIRING``  -> a ``KorryButton`` over its wired spec (renders from
  ``SimState``; refreshed by the tick).
- ``BAT_DISPLAY``   -> the live voltmeter pair (wired gauge).
- any other id      -> the local widget for its type (``widget_for``),
  rendered from the controller's ``ControlView`` and refreshed only when
  actuated — the tick never touches local widgets.
- ``mimic:`` / ``prop:`` -> painted scenery, inert.
"""

from __future__ import annotations

from rich.text import Text
from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.widgets import Static

from a320_tui.controller import CockpitController
from a320_tui.layouts import Section, ZoneSpec, resolve_section
from a320_tui.state import SimState
from a320_tui.widgets import widget_for
from a320_tui.widgets.korry_button import KorryButton
from a320_tui.wiring import WIRING

_GREEN = "green3"
_AMBER_7SEG = "bold orange1"
_DARK = "grey35"

_BAT_VARS = ("ELEC_BAT_1_POTENTIAL", "ELEC_BAT_2_POTENTIAL")


class BatteryPairDisplay(Static):
    """Both BAT voltmeters in one live readout (the model's BAT_DISPLAY)."""

    DEFAULT_CSS = """
    BatteryPairDisplay {
        width: 17;
        height: 4;
        border: round $primary-darken-2;
        content-align: center middle;
        text-align: center;
    }
    """

    def __init__(self) -> None:
        super().__init__()
        self.border_title = "BAT 1 · BAT 2"

    def update_state(self, state: SimState) -> None:
        text = Text(justify="center")
        for i, var in enumerate(_BAT_VARS):
            if i:
                text.append("  ")
            volts = state.value(var)
            if volts > 0.0:
                text.append(f"{volts:4.1f}V", _AMBER_7SEG)
            else:
                text.append("--.-", _DARK)
        self.update(text)


class PanelProp(Static):
    """A real-panel position the sim does not model. Inert scenery."""

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
    """The bus diagram painted on the real 35VU, as green text (paint, not
    instruments: the live picture belongs to the ELEC synoptic)."""

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


class SectionBox(Vertical):
    DEFAULT_CSS = """
    SectionBox {
        width: auto;
        height: auto;
        border: round $primary-darken-3;
        border-title-color: $text-muted;
        padding: 0 1;
    }
    SectionBox Horizontal {
        width: auto;
        height: auto;
    }
    """

    def __init__(self, title: str) -> None:
        super().__init__()
        self.border_title = title


class ZonePanel(Vertical):
    DEFAULT_CSS = """
    ZonePanel {
        width: auto;
        height: auto;
    }
    ZonePanel > Horizontal {
        width: auto;
        height: auto;
    }
    ZonePanel .zone-column {
        width: auto;
        height: auto;
    }
    """

    def __init__(self, zone: ZoneSpec, controller: CockpitController) -> None:
        super().__init__()
        self.zone = zone
        self.controller = controller
        self._wired: list[KorryButton] = []
        self._displays: list[BatteryPairDisplay] = []
        self._local: dict[str, object] = {}

    def compose(self) -> ComposeResult:
        model = self.controller.model
        with Horizontal():
            for column in self.zone.columns:
                with Vertical(classes="zone-column"):
                    for section in column:
                        resolved = resolve_section(section, model)
                        yield from self._compose_section(resolved)

    def _compose_section(self, section: Section) -> ComposeResult:
        box = SectionBox(section.title)
        with box:
            for row in section.rows:
                with Horizontal():
                    for slot in row:
                        yield self._make(slot)
        yield box

    def _make(self, slot: str):
        if slot.startswith("mimic:"):
            return BusMimic()
        if slot.startswith("prop:"):
            return PanelProp(slot.partition(":")[2])
        if slot == "BAT_DISPLAY":
            display = BatteryPairDisplay()
            self._displays.append(display)
            return display
        spec = WIRING.get(slot)
        if spec is not None:
            button = KorryButton(spec)
            self._wired.append(button)
            return button
        widget = widget_for(self.controller.view(slot))
        self._local[slot] = widget
        return widget

    def update_state(self, state: SimState) -> None:
        """Tick refresh: wired widgets only; local ones refresh on actuation."""
        for button in self._wired:
            button.update_state(state)
        for display in self._displays:
            display.update_state(state)

    def refresh_local(self, control_id: str) -> bool:
        widget = self._local.get(control_id)
        if widget is None:
            return False
        widget.refresh_view(self.controller.view(control_id))
        return True
