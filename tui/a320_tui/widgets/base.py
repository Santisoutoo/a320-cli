"""Base for every local cockpit control widget.

Wired controls keep their ``KorryButton`` (rendering from ``SimState``);
everything else renders from a frozen ``ControlView`` and posts one common
``Actuated(control_id, action, payload)`` message that the app routes to
the ``CockpitController``. The widget never mutates state itself: it emits
the intent, the controller applies it, and the app hands back the fresh
view via ``refresh_view``.

Keyboard contract (arrow keys stay reserved for moving focus):
``Enter``/click = primary action (press / cycle / push), ``[`` / ``]`` =
turn or move one notch, ``p`` = pull (FCU knobs), ``Esc`` = close a guard.
"""

from __future__ import annotations

from typing import Any

from rich.text import Text
from textual.message import Message
from textual.widgets import Static

from a320_tui.controller import ControlView

AMBER = "orange3"
WHITE = "bold white"
GREEN = "green3"
BLUE = "bold deep_sky_blue1"
RED = "bold red3"
DARK = "grey35"
UNLIT = "· · ·"


class CockpitControlWidget(Static, can_focus=True):
    DEFAULT_CSS = """
    CockpitControlWidget {
        width: auto;
        height: 4;
        border: round $primary-darken-2;
        content-align: center middle;
        text-align: center;
    }
    CockpitControlWidget:focus {
        border: round $accent;
    }
    """

    class Actuated(Message):
        # ``control`` is a reserved property on textual's Message; avoid it.
        def __init__(
            self, control_id: str, action: str, payload: Any = None
        ) -> None:
            self.control_id = control_id
            self.action = action
            self.payload = payload
            super().__init__()

    def __init__(self, view: ControlView) -> None:
        super().__init__()
        self._view = view
        self.border_title = view.legend
        self.styles.width = max(12, len(view.legend) + 6)

    def on_mount(self) -> None:
        self.refresh_view(self._view)

    def refresh_view(self, view: ControlView) -> None:
        self._view = view
        self.update(self.render_view(view))

    def render_view(self, view: ControlView) -> Text:
        raise NotImplementedError

    def _act(self, action: str, payload: Any = None) -> None:
        self.post_message(self.Actuated(self._view.id, action, payload))

    def on_click(self) -> None:
        self.key_enter()

    def key_enter(self) -> None:
        self._act("press")


def render_local_korry(view: ControlView) -> Text:
    """Korry lights of an unwired pushbutton, Airbus lights-out.

    FAULT can never light locally (no system behind it). When pressed, the
    button shows the engaged light its model declares: blue ON if it has
    one, else the white OFF (pressed = out of auto), else a plain marker.
    """
    top = ("FAULT", DARK) if "FAULT" in view.lights else (UNLIT, DARK)
    if not view.pressed:
        bottom = (UNLIT, DARK)
    elif "ON" in view.lights:
        bottom = ("ON", BLUE)
    elif "OFF" in view.lights:
        bottom = ("OFF", WHITE)
    else:
        bottom = ("▪ IN ▪", WHITE)
    text = Text(justify="center")
    text.append(top[0], top[1])
    text.append("\n")
    text.append(bottom[0], bottom[1])
    return text


class LocalKorry(CockpitControlWidget):
    """``pb`` (latching) and ``pb_mom`` (spring-loaded) without sim backing."""

    def render_view(self, view: ControlView) -> Text:
        return render_local_korry(view)
