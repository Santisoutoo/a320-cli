"""KorryButton: an Airbus overhead pushbutton rendered in ~12x4 cells.

Two light lines inside a bordered box, legend as the border title:

    ┌ BAT 1 ─────┐        ┌ EXT PWR ───┐
    │   FAULT    │  amber │   AVAIL    │  green
    │    OFF     │  white │     ON     │  blue
    └────────────┘        └────────────┘

An unlit light renders as dim dots, like a dark annunciator. The widget is
pure display + input: it renders from ``SimState`` and, when pressed (click
or Enter), posts a ``KorryButton.Pressed`` message with the toggled value —
it never touches the sim.
"""

from __future__ import annotations

from rich.text import Text
from textual.message import Message
from textual.widgets import Static

from a320_tui.manifest import ButtonSpec
from a320_tui.state import SimState

_AMBER = "orange3"
_WHITE = "bold white"
_GREEN = "green3"
_BLUE = "bold deep_sky_blue1"
_DARK = "grey35"
_UNLIT = "· · ·"


class KorryButton(Static, can_focus=True):
    DEFAULT_CSS = """
    KorryButton {
        width: 14;
        height: 4;
        border: round $primary-darken-2;
        content-align: center middle;
        text-align: center;
    }
    KorryButton:focus {
        border: round $accent;
    }
    KorryButton.world {
        border: round $warning-darken-1;
    }
    """

    class Pressed(Message):
        # ``control`` is a reserved property on textual's Message; use our own.
        def __init__(self, control_name: str, value: float) -> None:
            self.control_name = control_name
            self.value = value
            super().__init__()

    def __init__(self, spec: ButtonSpec) -> None:
        super().__init__()
        self.spec = spec
        self.border_title = spec.legend
        self._pb_value = 0.0
        # Fit the border title: "COMMERCIAL" or "AC ESS FEED" would otherwise
        # truncate to "COMMERC…" inside the default 14 cells.
        self.styles.width = max(12, len(spec.legend) + 6)
        if spec.style == "world":
            self.add_class("world")

    def update_state(self, state: SimState) -> None:
        self._pb_value = state.value(self.spec.state_var)
        self.update(self._render_lights(state))

    def _render_lights(self, state: SimState) -> Text:
        spec = self.spec
        pb_on = state.is_on(spec.state_var)
        fault = spec.fault_var is not None and state.is_on(spec.fault_var)
        avail = spec.avail_var is not None and state.is_on(spec.avail_var)

        if spec.style == "world":
            # Scenario controls (GPU plugged...) — not cockpit hardware.
            top = ("WORLD", _DARK)
            bottom = ("SET", _GREEN) if pb_on else (_UNLIT, _DARK)
        elif spec.style == "on_avail":
            top = ("AVAIL", _GREEN) if (avail and not pb_on) else (_UNLIT, _DARK)
            bottom = ("ON", _BLUE) if pb_on else (_UNLIT, _DARK)
        elif spec.style == "fault_on":
            # APU MASTER SW style: blue ON when engaged, dark when released.
            top = ("FAULT", _AMBER) if fault else (_UNLIT, _DARK)
            bottom = ("ON", _BLUE) if pb_on else (_UNLIT, _DARK)
        elif spec.style == "normal_altn":
            # AC ESS FEED: released = NORMAL (dark), pressed = ALTN (white).
            top = ("FAULT", _AMBER) if fault else (_UNLIT, _DARK)
            bottom = (_UNLIT, _DARK) if pb_on else ("ALTN", _WHITE)
        else:
            # auto_off (BAT, BUS TIE) and on_off (GEN, APU GEN): lit white OFF
            # when released, dark when engaged; amber FAULT on top.
            top = ("FAULT", _AMBER) if fault else (_UNLIT, _DARK)
            bottom = (_UNLIT, _DARK) if pb_on else ("OFF", _WHITE)

        text = Text(justify="center")
        text.append(top[0], top[1])
        text.append("\n")
        text.append(bottom[0], bottom[1])
        return text

    def _press(self) -> None:
        toggled = 0.0 if self._pb_value != 0.0 else 1.0
        self.post_message(self.Pressed(self.spec.control, toggled))

    def on_click(self) -> None:
        self._press()

    def key_enter(self) -> None:
        self._press()
