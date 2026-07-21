"""GuardedButton: `«X»` — a korry under a red guard, two actions to press."""

from __future__ import annotations

from rich.text import Text

from a320_tui.controller import ControlView
from a320_tui.widgets.base import (
    DARK,
    RED,
    WHITE,
    CockpitControlWidget,
    render_local_korry,
)


def render_guarded(view: ControlView) -> Text:
    if not view.guard_open:
        text = Text(justify="center")
        text.append("«", RED)
        text.append(" GUARD ", DARK)
        text.append("»", RED)
        text.append("\n")
        text.append("closed", DARK)
        return text
    text = render_local_korry(view)
    text.append("\n")
    text.append("GUARD OPEN", RED)
    return text


class GuardedButton(CockpitControlWidget):
    DEFAULT_CSS = """
    GuardedButton {
        border: round $error-darken-1;
    }
    GuardedButton:focus {
        border: round $accent;
    }
    """

    def __init__(self, view: ControlView) -> None:
        super().__init__(view)
        self.styles.height = 5 if view.guard_open else 4

    def render_view(self, view: ControlView) -> Text:
        self.styles.height = 5 if view.guard_open else 4
        return render_guarded(view)

    def key_enter(self) -> None:
        if self._view.guard_open:
            self._act("press")
        else:
            self._act("open_guard")

    def key_escape(self) -> None:
        if self._view.guard_open:
            self._act("close_guard")
