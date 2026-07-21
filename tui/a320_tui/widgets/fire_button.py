"""FirePushButton: the red ENG/APU fire pushbutton (push to pop out)."""

from __future__ import annotations

from rich.text import Text

from a320_tui.controller import ControlView
from a320_tui.widgets.base import DARK, RED, CockpitControlWidget


def render_fire(view: ControlView) -> Text:
    text = Text(justify="center")
    if view.popped_out:
        text.append("▲ POPPED ▲", RED)
        text.append("\n")
        text.append("squibs armed", DARK)
    else:
        # Lights-out: the FIRE light belongs to the sim, dark locally.
        text.append("FIRE", DARK)
        text.append("\n")
        text.append("· in ·", DARK)
    return text


class FirePushButton(CockpitControlWidget):
    DEFAULT_CSS = """
    FirePushButton {
        border: heavy $error;
    }
    FirePushButton:focus {
        border: heavy $accent;
    }
    """

    def render_view(self, view: ControlView) -> Text:
        return render_fire(view)
