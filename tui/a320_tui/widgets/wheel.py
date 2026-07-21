"""TrimWheel: continuous bidirectional control around neutral."""

from __future__ import annotations

from rich.text import Text

from a320_tui.controller import ControlView
from a320_tui.widgets.base import DARK, WHITE, CockpitControlWidget


def render_wheel(view: ControlView) -> Text:
    value = view.value if view.value is not None else 0.0
    text = Text(justify="center")
    text.append("◄ ", DARK)
    text.append(f"{value:+.2f}", WHITE)
    text.append(" ►", DARK)
    return text


class TrimWheel(CockpitControlWidget):
    def __init__(self, view: ControlView) -> None:
        super().__init__(view)
        self.styles.height = 3

    def render_view(self, view: ControlView) -> Text:
        return render_wheel(view)

    def key_enter(self) -> None:
        self._act("delta", 0.1)

    def key_right_square_bracket(self) -> None:
        self._act("delta", 0.1)

    def key_left_square_bracket(self) -> None:
        self._act("delta", -0.1)
